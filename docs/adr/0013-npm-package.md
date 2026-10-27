# ADR 0013: Pinned npm packages for the committed binaries

## Status

Accepted

## Context

The skill installs into another repository without `bin/`. [ADR 0011](0011-committed-binaries.md) tells that agent to stop rather than fetch a program. Most of those agents have Node and npm. They do not have a Rust toolchain, and they do not need one to run the client. A second implementation of the cryptography would drift from `crates/protocol`.

## Decision

`npm/` holds package templates at version `0.0.0`. `scripts/publish-npm.sh` stamps them with the workspace version, copies the committed binaries from `bin/`, and publishes six packages: `agent-msg-relay` and one platform package per target. The main package is a Node shim that spawns the matching binary. None of the packages has an install script. Linux packages do not set `libc`, because the musl binaries run on glibc.

The skill runs `bin/<triple>/agent-msg-relay` when that file exists. When it does not, the only install is `npm exec --yes --ignore-scripts --package=agent-msg-relay@<system version>`. The agent does not pass another version, `latest`, a URL, curl, or wget. `scripts/release.sh` rewrites those pins with the System version line. Pushing the matching `vX.Y.Z` tag publishes the packages from `.github/workflows/release.yml`. npm authenticates that workflow with a trusted publisher. The workflow requests `id-token: write` and does not use an npm token.

## Consequences

An agent in a repository without `bin/` can run this release through npm without Rust. The bytes are still the committed binaries. A later release does not add an npm command. The first publish of each package name is `scripts/publish-npm.sh --local` from an npm login, because the registry will not create a package from GitHub Actions. npm will not overwrite a version that is already published.

Each package's trusted publisher has to name `release.yml` and allow direct `npm publish`. A configuration that is not used within two days expires. Provenance is generated because the publish comes from that public workflow.
