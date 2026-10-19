# RUNE patch of hpke-rs 0.6.1

RUNE uses this copy through `[patch.crates-io]` in the workspace `Cargo.toml`.
It reaches the build as `davey 0.1.5` -> `openmls_rust_crypto 0.5.1` ->
`hpke-rs ^0.6` (DAVE end-to-end encryption for voice).

## Source

- Archive: `https://static.crates.io/crates/hpke-rs/hpke-rs-0.6.1.crate`
- SHA-256: `b6ad6a58eb3e0ee30be8bfc7a9770ae98adcfa1d9bc820a5847732ce84f70837`,
  identical to the crates.io checksum recorded in RUNE's `Cargo.lock` before
  the patch.
- Upstream commit (`.cargo_vcs_info.json`):
  `f3463e7530771d7f7116635335c25e7d2d11e861` of `cryspen/hpke-rs` (now
  `celabshq/hpke-rs`).
- License: MPL-2.0 (declared in `Cargo.toml`). Neither the archive nor the
  upstream repository at that commit ships the license text, so `LICENSE` is
  the canonical MPL-2.0 text from <https://www.mozilla.org/media/MPL/2.0/index.txt>
  (SHA-256 `3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`).
  The modified files remain under MPL-2.0.

Every other file is the archive content unchanged, except the archive's own
`Cargo.lock`, removed: Cargo ignores it for a path dependency, and it still
pinned the affected `libcrux-*` versions, which would mislead lockfile
scanners. `Cargo.toml.orig` and `.cargo_vcs_info.json` stay as provenance;
`Cargo.toml.orig` is the pristine upstream manifest, not the one Cargo reads.

## Changes

1. `Cargo.toml`: the unconditional `libcrux-sha3 = "0.0.8"` dependency is
   replaced by `sha3 = { version = "0.10.8", default-features = false }`
   (RustCrypto, on the `digest 0.10` already in RUNE's graph). The optional
   `hpke-rs-libcrux` dependency, the `libcrux` feature and its
   `hpke-rs-libcrux?/std` and `hpke-rs-libcrux?/deterministic-prng` feature
   entries are removed. Dev-dependencies are untouched (Cargo does not
   resolve them for a path dependency).
2. `src/lib.rs`: removed the `#[cfg(feature = "libcrux")]` re-export
   `pub use hpke_rs_libcrux as libcrux;`.
3. `src/kem.rs`: `derive_key_pair` calls a private
   `shake256::<N>(ikm) -> [u8; N]` helper built on `sha3::Shake256`
   (`finalize_xof_into`) instead of `libcrux_sha3::shake256::<N>`; N is 32
   for X-Wing and 64 for ML-KEM, as before. Added the `shake256_kat` unit
   test: SHAKE256 of the empty string and of bytes `0..32`, at 32 and 64
   bytes, against values computed independently with Python
   `hashlib.shake_256`.

No algorithm, key schedule or wire-format change. The DHKEM P-256 suite DAVE
uses never reached SHAKE256; only X-Wing and ML-KEM key derivation did.

## Advisories removed from RUNE's Cargo.lock

`cargo audit` (cargo-audit 0.22.1, RustSec DB of 1 329 advisories) on the
lock before the patch: 8 vulnerabilities, all in crates this patch removes.

- `libcrux-sha3 0.0.8`: RUSTSEC-2026-0207, RUSTSEC-2026-0208
- `libcrux-secrets 0.0.5`: RUSTSEC-2026-0212
- `libcrux-aesgcm 0.0.7`: RUSTSEC-2026-0209, RUSTSEC-2026-0211, and the
  RUSTSEC-2026-0210 unmaintained warning
- `libcrux-chacha20poly1305 0.0.7`: RUSTSEC-2026-0124
- `libcrux-kem 0.0.7`: RUSTSEC-2026-0330, RUSTSEC-2026-0331
- `proc-macro-error2 2.0.1` (unmaintained, RUSTSEC-2026-0173)

Packages that left the lock (729 -> 703 entries): `hpke-rs` (registry),
`hpke-rs-libcrux`, nineteen `libcrux-*` crates, `hax-lib`, `hax-lib-macros`,
`hax-lib-macros-types`, `core-models`, `pastey`, `proc-macro-error2`,
`proc-macro-error-attr2`, `rand_chacha 0.10.0`. Added: `sha3 0.10.9`,
`keccak 0.1.6`. After the patch `cargo audit` reports 0 vulnerabilities and
one warning, `instant 0.1.13` (unmaintained, RUSTSEC-2024-0384), which comes
from `davey` enabling openmls' `js` feature on every target
(`fluvio-wasm-timer` -> `parking_lot 0.11` -> `instant`), not from hpke-rs.

## Validation

- `cargo tree --target all -i libcrux-sha3` (and `libcrux-secrets`,
  `hpke-rs-libcrux`) matches no package in the RUNE workspace.
- RUNE: `cargo test --workspace --all-features`, including the DAVE
  two-party encryption, forgery/downgrade rejection and voice tests.
- This crate on its own, in a scratch copy outside the workspace, with the
  upstream `tests/` from the same commit (excluded from the archive):
  `cargo test --release --features rustcrypto,hpke-test,serialization,hazmat`
  passes the unit tests (with `shake256_kat`), `misc`, `test_hpke` (X-Wing
  round trips with the libcrux provider go through the patched
  `derive_key_pair`) and the RFC 9180 known-answer tests in
  `test_hpke_kat`.

## When to remove

Drop this directory and the `[patch.crates-io]` entry as soon as the
`openmls`/`openmls_rust_crypto` that `davey` resolves to accepts
`hpke-rs >= 0.7` (0.7.0 fixes the advisories upstream), or as soon as a
0.6.x release of hpke-rs no longer depends on an affected `libcrux-sha3`.
Check with `cargo tree -i hpke-rs` and `cargo audit` after removing it.
