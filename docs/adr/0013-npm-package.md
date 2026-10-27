# ADR 0013: Pinned npm packages for the committed binaries

## Status

Accepted

## Context

The skill installs into another repository without `bin/`. [ADR 0011](0011-committed-binaries.md) tells that agent to stop rather than fetch a program. Most of those agents have Node and npm. They do not have a Rust toolchain, and they do not need one to run the client. A second implementation of the cryptography would drift from `crates/protocol`.

## Decision

`npm/` holds package templates at version `0.0.0`. `scripts/publish-npm.sh` stamps them with the workspace version, copies the committed binaries from `bin/`, and publishes six packages: `agent-msg-relay` and one platform package per target. The main package is a Node shim that spawns the matching binary. None of the packages has an install script. Linux packages do not set `libc`, because the musl binaries run on glibc.

The skill runs `bin/<triple>/agent-msg-relay` when that file exists. When it does not, the only install is `npm exec --yes --ignore-scripts --package=agent-msg-relay@<system version>`. The agent does not pass another version, `latest`, a URL, curl, or wget. `scripts/release.sh` rewrites those pins with the System version line. Pushing the matching `vX.Y.Z` tag publishes the packages from the workflow. The `NPM_TOKEN` secret is required, and a missing secret fails the job.

## Consequences

An agent in a repository without `bin/` can run this release through npm without Rust. The bytes are still the committed binaries. A later release does not add an npm command. The first publish of a version that was tagged before this workflow needs one `workflow_dispatch`. npm will not overwrite a version that is already published.

A granular token cannot be scoped to a package name until that package exists. The first publish uses a token allowed to publish every package on the account. After the six packages exist, that token is revoked and `NPM_TOKEN` is replaced with one scoped to those names.
