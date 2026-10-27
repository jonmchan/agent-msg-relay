# Release

System version is the workspace semver in `Cargo.toml` ([ADR 0012](adr/0012-semver.md)). One release number covers the protocol crate, the module, the CLI, the skill, the committed binaries in `bin/`, and the npm packages.

## When to release

| Change | Bump | Also rebuild `bin/`? | Publish SpacetimeDB module? |
| --- | --- | --- | --- |
| CLI only (including version docs) | patch | yes | no |
| Protocol / crypto wire format | minor (pre-1.0) | yes | yes, if module reducers or schema change |
| Module schema or reducers | minor (pre-1.0) | if CLI changed | yes |
| Signing domain `agent-msg-relay-v1` | major (future) | yes | yes |

`cargo test` enforces that `README.md`, `docs/cli.md`, and `.agents/skills/agent-msg-relay/SKILL.md` contain `System version {version}` matching the crate. It also enforces that every install doc pins `agent-msg-relay@{version}`.

## Automated path (recommended)

From a clean `main` checkout with `gh` authenticated and Rust + Zig + `cargo-zigbuild` installed (see [Command line](cli.md#refresh-the-committed-binaries)):

```bash
# Patch bump (0.1.1 → 0.1.2), rebuild binaries, run tests, commit
./scripts/release.sh patch --commit

git push origin main
git tag v0.1.2
git push origin v0.1.2
```

Pushing `v*` starts [`.github/workflows/release.yml`](../.github/workflows/release.yml). It checks that the tag matches `Cargo.toml`, runs tests, builds the GitHub Release body from commits since the previous tag, uploads every file under `bin/` (including `SHA256SUMS`), and publishes the npm packages at that version. `scripts/release.sh` rewrites every `agent-msg-relay@X.Y.Z` pin. The package templates in `npm/` stay at `0.0.0`. There is no separate npm command on a later release.

The workflow reads the `NPM_TOKEN` secret. A missing secret fails the job, so a tag cannot ship a skill pin that npm does not have. Tag immediately after pushing `main`. The pin on `main` matches the package only after that publish finishes.

```bash
npm exec --yes --ignore-scripts --package=agent-msg-relay@0.1.4 -- agent-msg-relay
```

That is the install command for the published version. Do not download a binary.

Use an explicit version instead of `patch` or `minor`:

```bash
./scripts/release.sh 0.2.0 --commit
```

`--commit` stages the version bump, lockfile, docs, skill, binaries, and checksums. Without it, the script only prints what it would change.

## npm token (once)

Later tags publish on their own. This setup happens once. `v0.1.4` already created a GitHub Release before the workflow published to npm, so the first npm publish is a dispatch after this workflow is on `main`.

npm's package picker only lists packages that already exist. These six names are not on the registry yet, so the first token has to be allowed to publish every package on the account. After that publish creates them, replace it with a token scoped to those names.

1. Create an account at [npmjs.com](https://www.npmjs.com/) if needed. The unscoped package name does not require the username `jonmchan`.
2. Generate a granular access token. Set Packages and scopes to **Read and write** (publish and stage), and select **All packages**. Leave Organizations at no access. Allow the token to bypass two-factor authentication. Choose the shortest expiration the form allows. Do not paste the token into chat or pass it as a command argument.
3. Add the token as the Actions secret `NPM_TOKEN` on `jonmchan/agent-msg-relay`.
4. From the Actions tab, run the Release workflow with `workflow_dispatch` on `main`. That run publishes the workspace version. It does not create another GitHub Release.
5. Confirm `npm view agent-msg-relay@0.1.4 version` prints that same version, and that each of the five platform packages exists at that version.
6. Revoke the all-packages token. Generate a new granular token with the same write permission and bypass-2FA setting, and select only `agent-msg-relay`, `agent-msg-relay-darwin-arm64`, `agent-msg-relay-darwin-x64`, `agent-msg-relay-linux-arm64`, `agent-msg-relay-linux-x64`, and `agent-msg-relay-win32-x64`. Replace the `NPM_TOKEN` secret with that token. Later releases use the scoped token.

## Manual checklist

Use this when automation is unavailable or you need steps the script does not cover.

1. **Choose the version** (patch / minor / explicit), per ADR 0012.
2. **Bump** `[workspace.package].version` and `agent-msg-relay-protocol` in `Cargo.toml`.
3. **Refresh docs** — replace `System version X.Y.Z` in `README.md`, `docs/cli.md`, `.agents/skills/agent-msg-relay/SKILL.md`. Replace every `agent-msg-relay@X.Y.Z` pin with the same version. `scripts/release.sh` does both.
4. **Lockfile** — `cargo update -p agent-msg-relay -p agent-msg-relay-protocol -p agent-msg-relay-module`.
5. **Rebuild binaries** — all five targets; copy into `bin/<triple>/`; rewrite `bin/SHA256SUMS` (commands in [cli.md](cli.md#refresh-the-committed-binaries)).
6. **Verify** — `cargo test --workspace --locked` and `bin/<your-triple>/agent-msg-relay --version`.
7. **Commit and push** to `main`.
8. **Tag** — `git tag vX.Y.Z` and `git push origin vX.Y.Z` (tag must match `Cargo.toml` exactly, without the `v` prefix). The workflow publishes the npm packages after tests pass. `NPM_TOKEN` must already be set.
9. **GitHub Release** — created by the workflow, or manually:

   ```bash
   gh release create "vX.Y.Z" --title "vX.Y.Z" --notes-file release-notes.md
   ASSET_DIR="$(mktemp -d)"
   cp bin/aarch64-apple-darwin/agent-msg-relay "${ASSET_DIR}/agent-msg-relay-aarch64-apple-darwin"
   cp bin/x86_64-apple-darwin/agent-msg-relay "${ASSET_DIR}/agent-msg-relay-x86_64-apple-darwin"
   cp bin/aarch64-unknown-linux-musl/agent-msg-relay "${ASSET_DIR}/agent-msg-relay-aarch64-unknown-linux-musl"
   cp bin/x86_64-unknown-linux-musl/agent-msg-relay "${ASSET_DIR}/agent-msg-relay-x86_64-unknown-linux-musl"
   cp bin/x86_64-pc-windows-gnu/agent-msg-relay.exe "${ASSET_DIR}/agent-msg-relay-x86_64-pc-windows-gnu.exe"
   cp bin/SHA256SUMS "${ASSET_DIR}/SHA256SUMS"
   gh release upload "vX.Y.Z" "${ASSET_DIR}/"* --clobber
   ```

   Copy into a temp directory so each upload basename is unique (`agent-msg-relay` alone would collide).

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

The workflow lists commits under **Summary**. For user-facing releases, edit the GitHub description (or `gh release edit --notes-file`) into sections such as **CLI**, **Protocol / module**, **Infrastructure**, and **Upgrade**, as in the [v0.1.1 release](https://github.com/jonmchan/agent-msg-relay/releases/tag/v0.1.1).

For the first tag on the repo, the workflow lists commits reachable from the tag that are not tagged elsewhere (no prior tag).

## Agent operators

See [AGENTS.md](../AGENTS.md#release) for the short agent runbook.
