use rune_i18n::{es, t};
use std::collections::HashMap;
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

use crate::AuthError;

const MAX_HEADERS: usize = 8192;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

// Deliberately no Debug implementation: a verifier is a one-use secret.
pub(crate) struct Attempt {
    pub verifier: String,
    pub state: String,
    pub redirect: Url,
}

pub(crate) fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_string() -> Result<String, AuthError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| AuthError(es("No se pudo obtener aleatoriedad segura.")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

impl Attempt {
    pub fn new(redirect: Url) -> Result<Self, AuthError> {
        Ok(Self {
            verifier: random_string()?,
            state: random_string()?,
            redirect,
        })
    }

    pub fn authorize_url(&self, endpoint: &str, client_id: &str) -> String {
        let mut url = Url::parse(endpoint).expect("fixed authorization endpoint");
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", self.redirect.as_str())
            .append_pair("scope", "identify guilds")
            .append_pair("state", &self.state)
            .append_pair("code_challenge", &challenge(&self.verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("prompt", "consent");
        url.into()
    }

    pub async fn receive(self, listener: TcpListener) -> Result<(Self, String), AuthError> {
        tokio::time::timeout(CALLBACK_TIMEOUT, async {
            // Invalid requests cannot hold the listener or grow memory forever.
            for _ in 0..32 {
                let (mut stream, peer) = listener.accept().await
                    .map_err(|_| AuthError(es("No se pudo recibir la respuesta del navegador.")))?;
                if !peer.ip().is_loopback() { continue; }
                let request = tokio::time::timeout(REQUEST_TIMEOUT, read_headers(&mut stream)).await;
                let result = match request {
                    Ok(Ok(headers)) => self.parse(&headers),
                    _ => Callback::Invalid,
                };
                let accepted = matches!(result, Callback::Code(_) | Callback::Denied);
                let (head, body) = if accepted {
                    ("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
                        t("Respuesta recibida. Vuelve a RUNE para ver el resultado."))
                } else {
                    ("HTTP/1.1 400 Bad Request\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: 0\r\n\r\n", "")
                };
                let response = format!("{head}{body}");
                let _ = tokio::time::timeout(REQUEST_TIMEOUT, stream.write_all(response.as_bytes())).await;
                match result {
                    Callback::Code(code) => return Ok((self, code)),
                    Callback::Denied => return Err(AuthError(es("La autorización fue rechazada o cancelada en Discord."))),
                    Callback::Invalid => {}
                }
            }
            Err(AuthError(es("Demasiadas respuestas de autorización inválidas. Vuelve a intentarlo.")))
        }).await.map_err(|_| AuthError(es("Se agotó el tiempo para autorizar. Vuelve a iniciar sesión.")))?
    }

    pub(crate) fn parse(&self, headers: &str) -> Callback {
        let mut lines = headers.split("\r\n");
        let Some(first) = lines.next() else {
            return Callback::Invalid;
        };
        let parts: Vec<_> = first.split(' ').collect();
        if parts.len() != 3
            || parts[0] != "GET"
            || parts[2] != "HTTP/1.1"
            || !parts[1].starts_with('/')
            || parts[1].starts_with("//")
        {
            return Callback::Invalid;
        }
        let mut host = None;
        for line in lines.take_while(|line| !line.is_empty()) {
            let Some((name, value)) = line.split_once(':') else {
                return Callback::Invalid;
            };
            if name.eq_ignore_ascii_case("host") && host.replace(value.trim()).is_some() {
                return Callback::Invalid;
            }
            if name.eq_ignore_ascii_case("transfer-encoding")
                || (name.eq_ignore_ascii_case("content-length") && value.trim() != "0")
            {
                return Callback::Invalid;
            }
        }
        let expected_host = format!(
            "127.0.0.1:{}",
            self.redirect.port_or_known_default().unwrap_or(80)
        );
        if host != Some(expected_host.as_str()) {
            return Callback::Invalid;
        }
        let Ok(url) = self.redirect.join(parts[1]) else {
            return Callback::Invalid;
        };
        if url.origin() != self.redirect.origin()
            || url.path() != self.redirect.path()
            || url.fragment().is_some()
        {
            return Callback::Invalid;
        }
        let mut params = HashMap::new();
        for (key, value) in url.query_pairs() {
            if params
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Callback::Invalid;
            }
        }
        if params.get("state") != Some(&self.state)
            || params
                .get("iss")
                .is_some_and(|iss| iss != "https://discord.com")
        {
            return Callback::Invalid;
        }
        match (params.remove("code"), params.remove("error")) {
            (Some(code), None) if !code.is_empty() && code.len() <= 2048 => Callback::Code(code),
            (None, Some(_)) => Callback::Denied,
            _ => Callback::Invalid,
        }
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum Callback {
    Invalid,
    Denied,
    Code(String),
}

impl std::fmt::Debug for Callback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "Invalid",
            Self::Denied => "Denied",
            Self::Code(_) => "Code([redacted])",
        })
    }
}

pub(crate) async fn read_headers(stream: &mut TcpStream) -> Result<String, AuthError> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    while bytes.len() < MAX_HEADERS {
        let n = stream
            .read(&mut buffer)
            .await
            .map_err(|_| AuthError(es("Respuesta local ilegible.")))?;
        if n == 0 {
            return Err(AuthError(es("Respuesta local incompleta.")));
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.len() > MAX_HEADERS {
            break;
        }
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            return String::from_utf8(bytes[..end + 4].to_vec())
                .map_err(|_| AuthError(es("Respuesta local inválida.")));
        }
    }
    Err(AuthError(es("Respuesta local demasiado grande.")))
}
