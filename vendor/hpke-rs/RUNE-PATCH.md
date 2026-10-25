# RUNE patch of hpke-rs 0.8.0

RUNE uses this copy through `[patch.crates-io]` in the workspace `Cargo.toml`.
It reaches the build as `davey 0.1.5` -> `openmls_rust_crypto 0.5.1` (patched
in `vendor/openmls_rust_crypto` onto `hpke-rs 0.8`) -> `hpke-rs` (DAVE
end-to-end encryption for voice).

This replaces RUNE's earlier patch of hpke-rs 0.6.1, which existed because
0.6.1 needed `libcrux-sha3 0.0.8` (RUSTSEC-2026-0207/0208). Moving to 0.8.0
fixes those upstream (`libcrux-sha3 0.0.11`), but 0.8.0 still builds
`libcrux-sha3` unconditionally, and its `hax-lib` brings the unmaintained
`proc-macro-error2` (RUSTSEC-2026-0173) back into the lock. The same change
as before, RustCrypto `sha3` for the fixed-length SHAKE256, keeps libcrux out.

## Source

- Archive: `https://static.crates.io/crates/hpke-rs/hpke-rs-0.8.0.crate`
- SHA-256: `deb74477a6c7f6e8e9c68de06fe928db79b3b8b9e04c900bfb729bf096df7d22`
  (the crates.io checksum `cargo` records for `hpke-rs 0.8.0`).
- Upstream commit (`.cargo_vcs_info.json`):
  `42d68bd49b244b09bd02626c6ffa95e54dc64f91` of `celabshq/libcrux`, path
  `crates/protocols/hpke` (hpke-rs moved there from `cryspen/hpke-rs`).
- License: MPL-2.0 (declared in `Cargo.toml`). Neither the archive nor the
  upstream repository at that commit ships the license text, so `LICENSE` is
  the canonical MPL-2.0 text from <https://www.mozilla.org/media/MPL/2.0/index.txt>
  (SHA-256 `3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`).
  The modified files remain under MPL-2.0.

Every other file is the archive content unchanged, except the archive's own
`Cargo.lock`, removed: Cargo ignores it for a path dependency, and it pins
`libcrux-*` versions, which would mislead lockfile scanners.
`Cargo.toml.orig` and `.cargo_vcs_info.json` stay as provenance;
`Cargo.toml.orig` is the pristine upstream manifest, not the one Cargo reads.

## Changes

1. `Cargo.toml`: the unconditional `libcrux-sha3 = "=0.0.11"` dependency is
   replaced by `sha3 = { version = "0.10.8", default-features = false }`
   (RustCrypto). The optional `hpke-rs-libcrux` dependency and the features
   that only enable it (`libcrux`, `libcrux-no-sys-rng`,
   `libcrux-rustcrypto-p-curves`, `draft-ietf-hpke-pq`) are removed, with
   `hpke-rs-libcrux?/std` from `std`, `libcrux` and `draft-ietf-hpke-pq`
   from the docs.rs feature list and the two benches that require `libcrux`. `draft-connolly-cfrg-hpke-mlkem`
   stays as an empty feature (it only selects the unlabeled seed derivation
   in `kem.rs`). Dev-dependencies are untouched (Cargo does not resolve them
   for a path dependency).
2. `src/lib.rs`: removed the `#[cfg(feature = "libcrux-no-sys-rng")]`
   re-export `pub use hpke_rs_libcrux as libcrux;`.
3. `src/kem.rs`: `pq_derive_keypair_seed` calls a private
   `shake256::<N>(ikm) -> [u8; N]` helper built on `sha3::Shake256`
   (`finalize_xof_into`) instead of `libcrux_sha3::shake256::<N>`; N is 32
   or 64, as before. Added the `shake256_kat` unit test: SHAKE256 of the
   empty string and of bytes `0..32`, at 32 and 64 bytes, against values
   computed independently with Python `hashlib.shake_256`. Both the helper
   and the test are the ones the 0.6.1 patch had.

No algorithm, key schedule or wire-format change. The DHKEM P-256 suite DAVE
uses never reaches SHAKE256; only X-Wing and ML-KEM key derivation do.

## Validation

- `cargo tree --target all -i libcrux-sha3` (and `hax-lib`,
  `proc-macro-error2`, `hpke-rs-libcrux`) matches no package in the RUNE
  workspace; `cargo deny --all-features check` passes.
- RUNE: `cargo test --workspace`, including the DAVE tests.
- This crate on its own, in a scratch copy outside the workspace with the
  upstream `tests/` from the same commit (excluded from the archive) and the
  pristine manifest except for the `sha3` swap:
  `cargo test --release --features rustcrypto,hpke-test,serialization,hazmat`
  passes the unit tests (with `shake256_kat`), `misc`, `test_custom_rng`,
  `test_hpke` and the RFC 9180 known-answer tests in `test_hpke_kat`; adding
  `draft-ietf-hpke-pq` also passes `test_hpke_pq`, whose X-Wing and ML-KEM
  known answers go through the patched `pq_derive_keypair_seed`.

## When to remove

Drop this directory and its `[patch.crates-io]` entry when an hpke-rs
release no longer builds `libcrux-sha3` (or `hax-lib` no longer pulls
`proc-macro-error2`), or when `RUSTSEC-2026-0173` is withdrawn. Check with
`cargo tree --target all -i proc-macro-error2` and
`cargo deny --all-features check advisories` after removing it.
