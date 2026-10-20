#!/usr/bin/env bash
# RUNE's release key (docs/UPDATES.md#la-clave): run once, on the owner's
# machine.
#
#   scripts/update-key.sh [private-key-file]
#
# The private half goes to a file outside the repository (by default
# ~/.config/rune-release/update-signing.key, 0600) and, through `gh secret
# set`, to the GitHub Actions secret RUNE_UPDATE_SIGNING_KEY of
# Niiihuel/rune; it is never printed. The public half goes to
# crates/rune-update/release-key.pub, to commit: every RUNE built from then
# on trusts only releases signed with this key.
#
# Keep a copy of the private file somewhere safe. Installed copies cannot
# be told of another key: losing it means reinstalling by hand.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
public="$repo/crates/rune-update/release-key.pub"
key="${1:-$HOME/.config/rune-release/update-signing.key}"
key="$(realpath -m "$key")"

case "$key" in
  "$repo"/*)
    echo "update-key: the private key must live outside the repository" >&2
    exit 1
    ;;
esac
if [ -s "$public" ]; then
  echo "update-key: $public already holds a key; installed copies trust only that one" >&2
  echo "update-key: to replace it on purpose, empty that file first (docs/UPDATES.md#la-clave)" >&2
  exit 1
fi

mkdir -p "$(dirname "$key")"
chmod 700 "$(dirname "$key")"
# The private half is written by the tool itself (0600, never over a file
# that exists); only the public half reaches stdout.
(cd "$repo" && cargo run --quiet --locked -p rune-release -- keygen "$key") > "$public"
echo "update-key: public key written to crates/rune-update/release-key.pub: $(cat "$public")"
echo "update-key: private key in $key"

if command -v gh > /dev/null; then
  gh secret set RUNE_UPDATE_SIGNING_KEY --repo Niiihuel/rune < "$key"
  echo "update-key: secret RUNE_UPDATE_SIGNING_KEY set in Niiihuel/rune"
else
  echo "update-key: gh is not installed; set the secret with:" >&2
  echo "  gh secret set RUNE_UPDATE_SIGNING_KEY --repo Niiihuel/rune < \"$key\"" >&2
fi
echo "update-key: now commit crates/rune-update/release-key.pub"
