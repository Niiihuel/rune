use rune_i18n::es;
use std::collections::HashSet;
use std::time::Duration;

use reqwest::{redirect::Policy, Client, RequestBuilder};
use rune_model::{ImageKind, ProfileBanner, RemoteImage, Rgb, Server, ServerId, User, UserId};
use rune_provider::Snapshot;
use serde::{de::DeserializeOwned, Deserialize};
use tokio::time::Instant;

use crate::{auth::Attempt, AuthError};

const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Http {
    client: Client,
    pub authorize: String,
    token: String,
    authorization: String,
    guilds: String,
    revoke: String,
}

// Secrets and remote response bodies deliberately do not implement Debug.
pub(crate) struct Session {
    pub access_token: String,
    pub expires: Instant,
    pub scopes: Vec<String>,
    pub snapshot: Box<Snapshot>,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
    // refresh_token is intentionally ignored: this first slice reauthorizes.
}

#[derive(Deserialize)]
struct Authorization {
    application: Application,
    scopes: Vec<String>,
    expires: String,
    user: Option<DiscordUser>,
}

#[derive(Deserialize)]
struct Application {
    id: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    global_name: Option<String>,
    #[serde(default)]
    bot: bool,
    accent_color: Option<u32>,
    avatar: Option<String>,
    #[serde(default, deserialize_with = "present_banner")]
    banner: Option<serde_json::Value>,
}

// Option<Value> normally collapses null and omitted fields. Keep that
// distinction at the adapter boundary so partial accounts cannot clear artwork.
fn present_banner<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error> {
    serde_json::Value::deserialize(deserializer).map(Some)
}

fn profile_banner(id: UserId, value: Option<&serde_json::Value>) -> ProfileBanner {
    match value {
        None => ProfileBanner::Unknown,
        Some(serde_json::Value::Null) => ProfileBanner::Absent,
        Some(hash) => artwork(id, hash.as_str(), ImageKind::Banner)
            .map(ProfileBanner::Image)
            .unwrap_or_default(),
    }
}

#[derive(Deserialize)]
struct Guild {
    id: String,
    name: String,
    #[serde(default)]
    owner: bool,
}

pub(crate) fn snowflake(id: &str) -> Result<u64, AuthError> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AuthError(es("Discord devolvió un identificador inválido.")));
    }
    id.parse::<u64>()
        .ok()
        .filter(|n| *n != 0)
        .ok_or(AuthError(es("Discord devolvió un identificador inválido.")))
}

/// Optional public artwork never makes otherwise valid identity data fail.
fn artwork(id: UserId, hash: Option<&str>, kind: ImageKind) -> Option<RemoteImage> {
    let hash = hash?;
    let digest = hash.strip_prefix("a_").unwrap_or(hash);
    if digest.len() != 32 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let (folder, size) = match kind {
        ImageKind::Artwork => ("avatars", 128),
        ImageKind::Banner => ("banners", 512),
        ImageKind::AttachmentPreview
        | ImageKind::Clip
        | ImageKind::Original
        | ImageKind::ThumbHash
        | ImageKind::Obscured => return None,
    };
    Some(RemoteImage {
        url: format!("https://cdn.discordapp.com/{folder}/{id}/{hash}.png?size={size}"),
        kind,
    })
}

fn validate_scopes(scopes: &[String]) -> Result<(), AuthError> {
    let unique: HashSet<_> = scopes.iter().map(String::as_str).collect();
    if unique.len() != 2 || !unique.contains("identify") || !unique.contains("guilds") {
        return Err(AuthError(es(
            "Discord no concedió exactamente los permisos de identidad y servidores solicitados.",
        )));
    }
    Ok(())
}

// Tests change only endpoints and proxy selection, retaining the production
// transport policy so the redirect trap verifies the real credential boundary.
fn client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!(
            "DiscordBot (https://github.com/Niiihuel/rune, ",
            env!("CARGO_PKG_VERSION"),
            ") RUNE"
        ))
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// `builder` with REST routed as `proxy` says: a proxy named has no way
/// around it. `None` for an address reqwest refuses.
fn proxied(
    builder: reqwest::ClientBuilder,
    proxy: &rune_provider::ApiProxy,
) -> Option<reqwest::ClientBuilder> {
    use rune_provider::ApiProxy;
    Some(match proxy {
        ApiProxy::Automatic => builder,
        ApiProxy::Direct => builder.no_proxy(),
        ApiProxy::Url(url) => builder.no_proxy().proxy(reqwest::Proxy::all(url).ok()?),
    })
}

impl Http {
    /// Requests going as `proxy` says.
    pub fn new(proxy: &rune_provider::ApiProxy) -> Result<Self, AuthError> {
        let client = proxied(client_builder(), proxy)
            .ok_or(AuthError(es("La dirección del proxy no es válida.")))?
            .build()
            .map_err(|_| AuthError(es("No se pudo iniciar el cliente HTTPS.")))?;
        Ok(Self {
            client,
            authorize: "https://discord.com/oauth2/authorize".into(),
            token: "https://discord.com/api/oauth2/token".into(),
            authorization: "https://discord.com/api/v10/oauth2/@me".into(),
            guilds: "https://discord.com/api/v10/users/@me/guilds".into(),
            revoke: "https://discord.com/api/oauth2/token/revoke".into(),
        })
    }

    #[cfg(test)]
    pub fn local(base: &str) -> Self {
        Self {
            client: client_builder().no_proxy().build().unwrap(),
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            authorization: format!("{base}/authorization"),
            guilds: format!("{base}/guilds"),
            revoke: format!("{base}/revoke"),
        }
    }

    pub async fn exchange(
        &self,
        client_id: &str,
        attempt: Attempt,
        code: String,
    ) -> Result<Session, AuthError> {
        let issued = Instant::now();
        // Public-client HTTP is an interoperability experiment; do not retry
        // without PKCE or fall back to a personal token or embedded secret.
        let token: Token = self
            .json(self.client.post(&self.token).form(&[
                ("client_id", client_id),
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", attempt.redirect.as_str()),
                ("code_verifier", &attempt.verifier),
            ]))
            .await?;
        if !token.token_type.eq_ignore_ascii_case("bearer")
            || token.access_token.is_empty()
            || token.access_token.len() > 8192
            || token.expires_in == 0
            || token.expires_in > 366 * 86400
        {
            return Err(AuthError(es("Discord devolvió una sesión inválida.")));
        }
        let token_scopes: Vec<_> = token
            .scope
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect();
        validate_scopes(&token_scopes)?;
        let authorization: Authorization = self
            .json(
                self.client
                    .get(&self.authorization)
                    .bearer_auth(&token.access_token),
            )
            .await?;
        if authorization.application.id != client_id {
            return Err(AuthError(es(
                "La autorización pertenece a otra aplicación.",
            )));
        }
        validate_scopes(&authorization.scopes)?;
        let expiry = chrono::DateTime::parse_from_rfc3339(&authorization.expires)
            .map_err(|_| AuthError(es("Discord devolvió una caducidad inválida.")))?;
        let remaining = (expiry.with_timezone(&chrono::Utc) - chrono::Utc::now())
            .to_std()
            .map_err(|_| AuthError(es("La autorización de Discord ya caducó.")))?;
        let expires = (issued + Duration::from_secs(token.expires_in))
            .min(Instant::now() + remaining.min(Duration::from_secs(366 * 86400)));
        let user = authorization.user.ok_or(AuthError(es(
            "Discord no devolvió la identidad autorizada.",
        )))?;
        if user.bot {
            return Err(AuthError(es(
                "Esta integración requiere una identidad de usuario autorizada por OAuth2.",
            )));
        }
        let id = UserId(snowflake(&user.id)?);
        let me = User {
            identity: Default::default(),
            avatar: artwork(id, user.avatar.as_deref(), ImageKind::Artwork),
            banner: profile_banner(id, user.banner.as_ref()),
            id,
            name: user.username,
            display_name: user.global_name,
            accent: Rgb(user.accent_color.unwrap_or(0x7164c4) & 0xffffff),
            bot: false,
            pronouns: None,
            about: None,
            // A person, never a bot: their default picture is one of six by
            // when the account was made, as the official client chooses it.
            default_avatar: Some(rune_model::RemoteImage {
                url: format!(
                    "https://cdn.discordapp.com/embed/avatars/{}.png",
                    (id.0 >> 22) % 6
                ),
                kind: ImageKind::Artwork,
            }),
        };
        let mut servers = Vec::new();
        let mut seen = HashSet::new();
        let mut after = 0u64;
        // The documented user maximum is 200; tolerate future pagination,
        // but fail on repeated cursors or an unreasonable response count.
        for page in 0..10 {
            let mut request = self
                .client
                .get(&self.guilds)
                .bearer_auth(&token.access_token)
                .query(&[("limit", "200")]);
            if after != 0 {
                request = request.query(&[("after", after.to_string())]);
            }
            let guilds: Vec<Guild> = self.json(request).await?;
            if guilds.len() > 200 {
                return Err(AuthError(es(
                    "Discord devolvió demasiados servidores en una página.",
                )));
            }
            let count = guilds.len();
            let previous = after;
            for guild in guilds {
                let id = snowflake(&guild.id)?;
                if id <= previous || !seen.insert(id) {
                    return Err(AuthError(es("La paginación de servidores no avanzó.")));
                }
                after = after.max(id);
                servers.push(Server {
                    stickers: Vec::new(),
                    emojis: Vec::new(),
                    icon: None,
                    id: ServerId(id),
                    name: guild.name,
                    accent: Rgb(0x7164c4),
                    roles: Vec::new(),
                    owner: guild.owner.then_some(me.id),
                    description: None,
                    options: Default::default(),
                });
            }
            if count < 200 {
                break;
            }
            if page == 9 {
                return Err(AuthError(es(
                    "El listado de servidores superó el límite de esta integración.",
                )));
            }
        }
        if expires <= Instant::now() {
            return Err(AuthError(es(
                "La autorización de Discord caducó durante la conexión.",
            )));
        }
        Ok(Session {
            access_token: token.access_token,
            expires,
            scopes: vec!["identify".into(), "guilds".into()],
            snapshot: Box::new(Snapshot {
                users: vec![me.clone()],
                me: Some(me),
                servers,
                ..Snapshot::default()
            }),
        })
    }

    pub async fn revoke(&self, client_id: &str, token: String) -> bool {
        self.client
            .post(&self.revoke)
            .form(&[
                ("client_id", client_id),
                ("token", token.as_str()),
                ("token_type_hint", "access_token"),
            ])
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, AuthError> {
        let mut response = request
            .send()
            .await
            .map_err(|_| AuthError(es("No se pudo completar la conexión HTTPS con Discord.")))?;
        if !response.status().is_success() {
            return Err(AuthError(match response.status().as_u16() {
                400 | 401 => es("Discord rechazó la autorización. Revisa Public Client, el ID y el redirect registrado; el canje público directo es experimental."),
                403 => es("Discord no autorizó esta operación."),
                429 => es("Discord pidió esperar antes de volver a intentar."),
                _ => es("Discord devolvió un error. No se reintentó automáticamente."),
            }));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE as u64)
        {
            return Err(AuthError(es(
                "La respuesta de Discord superó el límite permitido.",
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AuthError(es("No se pudo leer la respuesta de Discord.")))?
        {
            if chunk.len() > MAX_RESPONSE - body.len() {
                return Err(AuthError(es(
                    "La respuesta de Discord superó el límite permitido.",
                )));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| AuthError(es("Discord devolvió una respuesta inesperada.")))
    }
}

// Desktop session and games (escritorio-sesion-y-juegos).
#[cfg(test)]
mod proxy_tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// A proxy on loopback hears where the request goes and refuses it:
    /// nothing reaches Discord.
    #[tokio::test]
    async fn a_named_proxy_carries_the_requests() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy =
            rune_provider::ApiProxy::url(&format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let http = Http::new(&proxy).unwrap();
        let heard = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = [0u8; 256];
            let read = socket.read(&mut head).await.unwrap();
            let _ = socket
                .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                .await;
            String::from_utf8_lossy(&head[..read])
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        assert!(http.client.get(&http.guilds).send().await.is_err());
        assert_eq!(heard.await.unwrap(), "CONNECT discord.com:443 HTTP/1.1");
        assert!(Http::new(&rune_provider::ApiProxy::Url("not a url".into())).is_err());
    }
}

#[cfg(test)]
mod artwork_tests {
    use super::*;

    #[test]
    fn profile_banner_maps_omitted_null_malformed_and_received_hash() {
        let hash = "a_0123456789abcdef0123456789abcdef";
        let mut value = serde_json::json!({"id":"42","username":"fixture"});
        let decoded: DiscordUser = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            profile_banner(UserId(42), decoded.banner.as_ref()),
            ProfileBanner::Unknown
        );
        for field in [
            serde_json::Value::Null,
            serde_json::json!(true),
            serde_json::json!("../x"),
            serde_json::json!(hash),
        ] {
            value["banner"] = field.clone();
            let decoded: DiscordUser = serde_json::from_value(value.clone()).unwrap();
            let expected = match field {
                serde_json::Value::Null => ProfileBanner::Absent,
                serde_json::Value::String(ref s) if s == hash => {
                    ProfileBanner::Image(RemoteImage {
                        url: format!("https://cdn.discordapp.com/banners/42/{hash}.png?size=512"),
                        kind: ImageKind::Banner,
                    })
                }
                _ => ProfileBanner::Unknown,
            };
            assert_eq!(
                profile_banner(UserId(42), decoded.banner.as_ref()),
                expected
            );
        }
    }
}
