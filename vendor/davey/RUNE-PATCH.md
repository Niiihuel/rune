# RUNE patch of davey 0.1.5

RUNE uses this copy through `[patch.crates-io]` in the workspace `Cargo.toml`.
`rune-discord-session` depends on `davey =0.1.5` for DAVE, the end-to-end
encryption Discord requires for calls. It moves together with
`vendor/openmls_rust_crypto`, `vendor/openmls_basic_credential` and
`vendor/hpke-rs`: all four exist to take `elliptic-curve 0.13` and with it
`crypto-bigint 0.5.5` (RUSTSEC-2026-0387, non-constant-time operations,
patched only in `>= 0.7.6`) out of RUNE's `Cargo.lock`.

## Source

- Archive: `https://static.crates.io/crates/davey/davey-0.1.5.crate`
- SHA-256: `27466c95d1885cc506b29cec686a91ba7c472376cca09850d026cceb9c29a888`,
  identical to the crates.io checksum recorded in RUNE's `Cargo.lock` before
  the patch.
- Upstream commit (`.cargo_vcs_info.json`):
  `73bcec27b7190ce47088c831e0aa72600cc657e7` of `Snazzah/davey`, path `davey`.
- License: MIT (declared in `Cargo.toml`). The archive ships no license text,
  so `LICENSE` is the repository's own file at that commit
  (SHA-256 `90760006b6e6c76a67476de39bee25e42e584679eac5d608765355d5d54e8afa`).

Every other file is the archive content unchanged, except the archive's own
`Cargo.lock`, removed (Cargo ignores it for a path dependency).
`Cargo.toml.orig` and `.cargo_vcs_info.json` stay as provenance;
`Cargo.toml.orig` is the pristine upstream manifest, not the one Cargo reads.

## Changes

1. `Cargo.toml`: `p256 "0.13"` -> `"0.14"`; `rand "0.8"` replaced by
   `rand_core "0.10"` and `getrandom "0.4"` (feature `sys_rng`). `openmls`
   stays `^0.8.1` from crates.io; `openmls_rust_crypto` and
   `openmls_basic_credential` keep their version requirements and resolve to
   the patched copies next to this one.
2. `src/signing_key_pair.rs`, the only file that touched `p256` or `rand`:
   `SigningKey::random(&mut OsRng)` -> `SigningKey::generate_from_rng(&mut
   UnwrapErr(SysRng))` (the operating system's RNG, as before) and
   `to_encoded_point(false)` -> `to_sec1_point(false)`, the 0.14 names of the
   same calls. The key pair is the same 32-byte scalar and 65-byte
   uncompressed SEC1 point.

No MLS, DAVE, frame or key-ratchet code changed. Newer commits on davey's
`master` (libdave parity guards) are not taken: this is the 0.1.5 release
RUNE already shipped, only moved to the new curve crates.

## Validation

- RUNE: `cargo test --workspace`, including the DAVE tests of
  `rune-discord-session` against the synthetic voice server (two-party
  encryption, forgery and downgrade rejection, video and screen frames).
- Interop, in a scratch crate outside the workspace: keys from this copy's
  `SigningKeyPair::generate` decoded with `p256 0.13`, and the checks in
  `vendor/openmls_rust_crypto/RUNE-PATCH.md`.

## When to remove

Drop this directory and its `[patch.crates-io]` entry when a davey release
depends on `p256 >= 0.14` (or no longer on `elliptic-curve 0.13`) together
with an openmls RustCrypto backend that does too. Check with
`cargo tree -i crypto-bigint@0.5.5 --all-features` and
`cargo deny --all-features check advisories` after removing it.
