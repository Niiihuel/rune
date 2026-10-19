#[cfg(unix)]
use std::io::Write as _;

use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};

use super::*;
#[cfg(unix)]
use crate::install::Install;
use crate::serve::{serve, Server};

fn key() -> SigningKey {
    let mut seed = [0; 32];
    getrandom::fill(&mut seed).unwrap();
    SigningKey::from_bytes(&seed)
}

fn sign(key: &SigningKey, bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key.sign(bytes).to_bytes())
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn artifact(url: &str, bytes: &[u8]) -> Artifact {
    Artifact {
        url: url.into(),
        size: bytes.len() as u64,
        sha256: hex(&Sha256::digest(bytes)),
    }
}

fn manifest(at: &str, minimum: Option<&str>, platforms: &[(&str, Artifact)]) -> Manifest {
    Manifest {
        version: version(at),
        date: "2026-10-19T12:00:00Z".into(),
        notes: "obligatoria: arregla el inicio".into(),
        minimum_version: minimum.map(version),
        platforms: platforms
            .iter()
            .map(|(name, artifact)| ((*name).to_owned(), artifact.clone()))
            .collect(),
    }
}

#[test]
fn a_signed_manifest_verifies_and_any_change_or_other_key_is_refused() {
    let signer = key();
    let bytes = serde_json::to_vec(&manifest("1.2.0", None, &[])).unwrap();
    let signature = sign(&signer, &bytes);
    let public = signer.verifying_key();
    assert_eq!(
        verify(&bytes, &format!("{signature}\n"), &public)
            .unwrap()
            .version,
        version("1.2.0")
    );
    // One byte changed after signing.
    let mut altered = bytes.clone();
    let at = altered.iter().position(|byte| *byte == b'2').unwrap();
    altered[at] = b'9';
    assert_eq!(verify(&altered, &signature, &public), Err(Error::Signature));
    // Signed by someone else.
    assert_eq!(
        verify(&bytes, &sign(&key(), &bytes), &public),
        Err(Error::Signature)
    );
    assert_eq!(
        verify(&bytes, "no es base64", &public),
        Err(Error::Signature)
    );
    assert_eq!(verify(&bytes, "", &public), Err(Error::Signature));
    // A signature of bytes that are not a manifest proves nothing useful.
    let garbage = b"{\"version\":";
    assert_eq!(
        verify(garbage, &sign(&signer, garbage), &public),
        Err(Error::Manifest)
    );
}

#[test]
fn keys_are_base64_and_a_weak_one_is_refused() {
    let signer = key();
    let text = base64::engine::general_purpose::STANDARD.encode(signer.verifying_key().to_bytes());
    assert_eq!(
        parse_key(&format!(" {text}\n")),
        Some(signer.verifying_key())
    );
    assert_eq!(parse_key(""), None);
    assert_eq!(parse_key("AAAA"), None);
    // The identity point: a small-order key anyone can forge for.
    let mut identity = [0; 32];
    identity[0] = 1;
    assert_eq!(
        parse_key(&base64::engine::general_purpose::STANDARD.encode(identity)),
        None
    );
}

#[test]
fn a_newer_release_is_offered_mandatory_only_below_its_minimum() {
    let linux = artifact("https://example.org/rune-x86_64-linux.gz", b"rune");
    let release = manifest("1.2.0", Some("1.1.0"), &[("linux-x86_64", linux.clone())]);
    let offered = |installed: &str| offer(release.clone(), &version(installed), "linux-x86_64");
    assert_eq!(offered("1.2.0"), None);
    assert_eq!(offered("1.3.0"), None);
    let below = offered("1.0.9").unwrap();
    assert!(below.mandatory);
    assert_eq!(below.artifact, Some(linux.clone()));
    assert!(!offered("1.1.0").unwrap().mandatory);
    assert!(!offered("1.1.5").unwrap().mandatory);
    // A pre-release of the minimum is older than it.
    assert!(offered("1.1.0-rc.1").unwrap().mandatory);
    // Without a minimum nothing is mandatory.
    let free = manifest("1.2.0", None, &[("linux-x86_64", linux.clone())]);
    assert!(
        !offer(free, &version("0.1.0"), "linux-x86_64")
            .unwrap()
            .mandatory
    );
    // Nothing for this platform: offered, without an artifact.
    let other = offer(release.clone(), &version("1.0.0"), "windows-x86_64").unwrap();
    assert!(other.mandatory && other.artifact.is_none());
    // An artifact that could not be trusted to install counts as none.
    for bad in [
        Artifact {
            url: "http://example.org/rune.gz".into(),
            ..linux.clone()
        },
        Artifact {
            size: MAX_ARTIFACT + 1,
            ..linux.clone()
        },
        Artifact {
            sha256: "abc".into(),
            ..linux.clone()
        },
    ] {
        let release = manifest("1.2.0", None, &[("linux-x86_64", bad)]);
        assert_eq!(
            offer(release, &version("1.0.0"), "linux-x86_64")
                .unwrap()
                .artifact,
            None
        );
    }
}

#[test]
fn the_minimum_is_the_newest_mandatory_tag_up_to_the_release() {
    let tags = [
        ("v0.9.0", "obligatoria: el servicio cambió"),
        ("v1.0.0", "Primera versión pública"),
        ("v1.1.0", "Obligatoria. Arregla el inicio de sesión"),
        (
            "v1.2.0",
            "Notas sobre las versiones obligatorias anteriores",
        ),
        ("v1.3.0", "OBLIGATORIA"),
        ("v2.0.0-rc.1", "obligatoria (candidata)"),
        ("nightly", "obligatoria"),
        ("1.4.0", "obligatoria sin la v"),
    ];
    let minimum = |at: &str| minimum_version(&version(at), tags.iter().copied());
    assert_eq!(minimum("1.2.0"), Some(version("1.1.0")));
    assert_eq!(minimum("1.3.0"), Some(version("1.3.0")));
    assert_eq!(minimum("1.0.0"), Some(version("0.9.0")));
    assert_eq!(minimum("2.0.0"), Some(version("2.0.0-rc.1")));
    assert_eq!(minimum("0.8.0"), None);
    assert!(mandatory("Versión obligatoria."));
    assert!(!mandatory("obligatorias"));
    assert!(!mandatory("no-obligatoriamente"));
}

#[test]
fn the_feed_is_two_files_under_one_address() {
    assert_eq!(
        Feed::at("http://127.0.0.1:8000/"),
        Feed {
            manifest: "http://127.0.0.1:8000/rune-update.json".into(),
            signature: "http://127.0.0.1:8000/rune-update.json.sig".into(),
        }
    );
    assert!(Feed::at(RELEASES)
        .manifest
        .starts_with("https://github.com/Niiihuel/rune/"));
}

/// Publish `manifest`, signed by `signer`, on `server`.
fn publish(server: &Server, signer: &SigningKey, manifest: &Manifest) {
    let bytes = serde_json::to_vec_pretty(manifest).unwrap();
    server.put(&format!("/{SIGNATURE}"), sign(signer, &bytes).into_bytes());
    server.put(&format!("/{MANIFEST}"), bytes);
}

#[tokio::test]
async fn a_check_reads_a_signed_feed_on_loopback_and_refuses_a_forged_one() {
    let signer = key();
    let release = manifest("9.0.0", Some("9.0.0"), &[]);
    let server = serve(0).await.unwrap();
    let client = client().unwrap();
    let feed = Feed::at(&server.base());
    let public = signer.verifying_key();
    // Nothing there yet.
    assert_eq!(
        check(&client, &feed, Some(&public)).await,
        Err(Error::Status(404))
    );
    publish(&server, &signer, &release);
    assert_eq!(
        check(&client, &feed, Some(&public)).await,
        Ok(release.clone())
    );
    assert_eq!(check(&client, &feed, None).await, Err(Error::NoKey));
    // The same feed, verified with another key.
    assert_eq!(
        check(&client, &feed, Some(&key().verifying_key())).await,
        Err(Error::Signature)
    );
    // A manifest changed after signing.
    server.put(
        &format!("/{MANIFEST}"),
        serde_json::to_vec_pretty(&manifest("9.0.1", None, &[])).unwrap(),
    );
    assert_eq!(
        check(&client, &feed, Some(&public)).await,
        Err(Error::Signature)
    );
    // A manifest larger than any real one.
    server.put(&format!("/{MANIFEST}"), vec![b' '; MAX_MANIFEST + 1]);
    assert_eq!(
        check(&client, &feed, Some(&public)).await,
        Err(Error::TooLarge)
    );
}

fn program(size: usize) -> Vec<u8> {
    (0..size).map(|at| (at * 31 % 251) as u8).collect()
}

/// A server with `body` at `/rune.gz`, and its artifact.
async fn artifact_server(body: &[u8]) -> (Server, Artifact) {
    let server = serve(0).await.unwrap();
    server.put("/rune.gz", body.to_vec());
    let wanted = artifact(&format!("{}/rune.gz", server.base()), body);
    (server, wanted)
}

#[tokio::test]
async fn a_download_is_kept_only_when_its_size_and_digest_match() {
    let body = program(300_000);
    let (server, good) = artifact_server(&body).await;
    let client = client().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    let path = download(&client, &good, folder.path(), |have, whole| {
        seen.push((have, whole))
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), body);
    assert_eq!(seen.first(), Some(&(0, body.len() as u64)));
    assert_eq!(seen.last(), Some(&(body.len() as u64, body.len() as u64)));
    // Asked again (its installation failed): there already, not fetched.
    let before = server.requests().len();
    assert_eq!(
        download(&client, &good, folder.path(), |_, _| {}).await,
        Ok(path.clone())
    );
    assert_eq!(server.requests().len(), before);

    // Another digest: what came is deleted.
    let mut wrong = good.clone();
    wrong.sha256 = hex(&Sha256::digest(b"otra cosa"));
    assert_eq!(
        download(&client, &wrong, folder.path(), |_, _| {}).await,
        Err(Error::Digest)
    );
    assert!(!folder
        .path()
        .join(format!("rune-{}.part", wrong.sha256))
        .exists());
    assert!(!folder
        .path()
        .join(format!("rune-{}", wrong.sha256))
        .exists());
    // Larger than signed: stopped and deleted.
    let mut short = wrong.clone();
    short.size = 1000;
    assert_eq!(
        download(&client, &short, folder.path(), |_, _| {}).await,
        Err(Error::Digest)
    );
    assert!(!folder
        .path()
        .join(format!("rune-{}.part", short.sha256))
        .exists());
    // Smaller than signed: kept to resume, never installed.
    let mut long = wrong;
    long.size = body.len() as u64 + 10;
    assert_eq!(
        download(&client, &long, folder.path(), |_, _| {}).await,
        Err(Error::Incomplete)
    );
}

#[tokio::test]
async fn a_cut_download_resumes_where_it_stopped() {
    let body = program(400_000);
    let (server, wanted) = artifact_server(&body).await;
    server.configure(|files| files.cut = Some(("/rune.gz".into(), 150_000)));
    let client = client().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let first = download(&client, &wanted, folder.path(), |_, _| {}).await;
    assert!(
        matches!(first, Err(Error::Network | Error::Incomplete)),
        "{first:?}"
    );
    let part = folder.path().join(format!("rune-{}.part", wanted.sha256));
    // What arrived before the cut, as far as the client read it.
    let kept = std::fs::metadata(&part).unwrap().len();
    assert!(kept > 0 && kept <= 150_000, "{kept}");
    let path = download(&client, &wanted, folder.path(), |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
    assert_eq!(
        server.requests(),
        vec![("/rune.gz".into(), None), ("/rune.gz".into(), Some(kept))]
    );
}

#[tokio::test]
async fn a_server_without_ranges_sends_it_again_from_the_start() {
    let body = program(200_000);
    let (server, wanted) = artifact_server(&body).await;
    server.configure(|files| {
        files.cut = Some(("/rune.gz".into(), 50_000));
        files.no_ranges = true;
    });
    let client = client().unwrap();
    let folder = tempfile::tempdir().unwrap();
    assert!(download(&client, &wanted, folder.path(), |_, _| {})
        .await
        .is_err());
    let path = download(&client, &wanted, folder.path(), |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

/// A tiny "program": the ELF magic and a marker.
#[cfg(unix)]
fn elf(marker: &str) -> Vec<u8> {
    [b"\x7fELF".as_slice(), marker.as_bytes()].concat()
}

#[cfg(unix)]
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[cfg(unix)]
#[test]
fn an_update_replaces_the_installation_whole_or_not_at_all() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let install = Install::at(root.path().join("app"));
    let source = root.path().join("rune-built");
    std::fs::write(&source, elf("1.0.0")).unwrap();
    // `rune --install`: this very program becomes the first version.
    let program = install.adopt(&source, &version("1.0.0")).unwrap();
    assert_eq!(program, install.root().join("current/rune"));
    assert_eq!(std::fs::read(&program).unwrap(), elf("1.0.0"));
    assert_eq!(install.current().as_deref(), Some("1.0.0"));
    let mode = std::fs::metadata(&program).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111, "executable");
    let running = std::fs::canonicalize(&program).unwrap();
    assert!(install.runs(&running));
    assert!(!install.runs(&source));

    let write = |name: &str, bytes: &[u8]| {
        let path = root.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    // A release that is not a Linux program changes nothing.
    let wrong = write("wrong.gz", &gzip(b"MZ windows"));
    assert!(install.apply(&wrong, &version("1.1.0")).is_err());
    // Nor does a damaged archive.
    let damaged = write("damaged.gz", &gzip(&elf("1.1.0"))[..20]);
    assert!(install.apply(&damaged, &version("1.1.0")).is_err());
    assert_eq!(install.current().as_deref(), Some("1.0.0"));
    assert_eq!(std::fs::read(&program).unwrap(), elf("1.0.0"));
    let names = |install: &Install| {
        let mut names: Vec<String> = std::fs::read_dir(install.root().join("versions"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&install), ["1.0.0"], "no half-written version left");

    let newer = write("newer.gz", &gzip(&elf("1.1.0")));
    assert_eq!(install.apply(&newer, &version("1.1.0")).unwrap(), program);
    assert_eq!(std::fs::read(&program).unwrap(), elf("1.1.0"));
    // The one running before stays, for whoever still runs it.
    assert_eq!(std::fs::read(&running).unwrap(), elf("1.0.0"));
    let newest = write("newest.gz", &gzip(&elf("1.2.0")));
    install.apply(&newest, &version("1.2.0")).unwrap();
    assert_eq!(names(&install), ["1.1.0", "1.2.0"]);
    assert_eq!(std::fs::read(&program).unwrap(), elf("1.2.0"));
    assert!(!install.root().join("current.new").exists());
    // The same version again is nothing to do.
    install.apply(&newest, &version("1.2.0")).unwrap();
    assert_eq!(install.current().as_deref(), Some("1.2.0"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_release_goes_from_a_loopback_feed_to_the_installation() {
    let signer = key();
    let built = gzip(&elf("2.0.0"));
    let server = serve(0).await.unwrap();
    server.put("/rune-x86_64-linux.gz", built.clone());
    let linux = artifact(&format!("{}/rune-x86_64-linux.gz", server.base()), &built);
    publish(
        &server,
        &signer,
        &manifest("2.0.0", Some("2.0.0"), &[(&platform(), linux)]),
    );

    let client = client().unwrap();
    let manifest = check(
        &client,
        &Feed::at(&server.base()),
        Some(&signer.verifying_key()),
    )
    .await
    .unwrap();
    let offered = offer(manifest, &version("1.0.0"), &platform()).unwrap();
    assert!(offered.mandatory);
    let root = tempfile::tempdir().unwrap();
    let install = Install::at(root.path());
    let file = download(
        &client,
        offered.artifact.as_ref().unwrap(),
        &install.downloads(),
        |_, _| {},
    )
    .await
    .unwrap();
    let program = install.apply(&file, &offered.version).unwrap();
    assert_eq!(std::fs::read(program).unwrap(), elf("2.0.0"));
}
