//! Requests to Discord's API as the web client sends them, the build its
//! public page names, and where an accepted invitation was seen
//! (owner decision of 2026-10-19; `crate::browser`).

use super::*;
use reqwest::header::USER_AGENT;
use rune_provider::InvitationTarget;

/// The web client's page is ~76 KiB; anything far larger is not it.
const PAGE: usize = 1024 * 1024;

impl Http {
    /// A request to Discord's API with the web client's headers.
    pub(super) fn api(&self, method: Method, url: String) -> reqwest::RequestBuilder {
        let headers = crate::browser::api_headers(&method);
        self.client.request(method, url).headers(headers)
    }

    /// The build the web client's public page names now, read as a browser
    /// loads that page: without the account's credential or API headers.
    pub(crate) async fn current_build(&self) -> Option<u64> {
        let origin = self.base.strip_suffix("/api/v10")?;
        let response = self
            .client
            .get(format!("{origin}/app"))
            .header(USER_AGENT, crate::browser::user_agent())
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        if response.status() != StatusCode::OK {
            return None;
        }
        let page = read_bounded(response, PAGE).await.ok()?;
        crate::browser::build_in(&String::from_utf8_lossy(&page))
    }

    /// Keeps where `invitation` leads as the web client's invite modal
    /// describes it (REA, build 641004), for the acceptance that follows.
    pub(super) fn remember_invitation(&self, invitation: &InvitationTarget, data: &Value) {
        #[derive(serde::Serialize)]
        struct Context<'a> {
            location: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            location_guild_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            location_channel_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            location_channel_type: Option<u64>,
            // Scheduled events (eventos-programados): the event the link
            // names, as the web client's acceptance adds it (REA).
            #[serde(skip_serializing_if = "Option::is_none")]
            invite_guild_scheduled_event_id: Option<&'a str>,
        }
        fn id(value: &Value) -> Option<&str> {
            value
                .as_str()
                .filter(|id| (1..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
        }
        let context = Context {
            location: "Desktop Invite Modal",
            location_guild_id: id(&data["guild"]["id"]),
            location_channel_id: id(&data["channel"]["id"]),
            location_channel_type: data["channel"]["type"].as_u64(),
            invite_guild_scheduled_event_id: invitation
                .url
                .split_once("?event=")
                .map(|(_, event)| event)
                .filter(|event| {
                    (1..=20).contains(&event.len()) && event.bytes().all(|b| b.is_ascii_digit())
                }),
        };
        use base64::Engine as _;
        let header = serde_json::to_vec(&context)
            .ok()
            .map(|json| base64::engine::general_purpose::STANDARD.encode(json))
            .and_then(|encoded| HeaderValue::from_str(&encoded).ok());
        if let Ok(mut kept) = self.invitation.lock() {
            *kept = header.map(|header| (invitation.key.clone(), header));
        }
    }

    /// The `X-Context-Properties` of accepting `invitation`, if it was the
    /// one inspected last.
    pub(super) fn invitation_context(&self, invitation: &InvitationTarget) -> Option<HeaderValue> {
        let kept = self.invitation.lock().ok()?;
        kept.as_ref()
            .filter(|(key, _)| *key == invitation.key)
            .map(|(_, header)| header.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves `reply` once on loopback and hands back what was asked.
    async fn serve(reply: String) -> (Http, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = vec![0u8; 8192];
            let read = socket.read(&mut head).await.unwrap();
            socket.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&head[..read]).into_owned()
        });
        (Http::local(&base), asked)
    }

    #[tokio::test]
    async fn the_build_comes_from_the_public_page_without_the_account() {
        let page = r#"<script>window.GLOBAL_ENV = {"BUILD_NUMBER":"654321","RELEASE_CHANNEL":"stable"}</script>"#;
        let (http, asked) = serve(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{page}",
            page.len()
        ))
        .await;
        assert_eq!(http.current_build().await, Some(654_321));
        let asked = asked.await.unwrap().to_ascii_lowercase();
        assert!(asked.starts_with("get /app http/1.1\r\n"), "{asked}");
        assert!(asked.contains(&format!(
            "user-agent: {}\r\n",
            crate::browser::user_agent().to_ascii_lowercase()
        )));
        for absent in ["authorization", "x-super-properties", "cookie"] {
            assert!(!asked.contains(absent), "{absent}");
        }
        let (http, _) =
            serve("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".into()).await;
        assert_eq!(http.current_build().await, None);
    }

    #[test]
    fn an_acceptance_says_where_its_inspected_invitation_leads() {
        use base64::Engine as _;
        let http = Http::local("http://127.0.0.1:9");
        let invitation = crate::invitations::target("https://discord.gg/synthetic").unwrap();
        let other = crate::invitations::target("https://discord.gg/another").unwrap();
        assert!(http.invitation_context(&invitation).is_none());
        http.remember_invitation(
            &invitation,
            &serde_json::json!({"code":"synthetic","guild":{"id":"10"},"channel":{"id":"11","type":0}}),
        );
        let header = http.invitation_context(&invitation).unwrap();
        let context = base64::engine::general_purpose::STANDARD
            .decode(header.as_bytes())
            .unwrap();
        assert_eq!(
            String::from_utf8(context).unwrap(),
            r#"{"location":"Desktop Invite Modal","location_guild_id":"10","location_channel_id":"11","location_channel_type":0}"#
        );
        assert!(http.invitation_context(&other).is_none());
        // A group's invitation names no server; odd ids are left out.
        http.remember_invitation(
            &other,
            &serde_json::json!({"code":"another","channel":{"id":"x\r\n","type":3}}),
        );
        let context = base64::engine::general_purpose::STANDARD
            .decode(http.invitation_context(&other).unwrap().as_bytes())
            .unwrap();
        assert_eq!(
            String::from_utf8(context).unwrap(),
            r#"{"location":"Desktop Invite Modal","location_channel_type":3}"#
        );
        assert!(http.invitation_context(&invitation).is_none());
        // Scheduled events (eventos-programados): the event a link names.
        let event = crate::invitations::target("https://discord.gg/synthetic?event=90").unwrap();
        http.remember_invitation(
            &event,
            &serde_json::json!({"code":"synthetic","guild":{"id":"10"}}),
        );
        let context = base64::engine::general_purpose::STANDARD
            .decode(http.invitation_context(&event).unwrap().as_bytes())
            .unwrap();
        assert_eq!(
            String::from_utf8(context).unwrap(),
            r#"{"location":"Desktop Invite Modal","location_guild_id":"10","invite_guild_scheduled_event_id":"90"}"#
        );
    }
}
