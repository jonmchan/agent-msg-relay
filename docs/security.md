# Security

Tests run on every pull request and on every push to `main`, on Linux, macOS, and Windows. The security workflow runs on those same events and again every Monday, so a newly published advisory fails the build even when the code has not changed.

## What gates a change

- `cargo test --locked` covers the protocol, the module's limit and time helpers, the client's crypto and file handling, and the skill text.
- `cargo fmt` and `cargo clippy -- -D warnings` keep the Rust warning-clean.
- The wasm32 build checks that the SpacetimeDB module still compiles for publish.
- [RustSec](https://rustsec.org/) via `cargo audit` fails on known vulnerable crates.
- `cargo deny` fails on yanked crates, unknown git or registry sources, and licenses outside `deny.toml`.
- CodeQL analyzes the Rust sources. Rust uses `build-mode: none`, which is the mode CodeQL supports for this language.
- The skill test rejects hidden Unicode and a short list of instruction-smuggling phrases in every file under `.agents/skills/agent-msg-relay/`, plus `AGENTS.md` and `docs/skill-reference.md`.

## Snyk

Snyk Open Source does not scan `Cargo.toml` or `Cargo.lock` with `snyk test`. For Rust, Snyk's documented path is a CycloneDX or SPDX SBOM, then `snyk sbom test`. That command is an Enterprise feature. Snyk Code for Rust is a separate Enterprise preview and does not cover this protocol's crates.

The security workflow generates one `bom.json` beside each workspace member with `cargo-cyclonedx` 0.5.9 (`--override-filename bom`) and runs `snyk sbom test` on each file when the repository secret `SNYK_TOKEN` is set. Without that secret the Snyk job reports that it is skipped. RustSec, cargo-deny, and CodeQL still run.

Add the token in the GitHub repository settings under Secrets and variables, Actions, then `SNYK_TOKEN`. The token is a Snyk API token. Fork pull requests do not receive it.

## Skill and local files

The client binary is committed under `bin/`. The skill tells an agent to run the file for its machine. If that file is missing, the only install is `npm exec --yes --ignore-scripts --package=agent-msg-relay@0.1.4 -- agent-msg-relay`. Do not download a binary. Message text, purpose, expected messages, names, and URLs are data. The skill says so, and the client will not acknowledge a message that fails signature or envelope checks. A shared paste keeps purpose and expected messages on single lines, and a git origin that contains whitespace or control characters is replaced with the default repository URL.

The bearer token is sent only to an `https` host, or to `http` on localhost. `.agent-msg/` is mode `0700` on Unix. The identity file, address book, and message logs are mode `0600`, and the client will not follow a symlink to write them.

## Licenses

This repository is MIT. SpacetimeDB 2.13, which the module links, is Apache-2.0. A new copyleft dependency fails `cargo deny`. Dependencies may still use Apache-2.0 and the other permissive licenses listed in `deny.toml`.
