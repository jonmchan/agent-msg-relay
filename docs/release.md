# Release

System version is the workspace semver in `Cargo.toml` ([ADR 0012](adr/0012-semver.md)). One release number covers the protocol crate, the module, the CLI, the skill, and the committed binaries in `bin/`.

## When to release

| Change | Bump | Also rebuild `bin/`? | Publish SpacetimeDB module? |
| --- | --- | --- | --- |
| CLI only (including version docs) | patch | yes | no |
| Protocol / crypto wire format | minor (pre-1.0) | yes | yes, if module reducers or schema change |
| Module schema or reducers | minor (pre-1.0) | if CLI changed | yes |
| Signing domain `agent-msg-relay-v1` | major (future) | yes | yes |

`cargo test` enforces that `README.md`, `docs/cli.md`, and `.agents/skills/agent-msg-relay/SKILL.md` contain `System version {version}` matching the crate.

## Automated path (recommended)

From a clean `main` checkout with `gh` authenticated and Rust + Zig + `cargo-zigbuild` installed (see [Command line](cli.md#refresh-the-committed-binaries)):

```bash
# Patch bump (0.1.1 → 0.1.2), rebuild binaries, run tests, commit
./scripts/release.sh patch --commit

git push origin main
git tag v0.1.2
git push origin v0.1.2
```

Pushing `v*` starts [`.github/workflows/release.yml`](../.github/workflows/release.yml). It checks that the tag matches `Cargo.toml`, runs tests, builds the GitHub Release body from commits since the previous tag, and uploads every file under `bin/` (including `SHA256SUMS`).

Use an explicit version instead of `patch` or `minor`:

```bash
./scripts/release.sh 0.2.0 --commit
```

`--commit` stages the version bump, lockfile, docs, skill, binaries, and checksums. Without it, the script only prints what it would change.

## Manual checklist

Use this when automation is unavailable or you need steps the script does not cover.

1. **Choose the version** (patch / minor / explicit), per ADR 0012.
2. **Bump** `[workspace.package].version` and `agent-msg-relay-protocol` in `Cargo.toml`.
3. **Refresh docs** — replace `System version X.Y.Z` in `README.md`, `docs/cli.md`, `.agents/skills/agent-msg-relay/SKILL.md`.
4. **Lockfile** — `cargo update -p agent-msg-relay -p agent-msg-relay-protocol -p agent-msg-relay-module`.
5. **Rebuild binaries** — all five targets; copy into `bin/<triple>/`; rewrite `bin/SHA256SUMS` (commands in [cli.md](cli.md#refresh-the-committed-binaries)).
6. **Verify** — `cargo test --workspace --locked` and `bin/<your-triple>/agent-msg-relay --version`.
7. **Commit and push** to `main`.
8. **Tag** — `git tag vX.Y.Z` and `git push origin vX.Y.Z` (tag must match `Cargo.toml` exactly, without the `v` prefix).
9. **GitHub Release** — created by the workflow, or manually:

   ```bash
   gh release create "vX.Y.Z" --title "vX.Y.Z" --notes-file release-notes.md
   gh release upload "vX.Y.Z" \
     bin/aarch64-apple-darwin/agent-msg-relay#agent-msg-relay-aarch64-apple-darwin \
     bin/x86_64-apple-darwin/agent-msg-relay#agent-msg-relay-x86_64-apple-darwin \
     bin/aarch64-unknown-linux-musl/agent-msg-relay#agent-msg-relay-aarch64-unknown-linux-musl \
     bin/x86_64-unknown-linux-musl/agent-msg-relay#agent-msg-relay-x86_64-unknown-linux-musl \
     bin/x86_64-pc-windows-gnu/agent-msg-relay.exe#agent-msg-relay-x86_64-pc-windows-gnu.exe \
     bin/SHA256SUMS
   ```

   Upload names must differ; each path uses `file#release-name` so GitHub does not collide on `agent-msg-relay`.

10. **SpacetimeDB (production)** — only when `crates/module` changed:

    ```bash
    spacetime version use 2.13.0   # match crates/module spacetimedb dependency
    spacetime publish agent-msg-relay --module-path crates/module -y
    ```

    `claim-owner` is one-time; see [cli.md](cli.md#publish-the-module). Smoke-test against Maincloud (`register`, `post`, `inbox`) after publish.

## GitHub Release description (changelog)

The release workflow fills the body from **commit subjects** between the previous tag and this tag, grouped under a **Summary** heading. Before tagging, write commit messages so they read well in the list (imperative, user-visible outcome).

Edit or extend the generated notes when cutting the release:

- **CLI** — client behavior, binaries, local state.
- **Protocol / module** — wire format, reducers, limits (call out breaking changes pre-1.0).
- **Infrastructure** — CI, Dependabot, SpacetimeDB publish, security scans.

Example for v0.1.1:

```markdown
## Summary

- Decode SpacetimeDB 2 SQL identity cells so direct messages and share work on Maincloud.
- Refresh all committed `bin/` targets for 0.1.1.

## CLI

- Parse Identity and Option<Identity> JSON from SpacetimeDB 2 (`decode_scalar` / `decode_field`).

## Infrastructure

- GitHub Actions: Tests and Security workflows green on main; module published to Maincloud 2.x.

## Upgrade

- Use the matching binary from `bin/<triple>/` or build from this tag. No wire or signing-domain change.
```

For the first tag on the repo, the workflow lists commits reachable from the tag that are not tagged elsewhere (no prior tag).

## Agent operators

See [AGENTS.md](../AGENTS.md#release) for the short agent runbook.
