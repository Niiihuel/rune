use super::*;

#[test]
fn reply_is_bound_to_one_capability_and_rejects_unbounded_or_injected_input() {
    let capability = "a".repeat(protocol::CAPABILITY_BYTES);
    let valid = protocol::encode_reply(&capability, "SYNTHETIC_SESSION_MARKER").unwrap();
    assert!(protocol::decode_reply(&capability, valid.as_bytes()).is_ok());
    for bad in [
        format!("{}:SYNTHETIC_SESSION_MARKER", "b".repeat(64)),
        format!("{capability}:short"),
        format!("{capability}:SYNTHETIC_SESSION_MARKER\nsecond-result"),
        format!("{capability}:{}", "x".repeat(2049)),
    ] {
        assert!(protocol::decode_reply(&capability, bad.as_bytes()).is_err());
    }
}

#[test]
fn main_document_origin_excludes_lookalikes_credentials_and_insecure_ports() {
    assert!(protocol::discord_origin("https://discord.com/login"));
    assert!(protocol::discord_origin(
        "https://discord.com:443/channels/@me"
    ));
    for invalid in [
        "http://discord.com/login",
        "https://discord.com.evil.invalid",
        "https://evil.invalid@discord.com",
        "https://discord.com:444",
        "file:///tmp/discord.com",
    ] {
        assert!(!protocol::discord_origin(invalid));
    }
}

#[test]
fn capability_and_errors_never_include_candidate_data() {
    assert_ne!(
        protocol::capability().unwrap(),
        protocol::capability().unwrap()
    );
    for code in [
        None,
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(5),
        Some(7),
        Some(8),
        Some(9),
        Some(protocol::RUNTIME_MISSING),
    ] {
        assert!(!format!("{:?}", protocol::helper_error(code)).contains("SYNTHETIC"));
    }
}

#[test]
fn without_webview2_every_window_says_where_microsoft_installs_it() {
    let code = Some(protocol::RUNTIME_MISSING);
    for error in [
        protocol::helper_error(code),
        protocol::captcha_error(code),
        protocol::engine_error(code),
    ] {
        assert!(error
            .to_string()
            .contains("https://go.microsoft.com/fwlink/p/?LinkId=2124703"));
    }
    assert!(!protocol::engine_error(Some(6))
        .to_string()
        .contains("WebView2"));
}

#[test]
fn engine_failure_messages_do_not_conflate_crash_memory_api_and_unknown() {
    let messages: Vec<_> = [4, 7, 8, 9]
        .into_iter()
        .map(|code| protocol::helper_error(Some(code)).to_string())
        .collect();
    for (index, message) in messages.iter().enumerate() {
        assert!(messages[..index].iter().all(|other| other != message));
    }
    assert!(messages[0].contains("falló"));
    assert!(messages[1].contains("límite de memoria"));
    assert!(messages[2].contains("pidió terminar"));
    assert!(messages[3].contains("no identificada"));
}

#[cfg(unix)]
fn shell(script: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("sh");
    command.args(["-c", script]);
    command
}

#[cfg(unix)]
#[tokio::test]
async fn helper_protocol_uses_private_input_and_requires_clean_exit() {
    let successful =
        shell("IFS= read -r nonce || :; printf '%s:SYNTHETIC_SESSION_MARKER' \"$nonce\"");
    assert!(acquire_from(successful, Duration::from_secs(2))
        .await
        .is_ok());
    let failed =
        shell("IFS= read -r nonce || :; printf '%s:SYNTHETIC_SESSION_MARKER' \"$nonce\"; exit 4");
    assert!(acquire_from(failed, Duration::from_secs(2)).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn helper_output_is_bounded_and_malformed_results_are_not_credentials() {
    for script in [
        "printf 'unrelated-output'",
        "head -c 2200 /dev/zero",
        "exit 0",
    ] {
        assert!(acquire_from(shell(script), Duration::from_secs(2))
            .await
            .is_err());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn helper_timeout_and_cancellation_terminate_owned_child() {
    // The fixture writes its PID to a private test pipe; it contains no token.
    let location = std::env::temp_dir().join(format!(
        "rune-login-child-{}-{}",
        std::process::id(),
        protocol::capability().unwrap()
    ));
    let script = "printf '%s' \"$$\" > \"$1\"; exec sleep 30";
    for cancel in [false, true] {
        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", script, "rune-login-test"])
            .arg(&location);
        let task = tokio::spawn(acquire_from(command, Duration::from_millis(100)));
        for _ in 0..100 {
            if location.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let pid = std::fs::read_to_string(&location).unwrap();
        if cancel {
            task.abort();
        }
        let result = task.await;
        assert!(result.is_err() || result.unwrap().is_err());
        let mut gone = false;
        for _ in 0..100 {
            let status = tokio::process::Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .await
                .unwrap();
            if !status.success() {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(gone, "the owned login helper must be terminated");
        std::fs::remove_file(&location).unwrap();
    }
}

fn challenge() -> rune_discord_session::Challenge {
    rune_discord_session::Challenge::new(
        "synthetic-site-key".into(),
        Some("SYNTHETIC_RQDATA".into()),
        true,
    )
    .unwrap()
}

#[test]
fn challenge_input_carries_only_what_the_page_needs_and_rejects_tampering() {
    let capability = "a".repeat(protocol::CAPABILITY_BYTES);
    let input = protocol::encode_challenge(&capability, &challenge());
    assert_eq!(
        input.as_str(),
        format!("{capability}\n1\nsynthetic-site-key\nSYNTHETIC_RQDATA")
    );
    assert!(input.len() <= protocol::MAX_CHALLENGE);
    let (returned, decoded) = protocol::decode_challenge(&input).unwrap();
    assert_eq!(returned, capability);
    assert_eq!(decoded.sitekey(), "synthetic-site-key");
    assert_eq!(decoded.rqdata(), Some("SYNTHETIC_RQDATA"));
    assert!(decoded.invisible());
    let plain = format!("{capability}\n0\nkey\n");
    let plain = protocol::decode_challenge(&plain).unwrap();
    assert!(plain.1.rqdata().is_none() && !plain.1.invisible());
    for bad in [
        format!("{}\n0\nkey\n", "z".repeat(64)),
        format!("{capability}\n2\nkey\n"),
        format!("{capability}\n0\n../key\n"),
        format!("{capability}\n0\nkey\nwith space"),
        format!("{capability}\n0\nkey"),
    ] {
        assert!(protocol::decode_challenge(&bad).is_none(), "{bad}");
    }
}

#[test]
fn an_answer_is_bound_to_its_capability_and_bounded() {
    let capability = "b".repeat(protocol::CAPABILITY_BYTES);
    assert!(
        protocol::decode_answer(&capability, format!("{capability}:P1_SYNTHETIC").as_bytes())
            .is_ok()
    );
    for bad in [
        format!("{}:P1_SYNTHETIC", "c".repeat(64)),
        format!("{capability}:"),
        format!("{capability}:two words"),
        format!("{capability}:{}", "x".repeat(protocol::MAX_ANSWER + 1)),
    ] {
        assert!(protocol::decode_answer(&capability, bad.as_bytes()).is_err());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn verification_helper_answers_once_and_a_closed_window_sends_nothing() {
    // The fixture echoes the capability it read, as the real page does.
    let solved = shell("IFS= read -r nonce || :; printf '%s:SYNTHETIC_HCAPTCHA_ANSWER' \"$nonce\"");
    assert!(verify_from(solved, &challenge(), Duration::from_secs(2))
        .await
        .is_ok());
    for (script, expected) in [
        ("cat >/dev/null; exit 2", protocol::captcha_error(Some(2))),
        ("cat >/dev/null; exit 10", protocol::captcha_error(Some(10))),
        (
            "cat >/dev/null; exec sleep 5",
            protocol::captcha_error(Some(3)),
        ),
    ] {
        let error = verify_from(shell(script), &challenge(), Duration::from_millis(300))
            .await
            .unwrap_err();
        assert_eq!(error, expected, "{script}");
    }
    for script in ["printf 'unrelated-output'", "head -c 8400 /dev/zero"] {
        assert!(
            verify_from(shell(script), &challenge(), Duration::from_secs(2))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn a_login_made_before_the_window_is_handed_over_once() {
    let secret = SessionSecret::try_from_local("SYNTHETIC_SESSION_MARKER".into()).unwrap();
    let login = PrefetchedLogin::new(Ok(secret));
    assert!(login.acquire().await.is_ok(), "the first sign-in takes it");
    let failed = PrefetchedLogin::new(Err(protocol::helper_error(Some(4))));
    let error = failed.acquire().await.unwrap_err();
    assert!(!cancelled(&error), "a crash is not a cancel");
    assert!(cancelled(&protocol::helper_error(Some(2))));
}
