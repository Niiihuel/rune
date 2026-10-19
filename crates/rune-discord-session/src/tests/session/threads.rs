//! Threads through the whole provider, against the local Gateway and HTTP
//! fixtures (2026-10-18). Nothing here reached Discord.

use super::*;
use rune_provider::{ThreadEdit, ThreadScope};

impl Harness {
    /// Data events until one matches, skipping the rest.
    async fn th_until(&mut self, wanted: impl Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.next_data().await;
            if wanted(&event) {
                return event;
            }
        }
    }
}

fn archived_thread(archived: bool) -> Value {
    json!({"id":"50","guild_id":"10","type":11,"name":"viejo","parent_id":"30",
        "owner_id":"42","thread_metadata":{"archived":archived,"locked":false,
            "archive_timestamp":"2026-09-09T08:00:00+00:00"}})
}

#[tokio::test]
async fn an_archived_thread_read_by_page_outlives_a_new_ready_of_the_same_account() {
    let mut app = Harness::new().await;
    let guild = json!({"id":"10","name":"Taller","owner_id":"42",
        "roles":[{"id":"10","name":"@everyone","permissions":"0","position":0}],
        "members":[{"user":{"id":"42","username":"Owner"},"roles":[]}],
        "channels":[{"id":"30","type":0,"name":"general","permission_overwrites":[]}]});
    app.dispatch
        .send(("GUILD_CREATE", guild.clone()))
        .await
        .unwrap();
    app.th_until(|e| matches!(e, Event::ChannelChanged(c) if c.id == ChannelId(30)))
        .await;
    app.running
        .commands
        .send(Command::LoadThreads {
            request: 1,
            parent: ChannelId(30),
            scope: ThreadScope::Archived,
            before: None,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/30/threads/archived/public?limit=25 "));
    let page = json!({"threads":[archived_thread(true)],"members":[],"has_more":false});
    reply.send(response(200, &page.to_string())).unwrap();
    app.th_until(|e| matches!(e, Event::ThreadsLoaded { request: 1, .. }))
        .await;

    // A new READY names the server but not the archived thread.
    app.dispatch
        .send((
            "READY",
            json!({"user":{"id":"42"},"session_id":"again",
                "resume_gateway_url":"wss://gateway.discord.gg",
                "guilds":[guild],"private_channels":[]}),
        ))
        .await
        .unwrap();
    let Event::Ready(snapshot) = app.th_until(|e| matches!(e, Event::Ready(_))).await else {
        unreachable!()
    };
    let parent = snapshot.channels.iter().find(|c| c.id == ChannelId(30));
    let kept = snapshot.channels.iter().find(|c| c.id == ChannelId(50));
    assert_eq!(
        kept.map(|c| c.permissions),
        parent.map(|c| c.permissions),
        "kept with what its parent allows"
    );
    assert!(kept.unwrap().thread.as_ref().unwrap().archived);

    // The session still knows it: its history, a reopening, its deletion.
    app.running
        .commands
        .send(Command::LoadHistory {
            request: 2,
            channel: ChannelId(50),
            before: None,
            limit: 50,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/50/messages?limit=50 "));
    reply.send(response(200, "[]")).unwrap();
    app.th_until(|e| matches!(e, Event::HistoryLoaded { request: 2, .. }))
        .await;
    app.running
        .commands
        .send(Command::EditThread {
            thread: ChannelId(50),
            edit: ThreadEdit {
                archived: Some(false),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/channels/50 "));
    reply
        .send(response(200, &archived_thread(false).to_string()))
        .unwrap();
    app.th_until(|e| matches!(e, Event::ChannelChanged(c) if c.id == ChannelId(50)))
        .await;
    app.dispatch
        .send((
            "THREAD_DELETE",
            json!({"id":"50","guild_id":"10","parent_id":"30","type":11}),
        ))
        .await
        .unwrap();
    app.th_until(|e| matches!(e, Event::ChannelRemoved { channel } if *channel == ChannelId(50)))
        .await;
}
