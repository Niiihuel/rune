//! The one identity this unofficial adapter presents to Discord: its web
//! client in the browser engine of RUNE's login and hCaptcha windows on this
//! computer, as the owner decided on 2026-10-19 (Serein presents a browser
//! too). REST, the Gateway, voice and those windows all present it, so no
//! transport tells another apart.
//!
//! The engine's own rather than Serein's fixed Chrome: hCaptcha sees the
//! engine whatever a user agent says. On Linux that is WebKitGTK, known
//! beforehand, which the web client names Safari (`navigator.vendor` is
//! Apple's). On Windows it is the installed WebView2 (windows-login): a page
//! in it tells what it reads ([`Engine`]) before anything goes out. The
//! properties are the ones the web client derives from those (REA of build
//! 641004 in `docs/REA.md`). Nothing here carries account data.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    LazyLock, OnceLock,
};

use base64::Engine as _;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT_LANGUAGE, ORIGIN, REFERER, USER_AGENT,
};
use reqwest::Method;
use rune_i18n::es;
use serde::{Deserialize, Serialize};

use crate::{LoginError, SessionLogin};

/// WebKitGTK 2.52's own `Version/`, so the windows' default and ours agree.
const VERSION: &str = "60.5";
/// The web client's build read last (REA, 2026-10-17), until its page names
/// the current one.
pub(crate) const FALLBACK_BUILD: u64 = 641_004;
static BUILD: AtomicU64 = AtomicU64::new(FALLBACK_BUILD);

/// What a page in the engine of the login and verification windows reads
/// of its browser: the web client derives its identity from these alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Engine {
    /// `navigator.userAgent`.
    pub user_agent: String,
    /// `navigator.vendor`.
    #[serde(default)]
    pub vendor: String,
    /// `navigator.userAgentData.brands`, in order; none without client hints.
    #[serde(default)]
    pub brands: Vec<Brand>,
    /// `navigator.userAgentData.platform`.
    #[serde(default)]
    pub platform: String,
    /// `navigator.languages[0]`.
    pub language: String,
    /// `Intl.DateTimeFormat().resolvedOptions().timeZone`.
    pub timezone: String,
}

/// One of `navigator.userAgentData.brands`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Brand {
    pub brand: String,
    pub version: String,
}

/// The engine every request presents once a page in it told what it reads.
static PRESENTED: OnceLock<Engine> = OnceLock::new();

/// WebKitGTK's, which needs no page to know: its own user agent (pinned in
/// its windows), Apple's vendor, no client hints, and the system's language
/// and zone as WebKit gives them to pages.
static WEBKIT: LazyLock<Engine> = LazyLock::new(|| {
    Engine {
    user_agent: format!(
        "Mozilla/5.0 (X11; Linux {}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{VERSION} Safari/605.1.15",
        std::env::consts::ARCH
    ),
    vendor: "Apple Computer, Inc.".into(),
    brands: Vec::new(),
    platform: String::new(),
    language: LOCALE.clone(),
    timezone: TIMEZONE.clone(),
}
});

/// The engine presented: the windows' own once learnt, WebKitGTK's before.
fn engine() -> &'static Engine {
    PRESENTED.get().unwrap_or(&WEBKIT)
}

/// The user agent of every request and of the login and verification windows.
pub fn user_agent() -> &'static str {
    &engine().user_agent
}

/// The user agent learnt from the windows' engine, once it was: what a
/// WebView2 window keeps should its runtime change meanwhile.
pub fn learnt_user_agent() -> Option<&'static str> {
    PRESENTED.get().map(|engine| engine.user_agent.as_str())
}

/// Learn the engine of `login`'s windows before anything goes out; a host
/// whose engine is WebKitGTK's has nothing to tell. Once learnt, it stays.
pub(crate) async fn learn(login: &dyn SessionLogin) -> Result<(), LoginError> {
    if PRESENTED.get().is_some() {
        return Ok(());
    }
    if let Some(engine) = login.engine().await? {
        if !engine.valid() {
            return Err(LoginError::new(es(
                "El navegador de la ventana de acceso dio una identidad no válida.",
            )));
        }
        let _ = PRESENTED.set(engine);
    }
    Ok(())
}

/// The build every request and Identify names from now on.
pub(crate) fn set_build(build: u64) {
    BUILD.store(build, Ordering::Relaxed);
}

/// The build the web client's page names in `window.GLOBAL_ENV`.
pub(crate) fn build_in(page: &str) -> Option<u64> {
    const KEY: &str = "\"BUILD_NUMBER\":\"";
    let rest = &page[page.find(KEY)? + KEY.len()..];
    let digits = &rest[..rest.find('"')?];
    if !(1..=9).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|build| *build > 0)
}

/// The system's language as WebKit tells pages (`navigator.language`):
/// `es_AR.UTF-8` is `es-AR`; none, `C` or `POSIX` is `en-US`.
static LOCALE: LazyLock<String> = LazyLock::new(|| {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find_map(|value| {
            let tag = value.split([':', '.', '@']).next()?.replace('_', "-");
            let valid = (2..=16).contains(&tag.len())
                && tag != "POSIX"
                && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            valid.then_some(tag)
        })
        .unwrap_or_else(|| "en-US".into())
});

/// Discord's interface languages, in the web client's order (build 641004).
const LOCALES: [&str; 31] = [
    "en-US", "en-GB", "zh-CN", "zh-TW", "cs", "da", "nl", "fr", "de", "el", "hu", "it", "ja", "ko",
    "pl", "pt-BR", "ru", "es-419", "es-ES", "sv-SE", "tr", "bg", "uk", "fi", "no", "hr", "ro",
    "lt", "th", "vi", "hi",
];

/// The interface language the web client picks for `tag` when the account
/// names none: the same, its language, an alias, or its first relative.
fn discord_locale(tag: &str) -> &'static str {
    let language = tag.split('-').next().unwrap_or(tag);
    let alias = match language {
        "es" if tag == language => Some("es-ES"),
        "nb" | "nn" if tag == language => Some("no"),
        "zh" if tag.starts_with("zh-Hant") => Some("zh-TW"),
        _ => None,
    };
    LOCALES
        .into_iter()
        .find(|code| *code == tag)
        .or_else(|| LOCALES.into_iter().find(|code| *code == language))
        .or(alias)
        .or_else(|| {
            LOCALES
                .into_iter()
                .find(|code| code.split('-').next() == Some(language))
        })
        .unwrap_or("en-US")
}

/// The system's zone as WebKit's `Intl` reports it, or `UTC` as it does.
static TIMEZONE: LazyLock<String> = LazyLock::new(|| {
    let zone = |name: &str| {
        let name = name.trim_start_matches(':');
        let name = name.split_once("zoneinfo/").map_or(name, |(_, zone)| zone);
        let valid = (1..=64).contains(&name.len())
            && !name.starts_with('/')
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-+".contains(&b));
        valid.then(|| name.to_owned())
    };
    std::env::var("TZ")
        .ok()
        .and_then(|name| zone(&name))
        .or_else(|| zone(std::fs::read_link("/etc/localtime").ok()?.to_str()?))
        .unwrap_or_else(|| "UTC".into())
});

/// This launch's ids, drawn once as the web client draws them per page load.
// shortcut: the heartbeat session id stays the launch's; the web client
// draws another after a long idle. Upgrade if Discord ever refuses it.
static LAUNCH: LazyLock<[String; 2]> = LazyLock::new(|| [uuid(), uuid()]);

fn uuid() -> String {
    let mut bytes = [0u8; 16];
    // Without entropy the id is still well formed, and still the launch's.
    let _ = getrandom::fill(&mut bytes);
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// The web client's super properties, field for field and in its order.
#[derive(Serialize)]
pub(crate) struct Properties<'a> {
    os: &'static str,
    browser: &'static str,
    device: &'static str,
    device_model_name: &'static str,
    system_locale: &'a str,
    has_client_mods: bool,
    browser_user_agent: &'a str,
    browser_version: &'a str,
    os_version: &'static str,
    referrer: &'static str,
    referring_domain: &'static str,
    referrer_current: &'static str,
    referring_domain_current: &'static str,
    release_channel: &'static str,
    client_build_number: u64,
    client_event_source: Option<&'static str>,
    client_launch_id: &'static str,
    /// Its WebAssembly signs the launch only once loaded; before, as here, null.
    launch_signature: Option<&'static str>,
    client_heartbeat_session_id: &'static str,
    client_app_state: &'static str,
}

/// The Identify's `properties`: the same, and why the socket connects.
#[derive(Serialize)]
pub(crate) struct Identify {
    #[serde(flatten)]
    properties: Properties<'static>,
    is_fast_connect: bool,
    gateway_connect_reasons: &'static str,
}

pub(crate) fn identify() -> Identify {
    Identify {
        properties: engine().properties(),
        is_fast_connect: false,
        gateway_connect_reasons: "COLD_START",
    }
}

/// `X-Super-Properties` of the engine presented.
#[cfg(test)]
pub(crate) fn super_properties() -> String {
    engine().super_properties()
}

/// What the web client and its browser add to a request to Discord's API.
pub(crate) fn api_headers(method: &Method) -> HeaderMap {
    engine().api_headers(method)
}

/// Visible ASCII and spaces, bounded: safe in a header and in the JSON.
fn plain(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len()) && value.bytes().all(|b| (b' '..=b'~').contains(&b))
}

impl Engine {
    /// What a page told is a browser's, and fits in a header.
    fn valid(&self) -> bool {
        let tag = |value: &str, extra: &[u8]| {
            plain(value, 64)
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
        };
        self.user_agent.starts_with("Mozilla/5.0 (")
            && plain(&self.user_agent, 512)
            && (self.vendor.is_empty() || plain(&self.vendor, 64))
            && (self.platform.is_empty() || tag(&self.platform, b" "))
            && tag(&self.language, b"-")
            && tag(&self.timezone, b"/_-+")
            && self.brands.len() <= 8
            && self.brands.iter().all(|brand| {
                plain(&brand.brand, 64)
                    && !brand.brand.contains(['"', '\\'])
                    && tag(&brand.version, b".")
            })
    }

    /// The web client's `getOS` for a desktop engine.
    // shortcut: desktop engines only (no phone, tablet or BlackBerry
    // branches); RUNE opens nothing else.
    fn os(&self) -> &'static str {
        let agent = self.user_agent.to_ascii_lowercase();
        if agent.contains("windows") {
            "Windows"
        } else if agent.contains("mac") {
            "Mac OS X"
        } else if agent.contains("linux") {
            "Linux"
        } else {
            ""
        }
    }

    /// The web client's `getBrowser` for a desktop engine: Apple's vendor
    /// first, then the client hints' brands, then the user agent.
    fn browser(&self) -> &'static str {
        let brand = |name: &str| self.brands.iter().any(|brand| brand.brand == name);
        let agent = &self.user_agent;
        if self.vendor.contains("Apple") {
            "Safari"
        } else if brand("Microsoft Edge") {
            "Edge"
        } else if brand("Opera") {
            "Opera"
        } else if agent.contains("Edge") {
            "Edge"
        } else if agent.contains("Chrome") {
            "Chrome"
        } else if agent.contains("Firefox") {
            "Firefox"
        } else {
            ""
        }
    }

    /// platform.js's version, which the web client sends: Edge's own token
    /// (`Edg/`), else `Version/` (Safari), else Chrome's.
    fn browser_version(&self) -> &str {
        ["Edg/", "Edge/", "Version/", "Chrome/"]
            .into_iter()
            .find_map(|token| self.user_agent.split_once(token))
            .map_or("", |(_, rest)| {
                let end = rest
                    .find(|c: char| " ();/_-".contains(c))
                    .unwrap_or(rest.len());
                &rest[..end]
            })
    }

    /// platform.js names Windows NT 10.0, also Windows 11's, "10"; WebView2
    /// runs on nothing older, and WebKitGTK's Linux has no version.
    fn os_version(&self) -> &'static str {
        if self.user_agent.contains("Windows NT 10.0") {
            "10"
        } else {
            ""
        }
    }

    fn properties(&self) -> Properties<'_> {
        Properties {
            os: self.os(),
            browser: self.browser(),
            device: "",
            device_model_name: "",
            system_locale: &self.language,
            has_client_mods: false,
            browser_user_agent: &self.user_agent,
            browser_version: self.browser_version(),
            os_version: self.os_version(),
            referrer: "",
            referring_domain: "",
            referrer_current: "",
            referring_domain_current: "",
            release_channel: "stable",
            client_build_number: BUILD.load(Ordering::Relaxed),
            client_event_source: None,
            client_launch_id: &LAUNCH[0],
            launch_signature: None,
            client_heartbeat_session_id: &LAUNCH[1],
            client_app_state: "focused",
        }
    }

    /// `X-Super-Properties`: the properties as base64 JSON.
    fn super_properties(&self) -> String {
        let json = serde_json::to_vec(&self.properties()).unwrap_or_default();
        base64::engine::general_purpose::STANDARD.encode(json)
    }

    /// What the web client and its browser add to a request to Discord's
    /// API. A browser names its `Origin` on every request but a read, and a
    /// Chromium one its low-entropy client hints on every request.
    fn api_headers(&self, method: &Method) -> HeaderMap {
        let language = self.language.split('-').next().unwrap_or("en");
        let accepted = if self.language == language {
            self.language.clone()
        } else {
            format!("{},{language};q=0.9", self.language)
        };
        let mut headers = HeaderMap::new();
        let mut put = |name: HeaderName, value: &str| {
            if let Ok(value) = HeaderValue::from_str(value) {
                headers.insert(name, value);
            }
        };
        put(USER_AGENT, &self.user_agent);
        put(
            HeaderName::from_static("x-super-properties"),
            self.super_properties().as_str(),
        );
        put(
            HeaderName::from_static("x-discord-locale"),
            discord_locale(&self.language),
        );
        put(
            HeaderName::from_static("x-discord-timezone"),
            &self.timezone,
        );
        put(ACCEPT_LANGUAGE, accepted.as_str());
        put(REFERER, "https://discord.com/channels/@me");
        if method != Method::GET {
            put(ORIGIN, "https://discord.com");
        }
        if !self.brands.is_empty() {
            let brands: Vec<_> = self
                .brands
                .iter()
                .map(|brand| format!("\"{}\";v=\"{}\"", brand.brand, brand.version))
                .collect();
            put(HeaderName::from_static("sec-ch-ua"), &brands.join(", "));
            put(HeaderName::from_static("sec-ch-ua-mobile"), "?0");
            put(
                HeaderName::from_static("sec-ch-ua-platform"),
                &format!("\"{}\"", self.platform),
            );
        }
        headers
    }
}

/// What a WebSocket opened by the web client's page says of itself.
pub(crate) fn socket_headers(request: &mut tokio_tungstenite::tungstenite::http::Request<()>) {
    use tokio_tungstenite::tungstenite::http::HeaderValue;
    let headers = request.headers_mut();
    if let Ok(agent) = HeaderValue::from_str(user_agent()) {
        headers.insert("user-agent", agent);
    }
    headers.insert("origin", HeaderValue::from_static("https://discord.com"));
}

/// Asserts that `request`, as it left for Discord's API, presents this
/// identity and carries `secret` in its `Authorization` alone.
#[cfg(test)]
pub(crate) fn assert_presented(request: &str, secret: &str) {
    let head = request.split("\r\n\r\n").next().unwrap();
    let lower = head.to_ascii_lowercase();
    let agent = format!("\r\nuser-agent: {}\r\n", user_agent().to_ascii_lowercase());
    assert!(lower.contains(&agent), "{head}");
    let encoded = head
        .lines()
        .find_map(|line| line.strip_prefix("x-super-properties: "))
        .expect("the web client's super properties");
    let properties: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
    )
    .unwrap();
    let engine = engine();
    assert_eq!(properties["browser_user_agent"], user_agent());
    assert_eq!(properties["browser"], engine.browser());
    assert_eq!(properties["os"], engine.os());
    assert_eq!(properties["system_locale"], engine.language);
    assert!(properties["client_build_number"].as_u64() > Some(0));
    for name in ["x-discord-locale: ", "x-discord-timezone: ", "referer: "] {
        assert!(lower.contains(&format!("\r\n{name}")), "{name}");
    }
    // A Chromium engine's client hints come along; WebKit has none.
    assert_eq!(
        lower.contains("\r\nsec-ch-ua: "),
        !engine.brands.is_empty(),
        "{head}"
    );
    for absent in ["x-fingerprint", "cookie:", "rune/"] {
        assert!(!lower.contains(absent), "{absent}");
    }
    let carrying: Vec<_> = request
        .lines()
        .filter(|line| line.contains(secret))
        .collect();
    assert!(
        carrying.len() <= 1
            && carrying
                .iter()
                .all(|line| line.to_ascii_lowercase().starts_with("authorization: ")),
        "{carrying:?}"
    );
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A WebView2 engine as RUNE's probe page reads it (synthetic values in
    /// the shape Chromium gives them).
    pub(crate) fn webview2(brands: &[&str]) -> Engine {
        Engine {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0".into(),
            vendor: "Google Inc.".into(),
            brands: brands
                .iter()
                .map(|brand| Brand {
                    brand: (*brand).into(),
                    version: if brand.starts_with("Not") { "24" } else { "141" }.into(),
                })
                .collect(),
            platform: "Windows".into(),
            language: "es-AR".into(),
            timezone: "America/Argentina/Buenos_Aires".into(),
        }
    }

    fn decoded(engine: &Engine) -> serde_json::Value {
        serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(engine.super_properties())
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_webview2_engine_is_named_as_the_web_client_names_it() {
        let edge = webview2(&[
            "Microsoft Edge WebView2",
            "Not?A_Brand",
            "Microsoft Edge",
            "Chromium",
        ]);
        assert!(edge.valid());
        let value = decoded(&edge);
        let text = serde_json::to_string(&value).unwrap();
        assert!(text.contains(r#""os":"Windows""#), "{text}");
        assert_eq!(value["browser"], "Edge");
        assert_eq!(value["browser_version"], "141.0.0.0");
        assert_eq!(value["os_version"], "10");
        assert_eq!(value["system_locale"], "es-AR");
        assert_eq!(value["browser_user_agent"], edge.user_agent);
        // Without Edge's own brand the web client falls to the user agent,
        // whose `Edg/` is not `Edge`: Chrome, with the same version.
        for brands in [
            &["Microsoft Edge WebView2", "Chromium", "Not?A_Brand"][..],
            &[],
        ] {
            let other = decoded(&webview2(brands));
            assert_eq!(other["browser"], "Chrome", "{brands:?}");
            assert_eq!(other["browser_version"], "141.0.0.0");
        }
        let headers = edge.api_headers(&Method::POST);
        assert_eq!(headers["user-agent"], edge.user_agent.as_str());
        assert_eq!(headers["x-discord-locale"], "es-419");
        assert_eq!(
            headers["x-discord-timezone"],
            "America/Argentina/Buenos_Aires"
        );
        assert_eq!(headers["accept-language"], "es-AR,es;q=0.9");
        assert_eq!(
            headers["sec-ch-ua"],
            r#""Microsoft Edge WebView2";v="141", "Not?A_Brand";v="24", "Microsoft Edge";v="141", "Chromium";v="141""#
        );
        assert_eq!(headers["sec-ch-ua-mobile"], "?0");
        assert_eq!(headers["sec-ch-ua-platform"], r#""Windows""#);
        assert!(!webview2(&[])
            .api_headers(&Method::GET)
            .contains_key("sec-ch-ua"));
    }

    #[test]
    fn only_a_browsers_values_pass_for_an_engine() {
        // As the Windows probe sends them.
        let json = r#"{"user_agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0","vendor":"Google Inc.","brands":[{"brand":"Chromium","version":"141"}],"platform":"Windows","language":"en-US","timezone":"Europe/Madrid"}"#;
        let engine: Engine = serde_json::from_str(json).unwrap();
        assert!(engine.valid());
        assert_eq!(engine.brands[0].brand, "Chromium");
        let broken = |change: fn(&mut Engine)| {
            let mut engine = engine.clone();
            change(&mut engine);
            !engine.valid()
        };
        assert!(broken(|e| e.user_agent = "RUNE/1.0".into()));
        assert!(broken(|e| e.user_agent.push_str("\r\nx-injected: 1")));
        assert!(broken(|e| e.user_agent.push_str(&"x".repeat(512))));
        assert!(broken(|e| e.language = "es AR".into()));
        assert!(broken(|e| e.language.clear()));
        assert!(broken(|e| e.timezone = "../etc".into()));
        assert!(broken(|e| e.brands[0].brand = "A\"; v=\"1".into()));
        assert!(broken(|e| e.brands[0].version = "1,2".into()));
        assert!(broken(|e| e.platform = "Win\"dows".into()));
        assert!(broken(|e| e.brands = vec![e.brands[0].clone(); 9]));
    }

    struct Told(Option<Engine>);

    impl SessionLogin for Told {
        fn acquire(
            &self,
        ) -> rune_provider::BoxFuture<'_, Result<crate::SessionSecret, LoginError>> {
            Box::pin(async { Err(LoginError::new(es("Se canceló el inicio de sesión."))) })
        }

        fn engine(&self) -> rune_provider::BoxFuture<'_, Result<Option<Engine>, LoginError>> {
            let engine = self.0.clone();
            Box::pin(async move { Ok(engine) })
        }
    }

    #[tokio::test]
    async fn an_engine_that_is_not_a_browsers_is_never_presented() {
        let before = user_agent();
        let mut wrong = webview2(&[]);
        wrong.user_agent = "curl/8".into();
        assert!(learn(&Told(Some(wrong))).await.is_err());
        assert!(learn(&Told(None)).await.is_ok());
        assert_eq!(user_agent(), before);
    }

    #[test]
    fn the_identity_is_one_webkit_browser_on_linux() {
        let agent = user_agent();
        assert!(agent.starts_with("Mozilla/5.0 (X11; Linux "));
        assert!(agent.ends_with(&format!(
            "AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{VERSION} Safari/605.1.15"
        )));
        assert!(!agent.contains("Chrome") && !agent.contains("RUNE"));
        let encoded = super_properties();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let text = String::from_utf8(decoded).unwrap();
        // The web client's order, which a map would sort.
        assert!(text.starts_with(r#"{"os":"Linux","browser":"Safari","device":"","#));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["browser_user_agent"], agent);
        assert_eq!(value["browser_version"], VERSION);
        assert_eq!(value["release_channel"], "stable");
        assert!(value["client_build_number"].as_u64().unwrap() > 0);
        assert!(value["client_event_source"].is_null());
        assert!(value["launch_signature"].is_null());
        assert_eq!(value["has_client_mods"], false);
        let launch = value["client_launch_id"].as_str().unwrap();
        assert_eq!(launch.len(), 36);
        assert_eq!(&launch[14..15], "4");
        assert_ne!(launch, value["client_heartbeat_session_id"]);
        // The same launch on every request.
        let again: serde_json::Value = serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(super_properties())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(again["client_launch_id"], launch);
        let identify = serde_json::to_value(identify()).unwrap();
        assert_eq!(identify["browser_user_agent"], agent);
        assert_eq!(identify["client_launch_id"], launch);
        assert_eq!(identify["is_fast_connect"], false);
        assert_eq!(identify["gateway_connect_reasons"], "COLD_START");
    }

    #[test]
    fn reads_and_writes_carry_the_web_clients_headers() {
        let read = api_headers(&Method::GET);
        let write = api_headers(&Method::POST);
        for headers in [&read, &write] {
            assert_eq!(headers["user-agent"], user_agent());
            assert!(!headers.contains_key("sec-ch-ua"));
            for name in [
                "x-super-properties",
                "x-discord-locale",
                "x-discord-timezone",
                "accept-language",
                "referer",
            ] {
                assert!(headers.contains_key(name), "{name}");
            }
            assert!(!headers.contains_key("x-fingerprint"));
            assert!(!headers.contains_key("authorization"));
        }
        assert!(!read.contains_key("origin"));
        assert_eq!(write["origin"], "https://discord.com");
        assert!(LOCALES.contains(&read["x-discord-locale"].to_str().unwrap()));
    }

    #[test]
    fn the_page_names_its_build_and_nothing_else_passes_for_one() {
        let page = r#"<script>window.GLOBAL_ENV = {"NODE_ENV":"production","BUILT_AT":"1792189653798","BUILD_NUMBER":"641004","RELEASE_CHANNEL":"stable"}</script>"#;
        assert_eq!(build_in(page), Some(641_004));
        for page in [
            "",
            r#""BUILD_NUMBER":"""#,
            r#""BUILD_NUMBER":"0""#,
            r#""BUILD_NUMBER":"64a004""#,
            r#""BUILD_NUMBER":"1234567890""#,
            r#""BUILD_NUMBER":641004"#,
        ] {
            assert_eq!(build_in(page), None, "{page}");
        }
    }

    #[test]
    fn languages_map_as_the_web_client_maps_them() {
        for (tag, locale) in [
            ("en-US", "en-US"),
            ("es-ES", "es-ES"),
            ("es-AR", "es-419"),
            ("es", "es-ES"),
            ("pt-PT", "pt-BR"),
            ("de-AT", "de"),
            ("nb", "no"),
            ("zh-Hant-TW", "zh-TW"),
            ("C", "en-US"),
            ("xx-YY", "en-US"),
        ] {
            assert_eq!(discord_locale(tag), locale, "{tag}");
        }
    }
}
