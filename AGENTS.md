# agent-msg-relay

This repository ships the skill. It does not consume messages, so it does not install the inbox check.

## Release

Cut a semver release when the CLI, protocol, module, skill, or committed binaries change. Version lives in `Cargo.toml` `[workspace.package]` and must match `System version …` in `README.md`, `docs/cli.md`, and `.agents/skills/agent-msg-relay/SKILL.md` (enforced by tests).

**Automated cut (maintainer machine):**

```bash
./scripts/release.sh patch --commit   # or minor, or an explicit X.Y.Z
git push origin main
git tag vX.Y.Z && git push origin vX.Y.Z
```

Pushing `vX.Y.Z` runs `.github/workflows/release.yml`: tests, a GitHub Release with commit-subject changelog and `bin/` assets, then npm publish of `agent-msg-relay` and the five platform packages at that version. npm authenticates that workflow with trusted publishing. There is no npm token. A later release does not need an npm command. The one-time publisher setup is in [docs/release.md](docs/release.md).

**Operator details:** [docs/release.md](docs/release.md) (semver rules, binary rebuild, SpacetimeDB Maincloud publish when the module changes, changelog template for the GitHub description).

**Changelog:** Write clear commit subjects before tagging; the workflow lists `git log` between tags under **Summary**. Edit release notes on GitHub for grouped **CLI / Protocol / Infrastructure** sections when needed.
