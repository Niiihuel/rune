//! Presence, identity and expressions (P5) over REST and the Gateway,
//! against local fixtures only. Nothing here reached Discord.

use super::message_recovery::guild_channel;
use super::*;
use rune_model::StickerId;
use rune_provider::{Failure, GifError, GifQuery};

const VIEW_AND_SEND: u64 = (1 << 10) | (1 << 11);

fn search(request: u64, query: GifQuery) -> Command {
    Command::SearchGifs {
        request,
        query,
        offset: 0,
    }
}

fn gif(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "title": name, "url": format!("https://klipy.com/gifs/{name}"),
        "src": format!("https://static.klipy.com/{name}.gif"), "width": 220, "height": 180})
}

#[tokio::test]
async fn standard_packs_are_read_and_their_stickers_are_sent_anywhere() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW_AND_SEND).await;
    // Without the packs read, a sticker of none of the account's servers is refused here.
    let stray = Command::SendSticker {
        channel: ChannelId(30),
        sticker: StickerId(77),
        nonce: 4,
    };
    app.running.commands.send(stray.clone()).await.unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Send { nonce: 4, .. },
            ..
        }
    ));
    app.running
        .commands
        .send(Command::LoadStickerPacks)
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/sticker-packs "));
    reply
        .send(response(
            200,
            &json!({"sticker_packs": [{"id": "1", "name": "Wumpus",
                "stickers": [{"id": "77", "name": "Hola", "format_type": 3},
                             {"id": "78", "name": "Chau", "format_type": 1}]}]})
            .to_string(),
        ))
        .unwrap();
    let Event::StickerPacks(packs) = app.next_data().await else {
        panic!("packs");
    };
    assert_eq!(packs[0].stickers.len(), 2);
    app.running.commands.send(stray).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/30/messages "));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["sticker_ids"], json!(["77"]));
    let mut sent = rest::message(101, "");
    sent["channel_id"] = json!("30");
    sent["sticker_items"] = json!([{"id": "77", "name": "Hola", "format_type": 3}]);
    reply.send(response(200, &sent.to_string())).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MessageCreated { nonce: Some(4), .. }
    ));
}

#[tokio::test]
async fn gifs_search_trends_with_categories_suggest_and_a_newer_search_wins() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(search(1, GifQuery::Trending))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(
        "GET /api/v10/gifs/trending-gifs?media_format=tinygif&locale=es-ES&limit=50 "
    ));
    reply
        .send(response(200, &json!([gif(1, "hola")]).to_string()))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/gifs/trending?media_format=tinygif&locale=es-ES "));
    reply
        .send(response(
            200,
            &json!({"categories": [{"name": "aplausos", "src": "https://static.klipy.com/a.gif"}],
                "gifs": [gif(1, "hola")]})
            .to_string(),
        ))
        .unwrap();
    let Event::GifResults(found) = app.next_data().await else {
        panic!("trending");
    };
    let page = found.result.unwrap();
    assert_eq!((found.request, page.items.len()), (1, 1));
    assert_eq!(page.categories[0].name, "aplausos");
    assert_eq!(page.items[0].url, "https://klipy.com/gifs/hola");

    // A newer search replaces the one in flight: only it answers.
    app.running
        .commands
        .send(search(2, GifQuery::Search("gato".into())))
        .await
        .unwrap();
    let (_, stale) = app.pending().await;
    app.running
        .commands
        .send(search(3, GifQuery::Search("perro feliz".into())))
        .await
        .unwrap();
    let _ = stale.send(response(200, &json!([gif(2, "gato")]).to_string()));
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(
        "GET /api/v10/gifs/search?q=perro+feliz&media_format=tinygif&locale=es-ES&limit=50 "
    ));
    reply
        .send(response(200, &json!([gif(3, "perro")]).to_string()))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::GifResults(found) if found.request == 3 && found.result.is_ok()
    ));

    // A failed search says so in its own answer.
    app.running
        .commands
        .send(search(4, GifQuery::Search("x".into())))
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply.send(response(500, "{}")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::GifResults(found) if found.request == 4 && found.result == Err(GifError::Unavailable)
    ));

    app.running
        .commands
        .send(Command::SuggestGifs {
            request: 5,
            query: "ga".into(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/gifs/suggest?q=ga&limit=5&locale=es-ES "));
    reply
        .send(response(200, &json!(["gato", "gatos"]).to_string()))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::GifSuggestions {
            request: 5,
            query: "ga".into(),
            terms: vec!["gato".into(), "gatos".into()],
        }
    );
}

#[tokio::test]
async fn a_new_plan_is_told_as_perks() {
    let mut app = Harness::new().await;
    app.dispatch
        .send((
            "USER_UPDATE",
            json!({"id": "42", "username": "rune-test", "premium_type": 2,
                "primary_guild": {"identity_guild_id": "10", "identity_enabled": true, "tag": "RUNE"}}),
        ))
        .await
        .unwrap();
    let Event::UserChanged(me) = app.running.next().await else {
        panic!("the account first");
    };
    assert_eq!(me.identity.tag.unwrap().tag, "RUNE");
    assert!(matches!(
        app.running.next().await,
        Event::Perks(perks) if perks.stickers_anywhere && perks.upgrade.as_deref() == Some("Nitro")
    ));
}

#[tokio::test]
async fn a_profile_reads_the_effect_it_wears_from_its_product() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadProfile {
            request: 3,
            user: rune_model::UserId(43),
            server: None,
            effect: true,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/43/profile?"));
    reply
        .send(response(
            200,
            &json!({"user": {"id": "43", "username": "peer"},
            "user_profile": {"collectibles": [
                {"sku_id": "77", "type": 3},
                {"sku_id": "99", "type": 1, "expires_at": null}
            ]}})
            .to_string(),
        ))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/collectibles-products/99 "));
    let asset = "https://cdn.discordapp.com/assets/content/0123abcd.png";
    reply
        .send(response(
            200,
            &json!({"items": [{"type": 1, "sku_id": "99",
                "effects": [
                    {"src": asset, "zIndex": 2, "position": {"x": 10, "y": 20}, "width": 200, "height": 300,
                     "start": 500, "duration": 2000, "loop": false, "loopDelay": 0},
                    {"src": "https://elsewhere.test/x.png", "zIndex": 1}
                ],
                "staticFrameSrc": asset,
                "reducedMotionSrc": "https://cdn.discordapp.com/assets/content/quieto.png"}]})
            .to_string(),
        ))
        .unwrap();
    let profile = loop {
        if let Event::ProfileLoaded {
            request: 3,
            profile,
        } = app.running.next().await
        {
            break profile;
        }
    };
    let effect = profile.effect.expect("its effect");
    assert_eq!(effect.layers.len(), 1, "only Discord's own pictures");
    assert_eq!((effect.layers[0].x, effect.layers[0].y), (10, 20));
    // Its time, as the official client plays it: an intro of two seconds.
    let layer = &effect.layers[0];
    assert_eq!(
        (layer.start, layer.duration, layer.looping),
        (500, 2000, false)
    );
    assert!(layer.shows(600) && !layer.shows(2_600));
    // Reduced motion shows the still the official client shows.
    assert!(effect.still.unwrap().url.ends_with("/quieto.png"));
}

#[tokio::test]
async fn hidden_effects_skip_the_read_of_their_product() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadProfile {
            request: 4,
            user: rune_model::UserId(43),
            server: None,
            effect: false,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/43/profile?"));
    reply
        .send(response(
            200,
            &json!({"user": {"id": "43", "username": "peer"},
            "user_profile": {"collectibles": [{"sku_id": "99", "type": 1}]}})
            .to_string(),
        ))
        .unwrap();
    // The profile comes at once, without asking for its effect's product.
    let profile = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if let Event::ProfileLoaded {
                request: 4,
                profile,
            } = app.running.next().await
            {
                break profile;
            }
        }
    })
    .await
    .expect("no read of the product held it back");
    assert_eq!(profile.effect, None);
}

#[tokio::test]
async fn an_activity_without_pictures_is_told_again_with_its_applications_icon() {
    let mut app = Harness::new().await;
    let playing = json!({"user": {"id": "43"}, "status": "online",
        "activities": [{"type": 0, "name": "Juego", "application_id": "7"}]});
    app.dispatch
        .send(("PRESENCE_UPDATE", playing.clone()))
        .await
        .unwrap();
    let Event::PresenceChanged { user, presence } = app.next_data().await else {
        panic!("presence");
    };
    assert_eq!(
        (user, presence.activities[0].image.clone()),
        (rune_model::UserId(43), None)
    );
    // Its application is read once, as the official client reads it.
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/applications/7/public "));
    let hash = "0123456789abcdef0123456789abcdef";
    reply
        .send(response(200, &json!({"id": "7", "icon": hash}).to_string()))
        .unwrap();
    let Event::PresenceChanged { presence, .. } = app.next_data().await else {
        panic!("presence with its icon");
    };
    let icon = format!("https://cdn.discordapp.com/app-icons/7/{hash}.png?size=128");
    assert_eq!(presence.activities[0].image.as_ref().unwrap().url, icon);
    // Known now: the next one comes with it, without another read.
    app.dispatch
        .send(("PRESENCE_UPDATE", playing))
        .await
        .unwrap();
    let Event::PresenceChanged { presence, .. } = app.next_data().await else {
        panic!("presence");
    };
    assert_eq!(presence.activities[0].image.as_ref().unwrap().url, icon);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), app.requests.recv())
            .await
            .is_err(),
        "no second read"
    );
}

#[tokio::test]
async fn an_activity_button_gives_the_link_its_app_answers_and_no_other() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW_AND_SEND).await;
    // The second button's link, read from the activity's app at its place.
    app.running
        .commands
        .send(Command::OpenActivityButton {
            request: 7,
            user: rune_model::UserId(11),
            session: "abc_123".into(),
            application: Some(42),
            index: 1,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/11/sessions/abc_123/activities/42/metadata "));
    reply
        .send(response(
            200,
            &json!({"button_urls": ["javascript:alert(1)", "https://tracker.example/match/9"]})
                .to_string(),
        ))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ActivityButtonUrl { request: 7, url } if url == "https://tracker.example/match/9"
    ));
    // A link that is not one a person may open is refused, with nothing else asked.
    app.running
        .commands
        .send(Command::OpenActivityButton {
            request: 8,
            user: rune_model::UserId(11),
            session: "abc_123".into(),
            application: None,
            index: 0,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/11/sessions/abc_123/activities/0/metadata "));
    reply
        .send(response(
            200,
            &json!({"button_urls": ["javascript:alert(1)"]}).to_string(),
        ))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ActivityButton { request: 8 },
            ..
        }
    ));
    // A session id with other characters never reaches a path.
    app.running
        .commands
        .send(Command::OpenActivityButton {
            request: 9,
            user: rune_model::UserId(11),
            session: "../me".into(),
            application: None,
            index: 0,
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ActivityButton { request: 9 },
            ..
        }
    ));
}
