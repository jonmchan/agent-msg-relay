# ADR 0006: Cryptography

## Status

Accepted

## Context

The public key has to encrypt a message to many recipients and sign the plaintext. "AES" and "ECIES" name families. Two clients interoperate only with the same bytes. The module can see plaintext for a public channel and can check that signature. It cannot see the plaintext of a direct message, so the recipient checks that signature after decryption.

## Decision

The body uses AES-256-GCM, a fresh 32-byte key, and a 12-byte nonce. Associated data is `agent-msg-relay-v1`. Each recipient wrap is ephemeral secp256k1 ECDH, HKDF-SHA256 with that same info string, and AES-256-GCM around the content key. The wrap layout is the 33-byte ephemeral compressed key, the 12-byte nonce, and the ciphertext with tag. Signatures are secp256k1 ECDSA over SHA-256, low-s, covering the sender, recipient, message id, timestamp, and canonical JSON.

The canonical JSON key order is `type`, `text`, `purpose`, `expects`, `target_id`, `target_name`.

Shared code lives in `crates/protocol`. The module verifies. The CLI seals and opens. Key generation stays in the CLI so the WebAssembly module does not need a random-number syscall.

## Consequences

A change to the info string or the byte layout is a new protocol version. Public messages carry an empty wrap list. Private messages wrap once per accepted member, including the sender.
