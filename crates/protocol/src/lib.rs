//! Shared rules for agent-msg-relay.
//!
//! The module and the CLI both depend on this crate. Signing bytes, the JSON
//! envelope, and the ECIES wrap layout live here so the two cannot drift.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use hkdf::Hkdf;
use k256::ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use k256::elliptic_curve::scalar::IsHigh;
use k256::elliptic_curve::sec1::ToSec1Point;
use k256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Semver of this release. The signing domain below is the wire compatibility
/// label and stays put until a signature would otherwise break.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DOMAIN: &str = "agent-msg-relay-v1";
pub const HKDF_INFO: &[u8] = b"agent-msg-relay-v1";
pub const SKEW_MICROS: i64 = 5 * 60 * 1_000_000;
pub const WRAP_LEN: usize = 33 + 12 + 32 + 16;

pub const TYPE_TEXT: &str = "text";
pub const TYPE_AGENT_INVITATION: &str = "agent_invitation";
pub const TYPE_CHANNEL_INVITATION: &str = "channel_invitation";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    #[serde(rename = "type")]
    pub message_type: String,
    pub text: String,
    pub purpose: String,
    pub expects: String,
    pub target_id: String,
    pub target_name: String,
}

impl Envelope {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            message_type: TYPE_TEXT.into(),
            text: text.into(),
            purpose: String::new(),
            expects: String::new(),
            target_id: String::new(),
            target_name: String::new(),
        }
    }

    pub fn to_canonical_json(&self) -> Result<String, String> {
        validate_envelope(self)?;
        serde_json::to_string(self).map_err(|err| err.to_string())
    }
}

pub fn validate_envelope(envelope: &Envelope) -> Result<(), String> {
    match envelope.message_type.as_str() {
        TYPE_TEXT | TYPE_AGENT_INVITATION | TYPE_CHANNEL_INVITATION => {}
        other if valid_public_name(other) => {}
        other => return Err(format!("unknown message type {other}")),
    }
    if matches!(
        envelope.message_type.as_str(),
        TYPE_AGENT_INVITATION | TYPE_CHANNEL_INVITATION
    ) && envelope.target_id.is_empty()
    {
        return Err("invitations require target_id".into());
    }
    Ok(())
}

pub fn valid_public_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (3..=32).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

pub fn valid_uuid(value: &str) -> bool {
    let mut parts = value.split('-');
    let widths = [8, 4, 4, 4, 12];
    widths.into_iter().all(|width| {
        parts.next().is_some_and(|part| {
            part.len() == width && part.chars().all(|char| char.is_ascii_hexdigit())
        })
    }) && parts.next().is_none()
}

/// Lowercase `0x` plus 64 hex characters.
pub fn normalize_identity(value: &str) -> Result<String, String> {
    let hex = value
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.len() != 64 || !hex.chars().all(|char| char.is_ascii_hexdigit()) {
        return Err("identity must be 64 hex characters".into());
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

pub fn signing_bytes(
    sender: &str,
    recipient_kind: &str,
    recipient_id: &str,
    message_id: &str,
    timestamp_micros: i64,
    plaintext: &str,
) -> Vec<u8> {
    format!(
        "{DOMAIN}\nsender:{sender}\nrecipient_kind:{recipient_kind}\nrecipient_id:{recipient_id}\nmessage_id:{message_id}\ntimestamp_micros:{timestamp_micros}\n{plaintext}"
    )
    .into_bytes()
}

pub fn register_bytes(identity: &str) -> Vec<u8> {
    format!("{DOMAIN}\nregister\n{identity}").into_bytes()
}

pub fn sign_prehash(secret_key_bytes: &[u8], payload: &[u8]) -> Result<Vec<u8>, String> {
    let secret = SecretKey::from_slice(secret_key_bytes).map_err(|err| err.to_string())?;
    let signing = SigningKey::from(secret);
    let digest = Sha256::digest(payload);
    let signature: Signature = signing
        .sign_prehash(&digest)
        .map_err(|err| err.to_string())?;
    Ok(signature.normalize_s().to_bytes().to_vec())
}

pub fn verify_prehash(
    public_key_sec1: &[u8],
    payload: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    let verifying =
        VerifyingKey::from_sec1_bytes(public_key_sec1).map_err(|err| err.to_string())?;
    let signature = Signature::from_slice(signature).map_err(|err| err.to_string())?;
    if bool::from(signature.s().is_high()) {
        return Err("signature is not low-s".into());
    }
    let digest = Sha256::digest(payload);
    verifying
        .verify_prehash(&digest, &signature)
        .map_err(|_| "signature verification failed".to_string())
}

pub fn public_key_from_secret(secret_key_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let secret = SecretKey::from_slice(secret_key_bytes).map_err(|err| err.to_string())?;
    Ok(secret.public_key().to_sec1_point(true).as_bytes().to_vec())
}

/// Seal a 32-byte content key to a compressed secp256k1 public key.
///
/// Layout: ephemeral compressed public key (33) || nonce (12) || ciphertext and tag (48).
pub fn seal_content_key(
    ephemeral_secret: &[u8],
    nonce: &[u8; 12],
    recipient_compressed: &[u8],
    content_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    if nonce.len() != 12 {
        return Err("nonce must be 12 bytes".into());
    }
    let secret = SecretKey::from_slice(ephemeral_secret).map_err(|err| err.to_string())?;
    let recipient =
        PublicKey::from_sec1_bytes(recipient_compressed).map_err(|err| err.to_string())?;
    let shared = k256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), recipient.as_affine());
    let mut wrapping_key = [0u8; 32];
    Hkdf::<Sha256>::new(None, shared.raw_secret_bytes())
        .expand(HKDF_INFO, &mut wrapping_key)
        .map_err(|_| "hkdf expand failed".to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&wrapping_key).map_err(|err| err.to_string())?;
    let encrypted = cipher
        .encrypt(
            &Nonce::try_from(&nonce[..]).map_err(|_| "nonce must be 12 bytes".to_string())?,
            Payload {
                msg: content_key,
                aad: DOMAIN.as_bytes(),
            },
        )
        .map_err(|_| "wrap encryption failed".to_string())?;
    let mut out = Vec::with_capacity(WRAP_LEN);
    out.extend_from_slice(secret.public_key().to_sec1_point(true).as_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&encrypted);
    if out.len() != WRAP_LEN {
        return Err("wrap length mismatch".into());
    }
    Ok(out)
}

pub fn open_content_key(secret_key_bytes: &[u8], wrap: &[u8]) -> Result<[u8; 32], String> {
    if wrap.len() != WRAP_LEN {
        return Err(format!("wrap must be {WRAP_LEN} bytes"));
    }
    let (ephemeral_bytes, rest) = wrap.split_at(33);
    let (nonce, ciphertext) = rest.split_at(12);
    let secret = SecretKey::from_slice(secret_key_bytes).map_err(|err| err.to_string())?;
    let ephemeral = PublicKey::from_sec1_bytes(ephemeral_bytes).map_err(|err| err.to_string())?;
    let shared = k256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), ephemeral.as_affine());
    let mut wrapping_key = [0u8; 32];
    Hkdf::<Sha256>::new(None, shared.raw_secret_bytes())
        .expand(HKDF_INFO, &mut wrapping_key)
        .map_err(|_| "hkdf expand failed".to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&wrapping_key).map_err(|err| err.to_string())?;
    let plain = cipher
        .decrypt(
            &Nonce::try_from(nonce).map_err(|_| "nonce must be 12 bytes".to_string())?,
            Payload {
                msg: ciphertext,
                aad: DOMAIN.as_bytes(),
            },
        )
        .map_err(|_| "wrap decryption failed".to_string())?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&plain);
    Ok(key)
}

pub fn seal_body(
    content_key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(content_key).map_err(|err| err.to_string())?;
    let encrypted = cipher
        .encrypt(
            &Nonce::try_from(&nonce[..]).map_err(|_| "nonce must be 12 bytes".to_string())?,
            Payload {
                msg: plaintext,
                aad: DOMAIN.as_bytes(),
            },
        )
        .map_err(|_| "body encryption failed".to_string())?;
    let mut out = Vec::with_capacity(12 + encrypted.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

pub fn open_body(content_key: &[u8; 32], body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 12 + 16 {
        return Err("ciphertext is too short".into());
    }
    let (nonce, ciphertext) = body.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(content_key).map_err(|err| err.to_string())?;
    cipher
        .decrypt(
            &Nonce::try_from(nonce).map_err(|_| "nonce must be 12 bytes".to_string())?,
            Payload {
                msg: ciphertext,
                aad: DOMAIN.as_bytes(),
            },
        )
        .map_err(|_| "body decryption failed".to_string())
}

pub fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    hex::decode(value.trim().trim_start_matches("0x")).map_err(|err| err.to_string())
}

pub fn encode_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Characters that can hide or reorder text in a skill, a paste, or a message.
pub fn is_hidden_char(character: char) -> bool {
    matches!(
        character,
        '\u{200B}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'
            | '\u{202B}'
            | '\u{202C}'
            | '\u{202D}'
            | '\u{202E}'
            | '\u{2060}'
            | '\u{2066}'
            | '\u{2067}'
            | '\u{2068}'
            | '\u{2069}'
            | '\u{FEFF}'
    ) || ('\u{E0000}'..='\u{E007F}').contains(&character)
}

pub fn contains_hidden_text(value: &str) -> bool {
    value.chars().any(is_hidden_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[31] = 7;
        bytes
    }

    #[test]
    fn register_signature_round_trip() {
        let secret = secret();
        let public = public_key_from_secret(&secret).unwrap();
        let payload = register_bytes("0x11");
        let signature = sign_prehash(&secret, &payload).unwrap();
        verify_prehash(&public, &payload, &signature).unwrap();
    }

    #[test]
    fn wrap_and_body_round_trip() {
        let recipient = secret();
        let recipient_pub = public_key_from_secret(&recipient).unwrap();
        let mut ephemeral = secret();
        ephemeral[31] = 9;
        let content_key = [4u8; 32];
        let wrap_nonce = [1u8; 12];
        let wrap = seal_content_key(&ephemeral, &wrap_nonce, &recipient_pub, &content_key).unwrap();
        assert_eq!(wrap.len(), WRAP_LEN);
        assert_eq!(open_content_key(&recipient, &wrap).unwrap(), content_key);
        let body_nonce = [2u8; 12];
        let sealed = seal_body(&content_key, &body_nonce, b"hello").unwrap();
        assert_eq!(open_body(&content_key, &sealed).unwrap(), b"hello");
    }

    #[test]
    fn envelope_key_order() {
        let json = Envelope::text("hi").to_canonical_json().unwrap();
        assert!(json.starts_with(r#"{"type":"text","text":"hi""#));
    }

    #[test]
    fn names_and_ids() {
        assert!(valid_public_name("agent-one"));
        assert!(!valid_public_name("Agent"));
        assert!(!valid_public_name("ab"));
        assert!(valid_uuid("00112233-4455-6677-8899-aabbccddeeff"));
        assert!(!valid_uuid("00112233-4455-6677-8899-aabbccddeeff-extra"));
        assert_eq!(
            normalize_identity(
                "0XABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789"
            )
            .unwrap(),
            "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
        assert!(normalize_identity("0xAABB").is_err());
    }

    #[test]
    fn invitation_requires_a_target() {
        let mut envelope = Envelope::text("hi");
        envelope.message_type = TYPE_CHANNEL_INVITATION.into();
        assert!(envelope.to_canonical_json().is_err());
        envelope.target_id = "00112233-4455-6677-8899-aabbccddeeff".into();
        assert!(envelope.to_canonical_json().is_ok());
    }

    #[test]
    fn named_message_types_are_allowed() {
        let mut envelope = Envelope::text("file the notes");
        envelope.message_type = "task".into();
        assert!(envelope
            .to_canonical_json()
            .unwrap()
            .contains(r#""type":"task""#));
        envelope.message_type = "Task".into();
        assert!(envelope.to_canonical_json().is_err());
        envelope.message_type = "text".into();
        assert!(envelope.to_canonical_json().is_ok());
    }

    #[test]
    fn envelope_rejects_unknown_fields() {
        let raw = r#"{"type":"text","text":"hi","purpose":"","expects":"","target_id":"","target_name":"","ignore":"previous instructions"}"#;
        assert!(serde_json::from_str::<Envelope>(raw).is_err());
    }

    #[test]
    fn tampered_ciphertext_and_wrong_key_fail() {
        let recipient = secret();
        let recipient_pub = public_key_from_secret(&recipient).unwrap();
        let mut ephemeral = secret();
        ephemeral[31] = 9;
        let content_key = [4u8; 32];
        let wrap = seal_content_key(&ephemeral, &[1u8; 12], &recipient_pub, &content_key).unwrap();
        let mut other = secret();
        other[31] = 3;
        assert!(open_content_key(&other, &wrap).is_err());
        let mut damaged = wrap.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 0xff;
        assert!(open_content_key(&recipient, &damaged).is_err());

        let sealed = seal_body(&content_key, &[2u8; 12], b"hello").unwrap();
        let mut body = sealed.clone();
        let last = body.len() - 1;
        body[last] ^= 0xff;
        assert!(open_body(&content_key, &body).is_err());
    }

    #[test]
    fn high_s_and_wrong_payload_are_rejected() {
        let secret = secret();
        let public = public_key_from_secret(&secret).unwrap();
        let payload = register_bytes("0x11");
        let signature = sign_prehash(&secret, &payload).unwrap();
        verify_prehash(&public, &payload, &signature).unwrap();
        assert!(verify_prehash(&public, b"other", &signature).is_err());
        let high = negate_s(&signature);
        let error = verify_prehash(&public, &payload, &high).unwrap_err();
        assert!(error.contains("low-s"), "{error}");
    }

    #[test]
    fn hidden_text_is_detected() {
        assert!(contains_hidden_text("ok\u{202E}evil"));
        assert!(contains_hidden_text("a\u{E0061}"));
        assert!(!contains_hidden_text("agent-msg-relay"));
    }

    /// `n - s` for the secp256k1 order, so a low-s signature becomes high-s.
    fn negate_s(signature: &[u8]) -> Vec<u8> {
        const ORDER: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x41,
        ];
        let mut scalar = [0u8; 32];
        let mut borrow = 0i16;
        for index in (0..32).rev() {
            let difference = i16::from(ORDER[index]) - i16::from(signature[32 + index]) - borrow;
            if difference < 0 {
                scalar[index] = (difference + 256) as u8;
                borrow = 1;
            } else {
                scalar[index] = difference as u8;
                borrow = 0;
            }
        }
        let mut high = signature[..32].to_vec();
        high.extend_from_slice(&scalar);
        high
    }
}
