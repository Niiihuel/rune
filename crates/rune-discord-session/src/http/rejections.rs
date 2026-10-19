//! Bounded rejection metadata, independent of remote messages but for the
//! notice of a server's AutoMod, bounded and read only for its codes.
//! Challenge values are kept only for an hCaptcha a person can solve here,
//! erased on drop.

use rune_i18n::es;
use serde::{de::IgnoredAny, Deserialize};
use zeroize::Zeroizing;

use super::{read_bounded, LoginError, RestError};
use crate::{printable, Challenge};

pub(super) enum Rejection {
    InvalidSession,
    AccountVerification,
    ActionVerification,
    /// An hCaptcha challenge, set as the official client expects it:
    /// HTTP 400 with `captcha_key` and `captcha_service: "hcaptcha"`.
    Challenge(Issued),
    /// A refusal the service names with a code RUNE can explain.
    Refused(&'static str),
    // AutoMod (automod).
    /// A server's AutoMod refused the write.
    Blocked(Blocked),
}

// AutoMod (automod).
/// What a server's AutoMod tells whoever wrote what it refused (JSON codes
/// 200000 to 200002, `AUTOMOD_MESSAGE_BLOCKED` and its kin in the official
/// client, which shows the answer's `message`, see docs/REA.md). `None`
/// when it says nothing of its own.
#[derive(Debug)]
pub(crate) struct Blocked {
    pub notice: Option<String>,
}

/// Characters of an AutoMod notice kept: the custom message is at most 150.
const MAX_NOTICE: usize = 300;
/// What Discord answers when a rule has no message of its own (documented
/// for 200000 and 200001): RUNE says it in the person's language instead.
const GENERIC_NOTICES: [&str; 2] = [
    "Message was blocked by automatic moderation",
    "Title was blocked by automatic moderation",
];

/// The AutoMod codes (documented, and the official client's `AbortCodes`).
fn automod(code: u64) -> bool {
    (200_000..=200_002).contains(&code)
}

#[derive(Deserialize)]
struct Notice {
    message: Option<String>,
}

/// What an AutoMod refusal's body tells, read only for such a refusal:
/// printable text, bounded, and not Discord's own generic sentence.
fn notice(bytes: &[u8]) -> Option<String> {
    let message = serde_json::from_slice::<Notice>(bytes).ok()?.message?;
    let message: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_NOTICE)
        .collect();
    let message = message.trim();
    (!message.is_empty() && !GENERIC_NOTICES.contains(&message)).then(|| message.to_owned())
}

/// A challenge with the values that bind its answer to the one rejected
/// request; they travel back only as headers of that same request.
pub(crate) struct Issued {
    pub challenge: Challenge,
    pub rqtoken: Option<Zeroizing<String>>,
    pub session: Option<Zeroizing<String>>,
    /// The request it stopped, as `"POST /channels/1/messages"`.
    pub route: String,
}

impl std::fmt::Debug for Issued {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Issued([REDACTED])")
    }
}

impl Rejection {
    pub(super) fn error(self, authenticated: bool) -> RestError {
        let reason = LoginError::new(match &self {
            Self::InvalidSession => es("Discord rechazó la sesión; iniciá sesión otra vez."),
            Self::AccountVerification => {
                es("Discord requiere verificar la cuenta en su aplicación oficial.")
            }
            Self::ActionVerification | Self::Challenge(_) => {
                es("Discord requiere una verificación para completar esta acción.")
            }
            Self::Refused(reason) => reason,
            Self::Blocked(_) => es("La moderación automática del servidor lo bloqueó."),
        });
        let ends_session =
            authenticated && matches!(self, Self::InvalidSession | Self::AccountVerification);
        let refused = matches!(self, Self::Refused(_) | Self::Blocked(_));
        let (challenge, blocked) = match self {
            Self::Challenge(issued) => (Some(Box::new(issued)), None),
            Self::Blocked(blocked) => (None, Some(blocked)),
            _ => (None, None),
        };
        RestError {
            reason,
            ambiguous: false,
            ends_session,
            invitation: None,
            refused,
            challenge,
            blocked,
        }
    }
}

/// Only integer codes and the presence of a non-null challenge are retained.
/// The ignored values are never materialized as owned strings or JSON trees.
#[derive(Deserialize)]
struct Metadata {
    code: Option<u64>,
    captcha_key: Option<IgnoredAny>,
    captcha_sitekey: Option<IgnoredAny>,
    captcha_rqdata: Option<IgnoredAny>,
    captcha_rqtoken: Option<IgnoredAny>,
    captcha_session_id: Option<IgnoredAny>,
}

/// The challenge fields the official web client reads
/// (`extractCaptchaPropsFromResponse`, see docs/REA.md).
#[derive(Deserialize)]
struct Fields {
    captcha_service: Option<String>,
    captcha_sitekey: Option<String>,
    captcha_rqdata: Option<String>,
    captcha_rqtoken: Option<String>,
    captcha_session_id: Option<String>,
    should_serve_invisible: Option<bool>,
}

pub(super) async fn inspect(response: reqwest::Response) -> Option<Rejection> {
    inspect_coded(response).await.0
}

/// [`inspect`], with the integer error code the body names, if any.
pub(super) async fn inspect_coded(response: reqwest::Response) -> (Option<Rejection>, Option<u64>) {
    let solvable = response.status() == reqwest::StatusCode::BAD_REQUEST;
    let Ok(bytes) = read_bounded(response, 64 * 1024).await else {
        return (None, None);
    };
    let bytes = Zeroizing::new(bytes);
    // Serde structs also accept positional arrays; the service error contract
    // requires an object, so an unrelated array cannot invalidate a session.
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return (None, None);
    }
    let Ok(data) = serde_json::from_slice::<Metadata>(&bytes) else {
        return (None, None);
    };
    let code = data.code;
    (rejection(&bytes, data, solvable), code)
}

fn rejection(bytes: &[u8], data: Metadata, solvable: bool) -> Option<Rejection> {
    // JSON error codes: https://docs.discord.com/developers/topics/opcodes-and-status-codes#json-json-error-codes
    // Authentication/account requirements take priority over action challenges.
    match data.code {
        Some(50014) => Some(Rejection::InvalidSession),
        Some(40002 | 50178) => Some(Rejection::AccountVerification),
        Some(60003) => Some(Rejection::ActionVerification),
        Some(code) if refusal(code).is_some() && data.captcha_key.is_none() => {
            refusal(code).map(Rejection::Refused)
        }
        // AutoMod (automod): only then is the answer's text read.
        Some(code) if automod(code) && data.captcha_key.is_none() => {
            Some(Rejection::Blocked(Blocked {
                notice: notice(bytes),
            }))
        }
        _ if solvable && data.captcha_key.is_some() => Some(
            challenge(bytes)
                .map(Rejection::Challenge)
                .unwrap_or(Rejection::ActionVerification),
        ),
        _ if [
            data.captcha_key,
            data.captcha_sitekey,
            data.captcha_rqdata,
            data.captcha_rqtoken,
            data.captcha_session_id,
        ]
        .iter()
        .any(Option::is_some) =>
        {
            Some(Rejection::ActionVerification)
        }
        _ => None,
    }
}

/// What a refusal about people and conversations means, by the service's
/// JSON error code (named in the official client's `AbortCodes`, see
/// docs/DISCORD_SESSION.md).
fn refusal(code: u64) -> Option<&'static str> {
    Some(match code {
        10013 => es("Esa persona ya no existe."),
        30002 => es("Llegaste al máximo de amigos que permite Discord."),
        30004 => es("El grupo ya tiene el máximo de personas."),
        40003 => es("Abriste demasiadas conversaciones nuevas; esperá un rato."),
        50007 => es("Esa persona no recibe mensajes directos tuyos."),
        80001 => es("Esa persona no acepta solicitudes de amistad."),
        80002 => es("No se pueden enviar solicitudes de amistad a un bot."),
        80003 | 50096 => es("No podés enviarte una solicitud a vos."),
        80004 => es("No encontramos a nadie con ese nombre de usuario."),
        80007 => es("Ya son amigos."),
        340007 => es("Discord limitó las solicitudes de amistad de esta cuenta."),
        _ => return None,
    })
}

/// Only hCaptcha opens in RUNE's window; another service, or any value out of
/// bounds, stays a verification for the official application.
fn challenge(bytes: &[u8]) -> Option<Issued> {
    let fields: Fields = serde_json::from_slice(bytes).ok()?;
    let rqtoken = fields.captcha_rqtoken.map(Zeroizing::new);
    let session = fields.captcha_session_id.map(Zeroizing::new);
    let challenge = Challenge::new(
        fields.captcha_sitekey?,
        fields.captcha_rqdata,
        fields.should_serve_invisible.unwrap_or(false),
    )?;
    (fields.captcha_service.as_deref() == Some("hcaptcha")
        && rqtoken
            .as_deref()
            .is_none_or(|value| printable(value, 2048))
        && session.as_deref().is_none_or(|value| printable(value, 512)))
    .then_some(Issued {
        challenge,
        rqtoken,
        session,
        route: String::new(),
    })
}
