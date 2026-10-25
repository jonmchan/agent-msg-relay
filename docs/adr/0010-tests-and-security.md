# ADR 0010: Tests and security scans

## Status

Accepted

## Context

The client holds a bearer token and a secp256k1 private key, and the skill is loaded into an agent's prompt. A vulnerable crate, a loosened file mode, or a skill edit that treats inbox text as instructions would ship to every repo that installs the skill. Snyk is a familiar scanner, and its current Rust documentation says `snyk test` does not read Cargo manifests. Rust SBOM testing is an Enterprise feature.

## Decision

Pull requests and pushes to `main` run formatting, Clippy, `cargo test --locked` on Linux, macOS, and Windows, and a wasm32 module build.

The security workflow runs on those events and every Monday. It is gated by RustSec (`cargo audit`), `cargo deny`, and CodeQL for Rust with `build-mode: none`. `cargo test` also checks the skill and startup files for hidden Unicode and for phrases that would pipe a shell installer or tell the agent to ignore its instructions.

When `SNYK_TOKEN` is set, the same workflow builds a CycloneDX SBOM with `cargo-cyclonedx` 0.5.9 and runs `snyk sbom test`. The other scanners stay required either way. Dependabot opens weekly updates for Cargo and GitHub Actions.

## Consequences

A new advisory fails the Monday run without a code change. Snyk does not scan this repo until an owner adds `SNYK_TOKEN`, and a free Snyk plan may reject `snyk sbom test`. The badges on the README point at the test workflow and the security workflow.

SpacetimeDB 2.13 is Apache-2.0. `deny.toml` denies copyleft licenses for this MIT repository.
