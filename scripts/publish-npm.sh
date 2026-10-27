#!/usr/bin/env bash
# Stage the committed bin/ binaries into npm packages and publish them.
# The package version is the workspace version in Cargo.toml. Templates stay at 0.0.0.
# Publish uses GitHub Actions trusted publishing (OIDC). It does not read an npm token.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DRY=0
LOCAL=0
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY=1
elif [[ "${1:-}" == "--local" ]]; then
  LOCAL=1
elif [[ -n "${1:-}" ]]; then
  echo "Usage: scripts/publish-npm.sh [--dry-run | --local]" >&2
  exit 2
fi

current_version() {
  awk '/^\[workspace.package\]/{found=1} found && /^version = /{gsub(/version = "|"/,""); print; exit}' Cargo.toml
}

VERSION="$(current_version)"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid workspace version: ${VERSION:-<empty>}" >&2
  exit 2
fi

STAGE="$ROOT/target/npm"
rm -rf "$STAGE"
mkdir -p "$STAGE"

# name|os is unused here|cpu unused|source relative to repo|filename inside the package
PLATFORMS=(
  "agent-msg-relay-darwin-arm64|bin/aarch64-apple-darwin/agent-msg-relay|agent-msg-relay"
  "agent-msg-relay-darwin-x64|bin/x86_64-apple-darwin/agent-msg-relay|agent-msg-relay"
  "agent-msg-relay-linux-arm64|bin/aarch64-unknown-linux-musl/agent-msg-relay|agent-msg-relay"
  "agent-msg-relay-linux-x64|bin/x86_64-unknown-linux-musl/agent-msg-relay|agent-msg-relay"
  "agent-msg-relay-win32-x64|bin/x86_64-pc-windows-gnu/agent-msg-relay.exe|agent-msg-relay.exe"
)

stamp_version() {
  perl -i -pe "s/0\\.0\\.0/${VERSION}/g" "$1"
}

stage_platform() {
  local name="$1" src="$2" dest="$3"
  local dir="$STAGE/$name"
  mkdir -p "$dir"
  cp "npm/$name/package.json" "$dir/package.json"
  stamp_version "$dir/package.json"
  cp "$src" "$dir/$dest"
  if [[ "$dest" != *.exe ]]; then
    chmod +x "$dir/$dest"
  fi
}

for spec in "${PLATFORMS[@]}"; do
  IFS='|' read -r name src dest <<<"$spec"
  if [[ ! -f "$src" ]]; then
    echo "Missing binary: $src" >&2
    exit 1
  fi
  stage_platform "$name" "$src" "$dest"
done

mkdir -p "$STAGE/agent-msg-relay/bin"
cp "npm/agent-msg-relay/package.json" "$STAGE/agent-msg-relay/package.json"
cp "npm/agent-msg-relay/bin/run.js" "$STAGE/agent-msg-relay/bin/run.js"
chmod +x "$STAGE/agent-msg-relay/bin/run.js"
stamp_version "$STAGE/agent-msg-relay/package.json"

node --check "$STAGE/agent-msg-relay/bin/run.js"
node -e '
  const fs = require("fs");
  const version = process.argv[1];
  const stage = process.argv[2];
  const meta = JSON.parse(fs.readFileSync(stage + "/agent-msg-relay/package.json", "utf8"));
  if (meta.version !== version) process.exit(1);
  if (meta.scripts) process.exit(1);
  const deps = meta.optionalDependencies || {};
  const names = Object.keys(deps);
  if (names.length !== 5) process.exit(1);
  for (const name of names) {
    if (deps[name] !== version) process.exit(1);
    const platform = JSON.parse(fs.readFileSync(stage + "/" + name + "/package.json", "utf8"));
    if (platform.version !== version) process.exit(1);
    if (platform.scripts) process.exit(1);
    if (platform.libc) process.exit(1);
  }
' "$VERSION" "$STAGE"

pack_one() {
  local dir="$1" needle="$2"
  local json
  json="$(cd "$dir" && npm pack --dry-run --json)"
  node -e '
    const listing = JSON.parse(process.argv[1]);
    const needle = process.argv[2];
    const files = (listing[0] && listing[0].files ? listing[0].files : []).map((file) => file.path);
    if (!files.includes(needle)) {
      console.error("Packed tarball is missing " + needle);
      console.error(files.join("\n"));
      process.exit(1);
    }
  ' "$json" "$needle"
}

for spec in "${PLATFORMS[@]}"; do
  IFS='|' read -r name _src dest <<<"$spec"
  pack_one "$STAGE/$name" "$dest"
done
pack_one "$STAGE/agent-msg-relay" "bin/run.js"

smoke_platform() {
  local uname_s uname_m pkg
  uname_s="$(uname -s)"
  uname_m="$(uname -m)"
  case "${uname_s}-${uname_m}" in
    Darwin-arm64) pkg="agent-msg-relay-darwin-arm64" ;;
    Darwin-x86_64) pkg="agent-msg-relay-darwin-x64" ;;
    Linux-x86_64) pkg="agent-msg-relay-linux-x64" ;;
    Linux-aarch64|Linux-arm64) pkg="agent-msg-relay-linux-arm64" ;;
    *)
      echo "Skipping binary smoke test on ${uname_s} ${uname_m}"
      return
      ;;
  esac
  local smoke="$STAGE/smoke/node_modules"
  mkdir -p "$smoke"
  cp -R "$STAGE/agent-msg-relay" "$smoke/agent-msg-relay"
  cp -R "$STAGE/$pkg" "$smoke/$pkg"
  local out
  out="$(node "$smoke/agent-msg-relay/bin/run.js" --version)"
  if ! grep -F "$VERSION" <<<"$out" >/dev/null; then
    echo "Staged binary did not print ${VERSION}: ${out}" >&2
    exit 1
  fi
  echo "$out"
}

smoke_platform

if [[ "$DRY" -eq 1 ]]; then
  echo "Dry run packed agent-msg-relay@${VERSION} under target/npm. Nothing was published."
  exit 0
fi

if [[ "$LOCAL" -eq 1 ]]; then
  if ! npm whoami >/dev/null 2>&1; then
    echo "Run npm login in this terminal first. The first publish is your npm account, not GitHub Actions." >&2
    exit 1
  fi
else
  if [[ -z "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]]; then
    echo "These packages are not on npm yet, so GitHub Actions cannot create them." >&2
    echo "In your terminal: npm login, then ./scripts/publish-npm.sh --local" >&2
    echo "See docs/release.md." >&2
    exit 1
  fi
  unset NPM_TOKEN NODE_AUTH_TOKEN || true
fi

if ! node -e 'const [a,b,c]=process.argv[1].split(".").map(Number); if(a<11||(a===11&&(b<5||(b===5&&c<1)))) process.exit(1)' "$(npm --version)"; then
  echo "npm $(npm --version) is older than 11.5.1; installing npm@11.5.1" >&2
  npm install -g npm@11.5.1
fi

publish_one() {
  local dir="$1" name="$2"
  local published
  published="$(npm view "${name}@${VERSION}" version 2>/dev/null || true)"
  if [[ "$published" == "$VERSION" ]]; then
    echo "${name}@${VERSION} is already published"
    return
  fi
  echo "Publishing ${name}@${VERSION}"
  if [[ "$LOCAL" -eq 1 ]]; then
    (cd "$dir" && npm publish --access public)
  else
    (cd "$dir" && npm publish --access public --provenance)
  fi
}

for spec in "${PLATFORMS[@]}"; do
  IFS='|' read -r name _src _dest <<<"$spec"
  publish_one "$STAGE/$name" "$name"
done
publish_one "$STAGE/agent-msg-relay" "agent-msg-relay"

echo "Published agent-msg-relay@${VERSION}"
