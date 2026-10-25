# ADR 0011: Committed client binaries

## Status

Accepted

## Context

The skill told every other repository to install Rustup and compile the client. That work is the same release binary on each operating system, and most agents only need to run it. One file still cannot run on Windows, Linux, and macOS, or on both CPU architectures of one operating system. Downloading a binary from outside this repository would give a copied skill a way to fetch a program of its own.

## Decision

Release binaries live in `bin/<target>/` and are rebuilt when the client changes. The skill runs the file for the current machine and does not download a binary. Copying the skill into another repository includes copying `bin/` to that repository's root.

The committed targets are `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, and `x86_64-pc-windows-gnu`. Linux is musl so the file does not depend on a distribution libc. Windows is the GNU target because that linker runs on macOS, and the executable links only libraries that ship with Windows. `bin/SHA256SUMS` records the SHA-256 of each file.

## Consequences

A repository that installs the skill can run the client without Rust. The files are stale until someone rebuilds them after a client change. Windows on ARM and any other target not listed still need a source build, and only when the operator asks for one.
