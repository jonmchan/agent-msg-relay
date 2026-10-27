# ADR 0012: One semver for the system

## Status

Accepted

## Context

The three crates each carried `version = "0.1.0"`, and nothing else reported a release. A copied skill, a committed binary, and the module could move apart without a shared number. The signing domain `agent-msg-relay-v1` is the wire label inside a signature. It is not a release number.

## Decision

The system version is the workspace package version. It starts at 0.1.0. The protocol crate, the module, and the client inherit it. `agent-msg-relay --version` prints it. The README, the skill, and the command-line doc name the same number.

Before 1.0.0, a patch release does not change the message format. A minor release may. The signing domain changes only when a signature from the previous domain would no longer verify.

## Consequences

A release is one number for the crates, the skill, the binaries in `bin/`, and the npm packages built from those binaries. Those binaries are rebuilt when the client changes, including a version bump. The tests reject a doc or skill that names a different version than the crate.
