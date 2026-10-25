# Spec: Discord sound catalog

Status: awaiting approval.

## Objective

Create a versioned, provider-independent catalog of Discord's public stock
client sound cues. The catalog is the source of truth for the later asset
vendoring and event-detection modules: it identifies every approved cue,
its public static asset path, checksum, pack and RUNE target filename.

The user-facing result is that RUNE can eventually use Discord's standard
call, voice, message, stream, camera, push-to-talk, reconnect and activity
sounds as its built-in defaults, including selectable built-in seasonal and
message-theme packs. The catalog module does not change playback behavior or
perform network access at runtime.

## Tech Stack

- Rust workspace, using the existing `rune-media`, `rune-ui` and
  `rune-desktop` sound boundaries.
- A checked-in, human-reviewable manifest and provenance record under
  `assets/sounds`, with SHA-256 checksums and the observed public bundle
  identity.
- Existing Rust test infrastructure; no new dependency is required unless
  the implementation proves an existing workspace dependency cannot validate
  the catalog.

## Commands

```sh
nix-shell
cargo fmt --all -- --check
cargo test -p rune-media
cargo test -p rune-ui
cargo test -p rune-desktop
cargo test --workspace
```

## Project Structure

| Path | Responsibility |
| --- | --- |
| `assets/sounds/` | Vendored default audio and the catalog manifest introduced by this work. |
| `assets/licenses/AUDIO-NOTICES.md` | Source and provenance notice for the packaged audio. |
| `apps/desktop/src/sounds.rs` | Later consumer of catalog-backed built-in sound assets. |
| `crates/rune-ui/src/call_sounds.rs` | Existing provider-independent event keys; unchanged by this catalog-only module. |
| `crates/rune-media/src/sounds.rs` | Existing audio validation boundary reused by later modules. |
| `SOUND_CAPABILITY_MAP.md` | Approved capability scope and build order. |

## Catalog Contract

Each catalog entry must contain:

- a stable logical cue name from the public Discord client bundle;
- the exact public static asset path and the observed bundle identity;
- the file format, byte size and SHA-256 checksum;
- one pack: `standard`, a message-theme pack or a seasonal pack;
- the intended RUNE target filename, if it maps to an existing or planned
  provider-independent sound event.

The initial inventory covers the public stock cues for call calling/ringing,
voice join/leave/move, mute/unmute, deafen/undeafen, disconnect/reconnect,
message and mention notifications, stream lifecycle and participant changes,
camera, push-to-talk, activities and stage waiting. It also records every
observed built-in message-theme and seasonal variant without treating those
variants as new domain events.

The manifest is pinned: builds and application startup never fetch a Discord
asset. A deliberate source-refresh command is the only operation permitted to
download public static assets, and it must fail if the manifest identity or a
checksum does not match the reviewed catalog.

## Code Style

- Keep the catalog data deterministic, sorted by logical cue name and easy to
  review in a diff.
- Use explicit domain names such as `catalog_entry`, `asset_path` and
  `expected_checksum`; do not infer event behavior from filename substrings.
- Keep public sound-event abstractions above provider-specific asset details.
- Add any dependency only with the required explanatory comment in its
  `Cargo.toml`.
- Write new interface text in Spanish through `rune_i18n`; this catalog-only
  module should not need interface text.

## Testing Strategy

- Unit-test that every entry has a unique logical cue name and target path.
- Unit-test format, pack membership, checksum syntax and expected path
  validation without network access.
- Test that the checked-in asset bytes match their catalog checksums once the
  asset module lands.
- Test that all 15 existing `CallSound` filenames receive a standard-default
  mapping before the later replacement is enabled.
- Run the focused package tests first, then `cargo test --workspace` after
  affected modules are implemented.

## Boundaries

Always:

- Use only unauthenticated, public Discord static asset URLs recorded in the
  catalog.
- Preserve the observed source identity and checksums with each vendored
  asset.
- Keep catalog and playback behavior provider-independent above the desktop
  asset boundary.

Ask first:

- Changing the approved source class, adding a dependency, or expanding the
  scope beyond public stock client notification cues.
- Adding a new externally visible RUNE sound event rather than mapping an
  existing event.

Never:

- Read Discord profiles, caches, credentials, tokens, session data or data
  from another person's device/account.
- Copy Discord code or package soundboard entries, soundmojis, games, or
  server/user-uploaded media.
- Download audio during application startup or on the UI thread.

## Success Criteria

- A reviewed catalog lists every observed public stock cue and its pack,
  source identity and checksum.
- The catalog has deterministic offline validation tests.
- The current 15 RUNE event filenames have an explicit standard-pack mapping
  ready for the following asset and event modules.
- Provenance is recorded without exposing user or session data.
- No runtime behavior changes until the dependent modules are approved and
  implemented.

## Open Questions

None. The owner delegated the public source and default-packaging decision on
2026-10-21; the implementation pins a reviewed public bundle snapshot rather
than relying on a mutable runtime source.
