#!/usr/bin/env bash
# Bump semver, sync version strings, rebuild committed CLI binaries, run tests.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

COMMIT=0
BUMP=""
VERSION=""

usage() {
  cat <<'EOF'
Usage: scripts/release.sh <patch|minor|VERSION> [--commit]

  patch|minor   Bump from current [workspace.package].version in Cargo.toml
  VERSION       Explicit semver (e.g. 0.1.2)

  --commit      Git add and commit release artifacts (does not push or tag)

Requires: rustup, cargo-zigbuild, zig (see docs/cli.md).
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --commit) COMMIT=1; shift ;;
    patch|minor)
      if [[ -n "$BUMP" || -n "$VERSION" ]]; then
        echo "Specify only one of patch, minor, or an explicit version." >&2
        exit 2
      fi
      BUMP="$1"
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *)
      if [[ -n "$VERSION" ]]; then
        echo "Unexpected argument: $1" >&2
        usage
        exit 2
      fi
      VERSION="$1"
      shift
      ;;
  esac
done

if [[ -z "$BUMP" && -z "$VERSION" ]]; then
  usage
  exit 2
fi

current_version() {
  awk '/^\[workspace.package\]/{found=1} found && /^version = /{gsub(/version = "|"/,""); print; exit}' Cargo.toml
}

bump_version() {
  local current="$1" kind="$2"
  IFS=. read -r major minor patch <<<"${current}"
  case "$kind" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    *) echo "unknown bump $kind" >&2; exit 2 ;;
  esac
  echo "${major}.${minor}.${patch}"
}

CURRENT="$(current_version)"
if [[ -n "$BUMP" ]]; then
  VERSION="$(bump_version "$CURRENT" "$BUMP")"
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid version: $VERSION" >&2
  exit 2
fi

echo "Release $CURRENT -> $VERSION"

marker="System version ${VERSION}"

perl -i -pe "s/^version = \"\\d+\\.\\d+\\.\\d+\"/version = \"$VERSION\"/" Cargo.toml
perl -i -pe "s/agent-msg-relay-protocol = \\{ version = \"\\d+\\.\\d+\\.\\d+\"/agent-msg-relay-protocol = { version = \"$VERSION\"/" Cargo.toml

for file in README.md docs/cli.md .agents/skills/agent-msg-relay/SKILL.md; do
  perl -i -pe "s/System version \\d+\\.\\d+\\.\\d+\\./$marker./" "$file"
done

cargo update -p agent-msg-relay -p agent-msg-relay-protocol -p agent-msg-relay-module

rustup target add aarch64-apple-darwin x86_64-apple-darwin \
  x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-pc-windows-gnu >/dev/null 2>&1 || true

cfg='profile.release.strip="symbols"'
cargo build --release --locked -p agent-msg-relay --target aarch64-apple-darwin --config "$cfg"
cargo build --release --locked -p agent-msg-relay --target x86_64-apple-darwin --config "$cfg"
cargo zigbuild --release --locked -p agent-msg-relay --target x86_64-unknown-linux-musl --config "$cfg"
cargo zigbuild --release --locked -p agent-msg-relay --target aarch64-unknown-linux-musl --config "$cfg"
cargo zigbuild --release --locked -p agent-msg-relay --target x86_64-pc-windows-gnu --config "$cfg"

cp target/aarch64-apple-darwin/release/agent-msg-relay bin/aarch64-apple-darwin/
cp target/x86_64-apple-darwin/release/agent-msg-relay bin/x86_64-apple-darwin/
cp target/x86_64-unknown-linux-musl/release/agent-msg-relay bin/x86_64-unknown-linux-musl/
cp target/aarch64-unknown-linux-musl/release/agent-msg-relay bin/aarch64-unknown-linux-musl/
cp target/x86_64-pc-windows-gnu/release/agent-msg-relay.exe bin/x86_64-pc-windows-gnu/

(
  cd bin
  shasum -a 256 \
    aarch64-apple-darwin/agent-msg-relay \
    x86_64-apple-darwin/agent-msg-relay \
    aarch64-unknown-linux-musl/agent-msg-relay \
    x86_64-unknown-linux-musl/agent-msg-relay \
    x86_64-pc-windows-gnu/agent-msg-relay.exe \
    | awk '{print $1 "  " $2}' > SHA256SUMS
)

cargo test --workspace --locked
bin/aarch64-apple-darwin/agent-msg-relay --version

if [[ "$COMMIT" -eq 1 ]]; then
  git add Cargo.toml Cargo.lock README.md docs/cli.md \
    .agents/skills/agent-msg-relay/SKILL.md bin/
  git commit -m "Release ${VERSION} with refreshed committed CLI binaries."
fi

cat <<EOF

Next:
  git push origin main
  git tag v${VERSION}
  git push origin v${VERSION}

GitHub Actions will create the release and attach bin/ assets.
Draft changelog bullets in commit messages before tagging; see docs/release.md.
EOF
