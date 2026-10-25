# Capability Map: Discord sound assets

Status: approved by the owner on 2026-10-21.

## Observed baseline

- RUNE already has a 15-event `CallSound` model, a per-event switch and a
  user-selected replacement folder. Its built-in assets will be replaced by
  the approved Discord stock cues.
- The locally installed Discord 1.0.155 `app.asar` has no audio assets
  (REA evidence `ev_deb5e674298c3e1a2fbb99e1146b541014cb4defac50cbb09da8b3b4bfd37c05`).
- The public Discord web bundle observed on 2026-10-21 contains the stock
  sound manifest, including call, voice, message, stream, camera, activity,
  PTT and reconnect cues. Its static analysis exhausted the bounded worker
  heap, so the catalog is based only on its directly observed literal asset
  entries, not inferred runtime behavior (REA evidence
  `ev_f69aa1c6169e90750f2db16f9c0a16732f0e3f64e46752428309d077dc4c68a0`).

## Scope assumption

“All sounds” means Discord's public stock client cues for voice, calls,
streams, notifications, camera, PTT, reconnect and voice activities. It
includes built-in message-theme and seasonal variants as selectable packs. It
excludes soundboard items, soundmojis, games and other server/user-uploaded
media, which are not client stock notification cues.

| Module id | Responsibility | Depends on |
| --- | --- | --- |
| `discord-sound-catalog` | Record every observed stock cue name, public asset identity, checksum and pack membership; reject unknown or mismatched files. | — |
| `discord-sound-assets` | Fetch the approved unauthenticated static assets during a reproducible source refresh, validate them and vendor them as RUNE defaults. | `discord-sound-catalog` |
| `sound-events` | Extend RUNE's provider-independent sound events and detect each one from existing `Store`, `Ui` and media state. | — |
| `sound-settings` | Let the person preview, select a pack and manage every event without blocking the UI. | `sound-events`, `discord-sound-assets` |
| `sound-verification` | Add deterministic unit/integration coverage and retain a concise REA finding ledger. | `discord-sound-catalog`, `discord-sound-assets`, `sound-events`, `sound-settings` |

Build order: `discord-sound-catalog` → `discord-sound-assets` →
`sound-events` → `sound-settings` → `sound-verification`.

## Approved source and distribution

The installed desktop package does not ship the audio files. The owner
authorizes an unauthenticated download from Discord's public static asset
bundle and packaging the validated result in `assets/sounds` and release
artifacts. The source refresh records the observed bundle identity, asset
paths and content checksums in version-controlled provenance data; no
runtime download is necessary.

The process may not read Discord profiles, caches, credentials or session
data, and it may not package code, soundboard entries, soundmojis, games or
media uploaded by servers or users.
