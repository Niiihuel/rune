//! A local feed of a made-up release, signed with a throwaway key, to try
//! RUNE's updates before there is a real one (`docs/UPDATES.md`):
//!
//! ```text
//! cargo run -p rune-release --bin rune-test-feed -- \
//!     [--version 9.9.9] [--mandatory] [--slow] [--corrupt] [--port 8765] [--program <file>]
//!     [--platform windows-x86_64]
//! ```
//!
//! It prints the `RUNE_UPDATE_FEED` and `RUNE_UPDATE_KEY` a debug build of
//! RUNE needs to use it, and serves until it is stopped. `--program` is
//! the program the release installs (a build of RUNE with that version);
//! without it the artifact is a stand-in that is not a program, which
//! installing refuses. `--slow` paces the download, `--corrupt` signs a
//! digest the artifact does not have. `--platform` serves another system
//! than this one: for Windows the program is the installer, as it is
//! (a RUNE under Wine updating from a feed on Linux).

use std::io::Write as _;
use std::time::Duration;

use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use rune_update::{Artifact, Manifest, Version};
use sha2::{Digest, Sha256};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (mut version, mut mandatory, mut slow, mut corrupt) =
        ("9.9.9".to_owned(), false, false, false);
    let (mut port, mut program, mut platform) = (8765, None, rune_update::platform());
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" => version = arguments.next().expect("--version <x.y.z>"),
            "--mandatory" => mandatory = true,
            "--slow" => slow = true,
            "--corrupt" => corrupt = true,
            "--port" => {
                port = arguments
                    .next()
                    .and_then(|port| port.parse().ok())
                    .expect("--port <n>")
            }
            "--program" => program = Some(arguments.next().expect("--program <file>")),
            "--platform" => platform = arguments.next().expect("--platform <os-arch>"),
            other => panic!("unknown argument {other}"),
        }
    }
    let version = Version::parse(&version).expect("a version");
    let program = match program {
        Some(file) => std::fs::read(file).expect("the program"),
        // A stand-in that does not compress, 8 MiB, so its progress shows.
        None => {
            let mut state = 0x9e37_79b9_7f4a_7c15_u64;
            (0..8 << 20)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect()
        }
    };
    // As the release workflow publishes them: Linux's program gzip-compressed,
    // Windows' installer as it is.
    let (name, artifact) = if platform.starts_with("windows") {
        ("rune-x86_64-windows-setup.exe", program)
    } else {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&program).unwrap();
        ("rune-x86_64-linux.gz", encoder.finish().unwrap())
    };

    let mut seed = [0; 32];
    getrandom::fill(&mut seed).unwrap();
    let key = SigningKey::from_bytes(&seed);
    rune_update::block_on(async move {
        let server = rune_update::serve::serve(port)
            .await
            .expect("the port is free");
        let mut sha256 = rune_update::hex(&Sha256::digest(&artifact));
        if corrupt {
            sha256 = rune_update::hex(&Sha256::digest(b"something else"));
        }
        let manifest = Manifest {
            minimum_version: mandatory.then(|| version.clone()),
            version,
            date: "2026-10-19T12:00:00-03:00".into(),
            notes: "Versión de prueba del feed local.\n\n- Arregla el inicio desde la caché.\n- La lista de miembros se abre más rápido.".into(),
            platforms: [(
                platform,
                Artifact {
                    url: format!("{}/{name}", server.base()),
                    size: artifact.len() as u64,
                    sha256,
                },
            )]
            .into(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let signature =
            base64::engine::general_purpose::STANDARD.encode(key.sign(&bytes).to_bytes());
        server.put(
            &format!("/{}", rune_update::SIGNATURE),
            signature.into_bytes(),
        );
        server.put(&format!("/{}", rune_update::MANIFEST), bytes);
        server.put(&format!("/{name}"), artifact);
        if slow {
            server.configure(|files| files.pace = Some(Duration::from_millis(150)));
        }
        println!("RUNE_UPDATE_FEED={}", server.base());
        println!(
            "RUNE_UPDATE_KEY={}",
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes())
        );
        std::future::pending::<()>().await;
    });
}
