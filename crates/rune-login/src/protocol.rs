use rune_discord_session::{Challenge, ChallengeAnswer, LoginError, SessionSecret};
use rune_i18n::es;
use zeroize::Zeroizing;

pub const CAPABILITY_BYTES: usize = 64;
pub const MAX_SECRET: usize = 2048;
pub const MAX_REPLY: usize = CAPABILITY_BYTES + 1 + MAX_SECRET;
/// The bound `Challenge` enforces on an answer.
pub const MAX_ANSWER: usize = 8192;
pub const MAX_ANSWER_REPLY: usize = CAPABILITY_BYTES + 1 + MAX_ANSWER;
/// Capability, flag, site key and request data, one per line.
pub const MAX_CHALLENGE: usize = CAPABILITY_BYTES + 3 + 128 + 1 + 4096;
/// What the engine's page reads of its browser, as JSON (`Engine`).
#[cfg_attr(not(windows), allow(dead_code))]
pub const MAX_ENGINE: usize = 4096;

pub fn capability() -> Result<String, LoginError> {
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(|_| {
        LoginError::new(es(
            "No se pudo obtener aleatoriedad segura para iniciar sesión.",
        ))
    })?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn valid_capability(value: &str) -> bool {
    value.len() == CAPABILITY_BYTES && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn decode_reply(capability: &str, bytes: &[u8]) -> Result<SessionSecret, LoginError> {
    let invalid = || LoginError::new(es("La respuesta de la ventana de acceso no es válida."));
    if bytes.len() > MAX_REPLY || !valid_capability(capability) {
        return Err(invalid());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let (returned, secret) = text.split_once(':').ok_or_else(invalid)?;
    if returned != capability {
        return Err(invalid());
    }
    SessionSecret::try_from_local(secret.to_owned())
}

/// What the verification helper reads from its private input. The values
/// that bind the answer to the rejected request never reach the helper.
pub fn encode_challenge(capability: &str, challenge: &Challenge) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "{capability}\n{}\n{}\n{}",
        u8::from(challenge.invisible()),
        challenge.sitekey(),
        challenge.rqdata().unwrap_or_default()
    ))
}

pub fn decode_challenge(input: &str) -> Option<(&str, Challenge)> {
    let mut lines = input.splitn(4, '\n');
    let capability = lines.next().filter(|value| valid_capability(value))?;
    let invisible = match lines.next()? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let sitekey = lines.next()?.to_owned();
    let rqdata = lines.next()?;
    let rqdata = (!rqdata.is_empty()).then(|| rqdata.to_owned());
    Some((capability, Challenge::new(sitekey, rqdata, invisible)?))
}

/// The page's answer is `{capability}:{answer}`, as a login reply.
pub fn decode_answer(capability: &str, bytes: &[u8]) -> Result<ChallengeAnswer, LoginError> {
    let invalid = || LoginError::new(es("La respuesta de la verificación no es válida."));
    if bytes.len() > MAX_ANSWER_REPLY || !valid_capability(capability) {
        return Err(invalid());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let (returned, answer) = text.split_once(':').ok_or_else(invalid)?;
    if returned != capability {
        return Err(invalid());
    }
    ChallengeAnswer::try_from_local(answer.to_owned())
}

/// The helpers' exit code when Windows has no WebView2 Runtime.
pub const RUNTIME_MISSING: i32 = 11;

/// What RUNE says without WebView2, with Microsoft's own installer.
fn runtime_missing() -> &'static str {
    es("Falta WebView2 Runtime de Microsoft, el navegador de la ventana de acceso en Windows. Instálalo desde https://go.microsoft.com/fwlink/p/?LinkId=2124703 y vuelve a intentarlo.")
}

/// Why the engine's probe told nothing (Windows).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn engine_error(code: Option<i32>) -> LoginError {
    LoginError::new(match code {
        Some(RUNTIME_MISSING) => runtime_missing(),
        _ => es("No se pudo leer la identidad del navegador de la ventana de acceso."),
    })
}

pub fn captcha_error(code: Option<i32>) -> LoginError {
    LoginError::new(match code {
        Some(2) => es("Se cerró la verificación sin completarla. No se envió nada."),
        Some(3) => es("Se agotó el tiempo de la verificación. No se envió nada."),
        Some(4 | 7 | 8 | 9) => {
            es("El navegador de la ventana de verificación falló. No se envió nada.")
        }
        Some(5) => es("Esta compilación no incluye la ventana de verificación."),
        Some(RUNTIME_MISSING) => runtime_missing(),
        Some(10) => es(
            "La verificación no pudo cargarse. Puedes aceptar la invitación desde el navegador.",
        ),
        _ => es("No se pudo completar la verificación. No se envió nada."),
    })
}

#[cfg(test)]
pub fn encode_reply(capability: &str, candidate: &str) -> Option<Zeroizing<String>> {
    if !valid_capability(capability)
        || !(16..=MAX_SECRET).contains(&candidate.len())
        || !candidate.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return None;
    }
    Some(Zeroizing::new(format!("{capability}:{candidate}")))
}

/// Whether `value` is a URL of `https://discord.com`, the login's own origin.
pub fn discord_origin(value: &str) -> bool {
    same_origin(value, |host| host == "discord.com")
}

/// Whether `value` is hCaptcha's, which the challenge frames load.
pub fn challenge_origin(value: &str) -> bool {
    same_origin(value, challenge_domain)
}

pub fn challenge_domain(host: &str) -> bool {
    host.strip_suffix("hcaptcha.com")
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
}

/// RUNE's own verification page, on a reserved name that never resolves:
/// the challenge is shown as RUNE's, never as a Discord document.
pub const CAPTCHA_PAGE: &str = "https://captcha.rune.invalid/";

pub fn captcha_page(value: &str) -> bool {
    value == CAPTCHA_PAGE
}

/// An HTTPS origin on its default port whose host `host` accepts, with no
/// credentials in the URL: compared parsed, never by prefix.
fn same_origin(value: &str, host: impl Fn(&str) -> bool) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let credentials = !url.username().is_empty() || url.password().is_some();
    match url.origin() {
        url::Origin::Tuple(scheme, url::Host::Domain(name), port) => {
            scheme == "https" && port == 443 && !credentials && host(&name)
        }
        _ => false,
    }
}

pub fn helper_error(code: Option<i32>) -> LoginError {
    LoginError::new(match code {
        Some(2) => es("Se canceló el inicio de sesión."),
        Some(3) => es("Se agotó el tiempo de inicio de sesión."),
        Some(4) => es("El navegador de la ventana de acceso falló. Vuelve a intentarlo."),
        Some(5) => es("Esta compilación no incluye la ventana de inicio de sesión."),
        Some(7) => es("El navegador cerró la ventana de acceso al superar su límite de memoria."),
        Some(8) => es("La aplicación pidió terminar el navegador de la ventana de acceso."),
        Some(9) => {
            es("El navegador de la ventana de acceso terminó por una causa no identificada.")
        }
        Some(RUNTIME_MISSING) => runtime_missing(),
        _ => es("No se pudo completar el inicio de sesión en la ventana temporal."),
    })
}
