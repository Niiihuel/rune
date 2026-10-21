#!/usr/bin/env bash
# Verify the public Discord sound assets packaged with RUNE. This never
# downloads files; their source bundle and public paths live in
# assets/sounds/discord/SOURCES.md.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/assets/sounds/discord"
sha256sum --check SHA256SUMS
