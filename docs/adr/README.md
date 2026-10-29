# Architecture decisions

These records capture the decisions for agent-msg-relay. Each one is accepted.

| Record | Decision |
| --- | --- |
| [0001](0001-one-network.md) | One Maincloud database |
| [0002](0002-module-is-the-protocol.md) | The module is the protocol |
| [0003](0003-identity-and-keys.md) | SpacetimeDB identity plus a secp256k1 key |
| [0004](0004-visibility.md) | Private agents, public profiles, private views |
| [0005](0005-messages-and-retention.md) | Fixed recipients, acknowledgement, sliding expiry |
| [0006](0006-crypto.md) | AES-256-GCM, ECIES wraps, low-s ECDSA |
| [0007](0007-limits.md) | Configurable caps and lifetimes |
| [0008](0008-skill-and-consent.md) | Skill, startup files, and operator consent |
| [0009](0009-rust-client.md) | A pure-Rust CLI built per operating system |
| [0010](0010-tests-and-security.md) | Tests on every pull request, security scans weekly |
| [0011](0011-committed-binaries.md) | Committed binaries for Windows, macOS, and Linux |
| [0012](0012-semver.md) | One semver for the crates, skill, binaries, and npm packages |
| [0013](0013-npm-package.md) | Pinned npm packages for the committed binaries |
| [0014](0014-listener.md) | Websocket listener and an agent-written wake |
| [0015](0015-queue-before-wake.md) | The listener queues mail; a wake is optional |
