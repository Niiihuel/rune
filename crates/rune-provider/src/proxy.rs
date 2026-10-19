//! How a provider's REST client reaches its service: through the proxies of
//! the environment (`HTTP_PROXY` and the like, the usual), straight, or
//! through one HTTP(S) proxy the person named, as Serein offers it
//! (`discord-api/src/proxy.rs`, read 2026-10-18). Only an address: a proxy
//! that asks for a user and a password is refused, since keeping those
//! credentials is not authorized (owner, 2026-10-18). The address is never
//! written to a log.

use rune_i18n::es;

/// The longest address taken.
const MAX_URL: usize = 2048;

/// Where REST requests go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ApiProxy {
    /// As the environment says, the way every program on the desktop does.
    #[default]
    Automatic,
    /// Straight to the service, whatever the environment says.
    Direct,
    /// Through this proxy, and only through it: `http://` or `https://`,
    /// a host and maybe a port, nothing else.
    Url(String),
}

impl ApiProxy {
    /// The proxy at `url`, if it is only an address; the reason otherwise,
    /// to show (in Spanish, through `t`).
    pub fn url(url: &str) -> Result<Self, &'static str> {
        valid_url(url.trim()).map(|()| Self::Url(url.trim().to_owned()))
    }

    /// As the host keeps it: `automatic`, `direct` or `url <address>`.
    pub fn kept(&self) -> String {
        match self {
            Self::Automatic => "automatic".into(),
            Self::Direct => "direct".into(),
            Self::Url(url) => format!("url {url}"),
        }
    }

    /// What [`Self::kept`] wrote; anything else, or an address that is no
    /// longer valid, is `None`.
    pub fn from_kept(kept: &str) -> Option<Self> {
        match kept.trim() {
            "automatic" => Some(Self::Automatic),
            "direct" => Some(Self::Direct),
            other => Self::url(other.strip_prefix("url ")?).ok(),
        }
    }
}

/// An `http://` or `https://` address of a host and maybe a port, with an
/// optional `/` and nothing else: no user or password, path, query or
/// fragment, spaces or backslashes.
fn valid_url(url: &str) -> Result<(), &'static str> {
    let refused = es(
        "Usá la dirección de un proxy HTTP o HTTPS, sin usuario ni contraseña, ruta, consulta ni fragmento: http://host:puerto.",
    );
    if url.is_empty()
        || url.len() > MAX_URL
        || url.contains('@')
        || url
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b == b'\\')
    {
        return Err(refused);
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .ok_or(refused)?;
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = match authority.strip_prefix('[') {
        // An IPv6 address, bracketed.
        Some(inner) => {
            let (address, after) = inner.split_once(']').ok_or(refused)?;
            if address.is_empty()
                || !address
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
            {
                return Err(refused);
            }
            match after {
                "" => (address, None),
                port => (address, Some(port.strip_prefix(':').ok_or(refused)?)),
            }
        }
        None => match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let host_ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'));
    let port_ok = port.is_none_or(|port| {
        !port.is_empty()
            && port.bytes().all(|b| b.is_ascii_digit())
            && port.parse::<u16>().is_ok_and(|port| port > 0)
    });
    if host_ok && port_ok {
        Ok(())
    } else {
        Err(refused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_bare_http_proxy_address_is_taken() {
        for url in [
            "http://127.0.0.1:8080",
            "https://proxy.example/",
            "http://[::1]:3128/",
            " HTTP://Proxy.Example:80 ",
        ] {
            assert!(ApiProxy::url(url).is_ok(), "{url}");
        }
        // Each refused for one reason: no scheme or another one, a user or
        // password, anything after the host, a bad port, a broken host.
        for url in [
            "",
            "http://",
            "lan.test:3128",
            "ftp://lan.test",
            "socks4://lan.test:1080",
            "https://ana:clave@lan.test",
            "http://@lan.test:3128",
            "https://lan.test/salida",
            "http://lan.test/x/../y",
            "http://lan.test/?clave=1",
            "https://lan.test/#arriba",
            "http://lan.test\\otro",
            "http://lan test",
            "http://lan.test:",
            "http://lan.test:0",
            "http://lan.test:65536",
            "http://[fe80::1",
        ] {
            assert!(ApiProxy::url(url).is_err(), "{url}");
        }
        let long = format!("http://{}.example", "x".repeat(MAX_URL));
        assert!(ApiProxy::url(&long).is_err());
    }

    #[test]
    fn the_choice_is_kept_and_read_back_and_anything_else_is_none() {
        for proxy in [
            ApiProxy::Automatic,
            ApiProxy::Direct,
            ApiProxy::url("http://127.0.0.1:8080").unwrap(),
        ] {
            assert_eq!(ApiProxy::from_kept(&proxy.kept()), Some(proxy));
        }
        assert_eq!(ApiProxy::from_kept("url http://a:b@c"), None);
        assert_eq!(ApiProxy::from_kept("socks"), None);
    }
}
