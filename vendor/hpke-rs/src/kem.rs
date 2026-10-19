use alloc::{vec, vec::Vec};

use hpke_rs_crypto::{error::Error, types::KemAlgorithm, HpkeCrypto, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{dh_kem, util, Hpke};

/// A KEM private key wrapper.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey(pub(crate) Vec<u8>);
pub(crate) type PublicKey = Vec<u8>;

#[inline(always)]
fn ciphersuite(alg: KemAlgorithm) -> Vec<u8> {
    util::concat(&[b"KEM", &(alg as u16).to_be_bytes()])
}

pub(crate) fn encaps<Crypto: HpkeCrypto>(
    hpke: &mut Hpke<Crypto>,
    pk_r: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let alg = hpke.kem_id;
    match alg {
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => {
            let randomness = hpke
                .random(alg.private_key_len())
                .map_err(|_| Error::InsufficientRandomness)?;
            dh_kem::encaps::<Crypto>(alg, pk_r, &ciphersuite(alg), &randomness)
        }
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06
        | KemAlgorithm::XWingDraft06Obsolete
        | KemAlgorithm::MlKem768
        | KemAlgorithm::MlKem1024 => Crypto::kem_encaps(alg, pk_r, hpke.rng()),
    }
}

pub(crate) fn decaps<Crypto: HpkeCrypto>(
    alg: KemAlgorithm,
    enc: &[u8],
    sk_r: &[u8],
) -> Result<Vec<u8>, Error> {
    match alg {
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => dh_kem::decaps::<Crypto>(alg, enc, sk_r, &ciphersuite(alg)),
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06
        | KemAlgorithm::XWingDraft06Obsolete
        | KemAlgorithm::MlKem768
        | KemAlgorithm::MlKem1024 => Crypto::kem_decaps(alg, enc, sk_r),
    }
}

pub(crate) fn auth_encaps<Crypto: HpkeCrypto>(
    hpke: &mut Hpke<Crypto>,
    pk_r: &[u8],
    sk_s: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let alg = hpke.kem_id;
    match alg {
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => {
            let randomness = hpke
                .random(alg.private_key_len())
                .map_err(|_| Error::InsufficientRandomness)?;
            dh_kem::auth_encaps::<Crypto>(alg, pk_r, sk_s, &ciphersuite(alg), &randomness)
        }
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06
        | KemAlgorithm::XWingDraft06Obsolete
        | KemAlgorithm::MlKem768
        | KemAlgorithm::MlKem1024 => Err(Error::UnsupportedKemOperation),
    }
}

pub(crate) fn auth_decaps<Crypto: HpkeCrypto>(
    alg: KemAlgorithm,
    enc: &[u8],
    sk_r: &[u8],
    pk_s: &[u8],
) -> Result<Vec<u8>, Error> {
    match alg {
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => {
            dh_kem::auth_decaps::<Crypto>(alg, enc, sk_r, pk_s, &ciphersuite(alg))
        }
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06
        | KemAlgorithm::XWingDraft06Obsolete
        | KemAlgorithm::MlKem768
        | KemAlgorithm::MlKem1024 => Err(Error::UnsupportedKemOperation),
    }
}

/// Returns (private, public)
pub(crate) fn key_gen<Crypto: HpkeCrypto>(
    alg: KemAlgorithm,
    prng: &mut Crypto::HpkePrng,
) -> Result<(PrivateKey, Vec<u8>), Error> {
    match alg {
        // For ECDH based keys, we generate a completely fresh key.
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => dh_kem::key_gen::<Crypto>(alg, prng),
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06
        | KemAlgorithm::XWingDraft06Obsolete
        | KemAlgorithm::MlKem768
        | KemAlgorithm::MlKem1024 => {
            let mut seed = vec![0u8; alg.private_key_len()];
            prng.fill_bytes(&mut seed);
            let (pk, sk) = derive_key_pair::<Crypto>(alg, &seed)?;
            Ok((sk, pk))
        }
    }
}

/// Derive key pair from the input key material `ikm`.
///
/// Returns (PublicKey, PrivateKey).
pub(crate) fn derive_key_pair<Crypto: HpkeCrypto>(
    alg: KemAlgorithm,
    ikm: &[u8],
) -> Result<(PublicKey, PrivateKey), Error> {
    match alg {
        KemAlgorithm::DhKemP256
        | KemAlgorithm::DhKemK256
        | KemAlgorithm::DhKemP384
        | KemAlgorithm::DhKemP521
        | KemAlgorithm::DhKem25519
        | KemAlgorithm::DhKem448 => dh_kem::derive_key_pair::<Crypto>(alg, &ciphersuite(alg), ikm),
        #[allow(deprecated)]
        KemAlgorithm::XWingDraft06 | KemAlgorithm::XWingDraft06Obsolete => {
            let seed = shake256::<32>(ikm);
            Crypto::kem_key_gen_derand(alg, &seed).map(|(ek, dk)| (ek, PrivateKey(dk)))
        }
        KemAlgorithm::MlKem768 | KemAlgorithm::MlKem1024 => {
            let seed = shake256::<64>(ikm);
            Crypto::kem_key_gen_derand(alg, &seed).map(|(ek, dk)| (ek, PrivateKey(dk)))
        }
    }
}

/// SHAKE256 with a fixed output length. RUNE patch: RustCrypto `sha3`
/// replaces `libcrux_sha3::shake256` (RUSTSEC-2026-0207, RUSTSEC-2026-0208).
fn shake256<const N: usize>(data: &[u8]) -> [u8; N] {
    use sha3::digest::{ExtendableOutput, Update};

    let mut hasher = sha3::Shake256::default();
    hasher.update(data);
    let mut out = [0u8; N];
    hasher.finalize_xof_into(&mut out);
    out
}

#[cfg(test)]
mod shake256_kat {
    use super::shake256;

    // Tests build with std (`cfg_attr(not(test), no_std)` in lib.rs).
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // Expected values computed independently with Python's hashlib.shake_256.
    #[test]
    fn matches_known_answers() {
        let counting: [u8; 32] = core::array::from_fn(|i| i as u8);
        assert_eq!(
            hex(&shake256::<32>(b"")),
            "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f"
        );
        assert_eq!(
            hex(&shake256::<64>(b"")),
            "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f\
             d75dc4ddd8c0f200cb05019d67b592f6fc821c49479ab48640292eacb3b7c4be"
        );
        assert_eq!(
            hex(&shake256::<32>(&counting)),
            "69f07c8840ce80024db30939882c3d5bbc9c98b3e31e4513ebd2ca9b4503cdd3"
        );
        assert_eq!(
            hex(&shake256::<64>(&counting)),
            "69f07c8840ce80024db30939882c3d5bbc9c98b3e31e4513ebd2ca9b4503cdd3\
             c9c90742452c7173d4a75ac49163e14ee0cc24ef7035b272d19a7af1099b333f"
        );
    }
}
