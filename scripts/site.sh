#!/usr/bin/env bash
# Recreate or update the GitHub Pages site.
#
#   ./scripts/site.sh sync     Copy docs/ into the Starlight collection.
#   ./scripts/site.sh dev      Sync, install, and start the local site.
#   ./scripts/site.sh build    Sync, install, and write site/dist.
#   ./scripts/site.sh preview  Serve the last build.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

cmd="${1:-build}"
case "$cmd" in
sync)
	node scripts/sync-site-docs.mjs
	;;
dev)
	node scripts/sync-site-docs.mjs
	(cd site && npm install && npm run dev)
	;;
build)
	node scripts/sync-site-docs.mjs
	(cd site && npm install && npm run build)
	;;
preview)
	(cd site && npm run preview)
	;;
*)
	echo "usage: ./scripts/site.sh sync|dev|build|preview" >&2
	exit 2
	;;
esac
