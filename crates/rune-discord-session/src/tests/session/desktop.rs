//! What this device says the person is doing (P6), through the whole
//! provider, against local fixtures only. None of this reached Discord.

use super::*;
use rune_model::{ActivitySource, LocalActivity, LocalActivityKind};

async fn presence_update(app: &mut Harness) -> Value {
    tokio::time::timeout(Duration::from_secs(2), app.presences.recv())
        .await
        .expect("a presence update")
        .unwrap()["d"]
        .clone()
}

fn program(large: &str, small: &str) -> LocalActivity {
    LocalActivity {
        application: Some(777),
        kind: LocalActivityKind::Playing,
        name: None,
        details: Some("Nivel 3".into()),
        state: None,
        start: Some(1_700_000_000_000),
        end: None,
        large_image: Some(large.into()),
        large_text: Some("Mapa".into()),
        small_image: Some(small.into()),
        small_text: Some("Clase".into()),
        source: ActivitySource::Program,
    }
}

#[tokio::test]
async fn a_game_goes_with_the_presence_and_a_program_is_named_and_its_pictures_looked_up() {
    let mut app = Harness::new().await;
    // Read with none set: the first presence, with nothing of this device.
    assert_eq!(presence_update(&mut app).await["activities"], json!([]));
    // A game recognized by its process needs nothing looked up.
    let game = LocalActivity {
        application: Some(1),
        name: Some("Juego".into()),
        start: Some(5),
        source: ActivitySource::Game,
        ..LocalActivity::default()
    };
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(game)))
        .await
        .unwrap();
    assert_eq!(
        presence_update(&mut app).await["activities"],
        json!([{"name": "Juego", "type": 0, "application_id": "1", "timestamps": {"start": 5}}])
    );
    // A program: its name, its keys and its address, one request each.
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(program(
            "cover",
            "https://example.com/a.png",
        ))))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/applications/777/rpc "));
    reply
        .send(response(
            200,
            &json!({"id": "777", "name": "Programa"}).to_string(),
        ))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/oauth2/applications/777/assets "));
    reply
        .send(response(
            200,
            &json!([{"id": "4242", "name": "cover", "type": 1}]).to_string(),
        ))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/applications/777/external-assets "));
    let sent: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(sent, json!({"urls": ["https://example.com/a.png"]}));
    reply
        .send(response(
            200,
            &json!([{"url": "https://example.com/a.png", "external_asset_path": "external/x/a.png"}])
                .to_string(),
        ))
        .unwrap();
    assert_eq!(
        presence_update(&mut app).await["activities"],
        json!([{
            "name": "Programa", "type": 0, "application_id": "777", "details": "Nivel 3",
            "timestamps": {"start": 1_700_000_000_000_i64},
            "assets": {"large_image": "4242", "large_text": "Mapa",
                "small_image": "mp:external/x/a.png", "small_text": "Clase"},
        }])
    );
    // Asked again, the same program needs no lookup: all of it is kept.
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(program(
            "cover",
            "https://example.com/a.png",
        ))))
        .await
        .unwrap();
    let mut changed = program("cover", "https://example.com/a.png");
    changed.details = Some("Nivel 4".into());
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(changed)))
        .await
        .unwrap();
    assert_eq!(
        presence_update(&mut app).await["activities"][0]["details"],
        "Nivel 4"
    );
    // Taken away, it leaves the presence; the custom status would stay.
    app.running
        .commands
        .send(Command::SetLocalActivity(None))
        .await
        .unwrap();
    assert_eq!(presence_update(&mut app).await["activities"], json!([]));
}

#[tokio::test]
async fn an_application_discord_does_not_name_is_not_published() {
    let mut app = Harness::new().await;
    presence_update(&mut app).await;
    let mut unnamed = program("mp:external/y", "mp:external/z");
    unnamed.application = Some(888);
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(unnamed)))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/applications/888/rpc "));
    reply
        .send(response(
            200,
            &json!({"id": "1", "name": "Otro"}).to_string(),
        ))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), app.presences.recv())
            .await
            .is_err(),
        "nothing goes out"
    );
}

#[tokio::test]
async fn the_list_of_games_is_read_without_the_credential() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadDetectableGames)
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/applications/detectable "));
    assert!(
        !request.to_lowercase().contains("authorization:"),
        "a public list"
    );
    reply
        .send(response(
            200,
            &json!([{"id": "9", "name": "Juego", "executables": [{"name": "juego.exe", "os": "win32"}]}])
                .to_string(),
        ))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::DetectableGames(vec![rune_model::DetectableGame {
            application: Some(9),
            name: "Juego".into(),
            executables: vec!["juego.exe".into()],
        }])
    );
}
