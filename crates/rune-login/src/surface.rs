//! What each temporary window loads, accepts and hands back, the same for
//! both engines: WebKitGTK on Linux (`webview`) and WebView2 on Windows
//! (`windows`, windows-login).

use std::io::{Read, Write};
use std::time::Duration;

use rune_discord_session::Challenge;
use rune_i18n::t;
use zeroize::Zeroizing;

use crate::protocol::{self, CAPTCHA_PAGE};
use crate::{CAPTCHA_LIFETIME, LIFETIME};

/// How a window ended: the helper's exit code and its one reply.
pub type Outcome = (i32, Option<Zeroizing<String>>);

/// The login helper: its capability in, the session out, through `open`'s
/// window.
pub fn run_login(open: impl FnOnce(Surface, Duration) -> Outcome) -> i32 {
    let Some(input) =
        private_input(protocol::CAPABILITY_BYTES).filter(|input| protocol::valid_capability(input))
    else {
        return 6;
    };
    let (code, result) = open(login(&input, None), LIFETIME);
    reply(code, result)
}

/// The verification helper: the challenge in, the person's answer out.
pub fn run_captcha(open: impl FnOnce(Surface, Duration) -> Outcome) -> i32 {
    let Some(input) = private_input(protocol::MAX_CHALLENGE) else {
        return 6;
    };
    let Some((capability, challenge)) = protocol::decode_challenge(&input) else {
        return 6;
    };
    let surface = captcha(capability, &challenge, CAPTCHA_LIFETIME, None);
    let (code, result) = open(surface, CAPTCHA_LIFETIME);
    reply(code, result)
}

/// The helpers are not public token-output commands: they require the
/// private pipes their parent created, and never print an interactive result.
fn private_input(max: usize) -> Option<Zeroizing<String>> {
    if !piped() {
        return None;
    }
    let mut input = Zeroizing::new(String::new());
    std::io::stdin()
        .take((max + 1) as u64)
        .read_to_string(&mut input)
        .ok()
        .filter(|read| *read <= max)?;
    Some(input)
}

/// Whether stdin and stdout are pipes, never a file or a terminal.
#[cfg(unix)]
pub fn piped() -> bool {
    use std::os::unix::fs::FileTypeExt;
    ["/proc/self/fd/0", "/proc/self/fd/1"]
        .iter()
        .all(|fd| std::fs::metadata(fd).is_ok_and(|metadata| metadata.file_type().is_fifo()))
}

/// Whether stdin and stdout are pipes, never a file or a console.
#[cfg(windows)]
pub fn piped() -> bool {
    use ::windows::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_PIPE};
    use ::windows::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE]
        .into_iter()
        .all(|which| {
            // SAFETY: the handles are this process's own standard ones, only queried.
            unsafe { GetStdHandle(which) }.is_ok_and(|handle| {
                !handle.is_invalid() && unsafe { GetFileType(handle) } == FILE_TYPE_PIPE
            })
        })
}

pub fn reply(code: i32, result: Option<Zeroizing<String>>) -> i32 {
    if let Some(result) = result {
        if std::io::stdout()
            .write_all(result.as_bytes())
            .and_then(|()| std::io::stdout().flush())
            .is_err()
        {
            return 6;
        }
    }
    code
}

const LOGIN_SCRIPT: &str = include_str!("handoff.js");
const CAPTCHA_SCRIPT: &str = include_str!("captcha.js");
const CAPTCHA_HTML: &str = include_str!("captcha.html");

/// The exit code and reply for one value taken from the page.
pub type Finish = Box<dyn Fn(Zeroizing<String>) -> (i32, Option<Zeroizing<String>>)>;

pub struct Surface {
    pub title: &'static str,
    pub size: (i32, i32),
    /// The only document that may hand a result back.
    pub page: fn(&str) -> bool,
    /// WebKit's registrable domain of `page`, for the challenge's storage.
    #[cfg_attr(windows, allow(dead_code))]
    pub domain: &'static str,
    /// Local HTML to serve at `url`, or `url` itself from the network.
    pub html: Option<String>,
    pub url: &'static str,
    /// WebKit's message handler; WebView2 has one channel per window.
    #[cfg_attr(windows, allow(dead_code))]
    pub handler: &'static str,
    /// Injected at the start of every top-level document; it checks its origin.
    pub script: String,
    #[cfg_attr(windows, allow(dead_code))]
    pub script_origin: &'static str,
    /// Evaluated in the main frame after a wake; returns a bounded string.
    pub take: String,
    pub finish: Finish,
}

/// Discord's login page; the session comes back as `{capability}:{session}`.
pub fn login(capability: &str, fixture: Option<&str>) -> Surface {
    let take = format!("(() => {{ if (window !== window.top || location.origin !== 'https://discord.com') return null; const take = window['__rune_take_{capability}']; if (typeof take !== 'function') return null; const value = take(); return typeof value === 'string' && value.length <= {} ? value : null; }})()", protocol::MAX_REPLY);
    let owned = capability.to_owned();
    Surface {
        title: t("RUNE · Iniciar sesión en Discord"),
        size: (920, 720),
        page: protocol::discord_origin,
        domain: "discord.com",
        html: fixture.map(str::to_owned),
        url: "https://discord.com/login",
        handler: "runeLoginWake",
        script: LOGIN_SCRIPT.replace("__RUNE_CAPABILITY__", capability),
        script_origin: "https://discord.com/*",
        take,
        finish: Box::new(move |reply| {
            if protocol::decode_reply(&owned, reply.as_bytes()).is_err() {
                (6, None)
            } else {
                (0, Some(reply))
            }
        }),
    }
}

/// One hCaptcha on RUNE's own page; its answer comes back once as
/// `{capability}:{answer}`. Expired or unloadable challenges send nothing.
pub fn captcha(
    capability: &str,
    challenge: &Challenge,
    lifetime: Duration,
    fixture: Option<&str>,
) -> Surface {
    let config = Zeroizing::new(
        serde_json::json!({
            "capability": capability,
            "sitekey": challenge.sitekey(),
            "rqdata": challenge.rqdata(),
            "invisible": challenge.invisible(),
            "lifetime": lifetime.as_millis() as u64,
            "texts": {
                "failed": t("La verificación no pudo cargarse. No se envió nada."),
                "verified": t("Verificación completa. Volviendo a RUNE…"),
                "ready": t("Completa la verificación para entrar al servidor."),
            },
        })
        .to_string(),
    );
    let max = protocol::CAPABILITY_BYTES + ":verified:".len() + protocol::MAX_ANSWER;
    let take = format!("(() => {{ if (window !== window.top || location.href !== '{CAPTCHA_PAGE}') return null; const take = window['__rune_captcha_take_{capability}']; if (typeof take !== 'function') return null; const value = take(); return typeof value === 'string' && value.length <= {max} ? value : null; }})()");
    let owned = capability.to_owned();
    Surface {
        title: t("RUNE · Verificación"),
        size: (480, 640),
        page: protocol::captcha_page,
        domain: "rune.invalid",
        html: Some(fixture.map_or_else(captcha_html, str::to_owned)),
        url: CAPTCHA_PAGE,
        handler: "runeCaptchaWake",
        script: CAPTCHA_SCRIPT.replace("__RUNE_CAPTCHA__", &config),
        script_origin: "https://captcha.rune.invalid/*",
        take,
        finish: Box::new(move |value| captcha_reply(&owned, &value)),
    }
}

/// RUNE's verification page in the current language: its `__RUNE_*__`
/// places take the language's code and texts, escaped for HTML.
fn captcha_html() -> String {
    [
        ("__RUNE_LANG__", rune_i18n::current().code()),
        ("__RUNE_TITLE__", t("RUNE · Verificación")),
        ("__RUNE_HEADING__", t("Verificación para entrar al servidor")),
        ("__RUNE_LOADING__", t("Cargando la verificación…")),
        ("__RUNE_START__", t("Empezar la verificación")),
        (
            "__RUNE_NOTE__",
            t("Página propia de RUNE. Solo se envía la respuesta a esta solicitud; cerrar la ventana no envía nada."),
        ),
    ]
    .into_iter()
    .fold(CAPTCHA_HTML.to_owned(), |html, (place, text)| {
        html.replace(place, &escape_html(text))
    })
}

fn escape_html(text: &str) -> String {
    text.chars()
        .fold(String::with_capacity(text.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                c => out.push(c),
            }
            out
        })
}

/// `{capability}:verified:{answer}` becomes the helper's reply; an expired or
/// unloadable challenge ends with its own code and no reply.
fn captcha_reply(capability: &str, value: &str) -> (i32, Option<Zeroizing<String>>) {
    let Some(kind) = value
        .strip_prefix(capability)
        .and_then(|rest| rest.strip_prefix(':'))
    else {
        return (6, None);
    };
    match kind.split_once(':') {
        Some(("verified", answer)) => {
            let reply = Zeroizing::new(format!("{capability}:{answer}"));
            match protocol::decode_answer(capability, reply.as_bytes()) {
                Ok(_) => (0, Some(reply)),
                Err(_) => (6, None),
            }
        }
        Some(("expired", "")) => (3, None),
        Some(("error", "")) => (10, None),
        _ => (6, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_verified_page_answer_with_this_capability_becomes_a_reply() {
        let capability = "d".repeat(protocol::CAPABILITY_BYTES);
        let (code, reply) =
            captcha_reply(&capability, &format!("{capability}:verified:P1_SYNTHETIC"));
        assert_eq!(code, 0);
        assert_eq!(
            reply.as_deref().map(String::as_str),
            Some(format!("{capability}:P1_SYNTHETIC").as_str())
        );
        for (value, expected) in [
            (format!("{capability}:expired:"), 3),
            (format!("{capability}:error:"), 10),
            (format!("{capability}:verified:"), 6),
            (format!("{capability}:verified:two words"), 6),
            (format!("{}:verified:P1_SYNTHETIC", "e".repeat(64)), 6),
            (format!("{capability}:closed:"), 6),
        ] {
            let (code, reply) = captcha_reply(&capability, &value);
            assert_eq!(code, expected, "{value}");
            assert!(reply.is_none());
        }
        assert!(protocol::captcha_page("https://captcha.rune.invalid/"));
        for other in [
            "https://captcha.rune.invalid/other",
            "https://discord.com/",
            "http://captcha.rune.invalid/",
            "https://captcha.rune.invalid.evil.test/",
        ] {
            assert!(!protocol::captcha_page(other));
        }
    }

    #[test]
    fn the_verification_page_is_written_in_the_current_language() {
        let spanish = captcha_html();
        assert!(spanish.contains(r#"<html lang="es">"#) && !spanish.contains("__RUNE_"));
        assert!(spanish.contains("<h1>Verificación para entrar al servidor</h1>"));
        let _english = rune_i18n::scoped(rune_i18n::Language::English);
        let english = captcha_html();
        assert!(english.contains(r#"<html lang="en">"#) && !english.contains("__RUNE_"));
        assert!(english.contains("<title>RUNE · Verification</title>"));
        assert_eq!(
            escape_html(r#"<a href="x">'&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn challenge_hosts_do_not_include_lookalikes_or_insecure_origins() {
        assert!(protocol::challenge_origin(
            "https://newassets.hcaptcha.com/captcha/"
        ));
        assert!(protocol::challenge_origin("https://hcaptcha.com/1/api.js"));
        for value in [
            "http://hcaptcha.com",
            "https://hcaptcha.com.evil.invalid",
            "https://evilhcaptcha.com",
            "https://evil.invalid@hcaptcha.com",
            "https://hcaptcha.com:444",
        ] {
            assert!(!protocol::challenge_origin(value), "{value}");
        }
    }
}
