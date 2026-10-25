# RUNE patch of openmls_basic_credential 0.5.0

RUNE uses this copy through `[patch.crates-io]` in the workspace `Cargo.toml`.
It reaches the build as `davey 0.1.5` -> `openmls_basic_credential ^0.5.0`
(the key pair DAVE signs MLS messages with), and as a dev-dependency of
`rune-discord-session` (the synthetic voice server's fixtures). See
`vendor/davey/RUNE-PATCH.md` for why: the 0.5.0 release needs `p256 0.13`,
on `elliptic-curve 0.13` -> `crypto-bigint 0.5.5` (RUSTSEC-2026-0387).

## Source

- Archive: `https://static.crates.io/crates/openmls_basic_credential/openmls_basic_credential-0.5.0.crate`
- SHA-256: `983e8be1457dd6f316f409292cec334af3b57b49a19deadc925c83c3c35e15b6`,
  identical to the crates.io checksum recorded in RUNE's `Cargo.lock` before
  the patch.
- Upstream commit (`.cargo_vcs_info.json`):
  `6b85f0edc560b4fe0f5b9266092947a774614f3f` of `openmls/openmls`, path
  `basic_credential`.
- License: MIT (declared in `Cargo.toml`). The archive ships no license text,
  so `LICENSE` is the repository's own file at that commit
  (SHA-256 `43e5e3c4b5cca67f9ea912f7e1929702a848aa765d3b3e14f25d3838a5a5565d`).

Every other file is the archive content unchanged, except the archive's own
`Cargo.lock`, removed. `Cargo.toml.orig` and `.cargo_vcs_info.json` stay as
provenance.

## Changes

The same calls upstream openmls `main` uses in this file after its move to
`p256 0.14` (`be248d4b1306d87a641ce24ed0ffcab2b27967f6`); nothing else of
upstream's later changes is taken.

1. `Cargo.toml`: `p256 "0.13"` -> `"0.14"`; `rand "0.8"` replaced by
   `rand_core "0.10"` and `getrandom "0.4"` (feature `sys_rng`);
   `ed25519-dalek "2.0"` -> `"3.0"`.
2. `src/lib.rs`:
   - key generation: `SigningKey::random(&mut OsRng)` ->
     `generate_from_rng(&mut UnwrapErr(SysRng))` (the operating system's RNG,
     as before), `to_encoded_point(false)` -> `to_sec1_point(false)`; Ed25519
     generation takes the same RNG;
   - P-256 signing: the private key slice is converted with `try_into()`, so
     a key of the wrong length returns `SignerError::SigningError` instead of
     panicking.

The serialized key pair (TLS and serde) is unchanged: same fields, a 32-byte
scalar and a 65-byte uncompressed SEC1 public key. Signatures stay DER.

## Validation

See `vendor/openmls_rust_crypto/RUNE-PATCH.md`: RUNE's DAVE tests, and key
pairs from this copy and from the crates.io release signing for each other's
verifiers in a scratch interop crate.

## When to remove

Drop this directory and its `[patch.crates-io]` entry together with
`vendor/davey` (see there).
