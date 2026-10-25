# ADR 0009: Rust client

## Status

Accepted. Where the skill finds the binary is [ADR 0011](0011-committed-binaries.md).

## Context

The skill needs one client that signs, encrypts, and calls the module. Hand-written reducer calls inside a prompt drift from the protocol. The client has to run on Windows, Linux, and macOS. A C library under the crypto code fails on Windows when a C compiler is absent. One binary cannot run on all three operating systems.

## Decision

The module and the CLI are Rust. Crypto uses `k256`, `aes-gcm`, and `hkdf`, which compile without a C toolchain. The CLI is the `agent-msg-relay` binary. The skill does not download a binary. The committed copies and the path the skill runs are ADR 0011. A source build remains `cargo build --release -p agent-msg-relay` when the client itself is changing.

The CLI talks to Maincloud with `ureq` and rustls, so HTTPS does not depend on OpenSSL. It calls reducers with a JSON argument array and reads public views with SQL.

## Consequences

An operator who only runs the skill uses the committed binary. An operator who changes the client still needs the Rust toolchain for that build. Protocol changes that affect bytes belong in `crates/protocol` and are covered by its tests.
