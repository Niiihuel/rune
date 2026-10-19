//! A stage's requests over loopback fixtures: the route and body the
//! official web client sends, and nothing reported before the Gateway.

use super::*;
use rune_model::{ChannelId, ServerId};
use serde_json::{json, Value};

#[tokio::test]
async fn hands_invitations_and_stepping_down_use_the_voice_state_routes() {
    let me = UserId(9);
    for (command, route, check) in [
        (
            Command::RaiseHand {
                channel: ChannelId(30),
                raised: true,
            },
            "PATCH /api/v10/guilds/10/voice-states/@me",
            (|body: &Value| {
                body["request_to_speak_timestamp"].is_string() && body.get("suppress").is_none()
            }) as fn(&Value) -> bool,
        ),
        (
            Command::RaiseHand {
                channel: ChannelId(30),
                raised: false,
            },
            "PATCH /api/v10/guilds/10/voice-states/@me",
            |body: &Value| body["request_to_speak_timestamp"].is_null(),
        ),
        (
            Command::SetSpeaker {
                channel: ChannelId(30),
                user: me,
                speaker: true,
            },
            "PATCH /api/v10/guilds/10/voice-states/@me",
            |body: &Value| {
                body["suppress"] == json!(false) && body["request_to_speak_timestamp"].is_null()
            },
        ),
        (
            Command::SetSpeaker {
                channel: ChannelId(30),
                user: UserId(8),
                speaker: true,
            },
            "PATCH /api/v10/guilds/10/voice-states/8",
            |body: &Value| {
                body["suppress"] == json!(false) && body["request_to_speak_timestamp"].is_string()
            },
        ),
        (
            Command::SetSpeaker {
                channel: ChannelId(30),
                user: UserId(8),
                speaker: false,
            },
            "PATCH /api/v10/guilds/10/voice-states/8",
            |body: &Value| {
                body["suppress"] == json!(true) && body.get("request_to_speak_timestamp").is_none()
            },
        ),
    ] {
        let (http, captured) =
            fixture("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into()).await;
        let result = http.stage(&secret(), ServerId(10), me, &command).await;
        // The Gateway's VOICE_STATE_UPDATE confirms it: nothing from here.
        assert!(matches!(result, Ok(ref events) if events.is_empty()));
        let request = captured.await.unwrap();
        assert!(
            request.starts_with(&format!("{route} HTTP/1.1")),
            "{request}"
        );
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["channel_id"], json!("30"));
        assert!(check(&body), "{body}");
    }
}
