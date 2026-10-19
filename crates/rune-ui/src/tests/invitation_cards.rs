use super::*;
use rune_components::message::InvitationCard;
use rune_model::{ServerId, TextReference, TextTarget};
use rune_provider::{InvitationError, InvitationPreview, InvitationResult, InvitationTarget};
use std::collections::{HashMap, HashSet};

const BASE: &str = "https://fixture.invalid/invite/";

fn recognize(url: &str) -> Option<InvitationTarget> {
    let key = url.strip_prefix(BASE)?;
    Some(InvitationTarget {
        key: key.into(),
        url: format!("https://fixture.invalid/page/{key}"),
    })
}

fn preview(server: ServerId, name: &str) -> InvitationPreview {
    InvitationPreview {
        destination: rune_provider::InvitationDestination::Server(server),
        name: name.into(),
        description: None,
        image: None,
        members: Some(45),
        online: Some(12),
        expires_at: None,
        event: None,
    }
}

fn answer(
    host: &mut Host,
    request: u64,
    target: &InvitationTarget,
    result: Result<InvitationPreview, InvitationError>,
) {
    let now = host.now;
    invitation_card_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target: target.clone(),
            result,
        },
        now,
    );
}

fn click(host: &mut Host, id: MessageId, what: &str) -> Vec<Intent> {
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{id}:{what}"),
        },
        host.now,
    )
}

#[test]
fn visible_invitation_cards_look_up_their_server_bounded_cached_and_never_join() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
    capabilities.invitation_preview = true;
    // Without accepting here, «Unirse» opens the browser.
    capabilities.invitation_accept = false;
    let member_of = host.store.channel(channel).unwrap().server.unwrap();
    let last = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    // Twenty links; the last two name the same invitation.
    let mut messages: HashMap<String, MessageId> = HashMap::new();
    for index in 0..20u64 {
        let key = format!("k{}", index.min(18));
        let mut message = last.clone();
        message.id = MessageId(last.id.0 + 1 + index);
        message.content = format!("{BASE}{key}");
        message.text_references = vec![TextReference {
            source: message.content.clone(),
            target: TextTarget::Invitation,
        }];
        messages.entry(key).or_insert(message.id);
        host.event(Event::MessageCreated {
            message,
            nonce: None,
        });
    }
    let mut intents = Vec::new();
    host.frame();
    assert!(host.painted().join(" ").contains("Consultando invitación…"));
    let first = wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize);
    let keys: HashSet<_> = first.iter().map(|(_, target)| target.key.clone()).collect();
    assert!(keys.contains("k18") && keys.contains("k17"), "{keys:?}");
    assert_eq!(keys.len(), first.len(), "one lookup per invitation");
    assert!(first.len() >= 3 && first.len() < 8, "{}", first.len());
    assert!(
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize)
            .is_empty(),
        "lookups under way are not repeated"
    );

    // Older links come into view: only what is left of the eight starts.
    let index = host.store.timeline(channel).unwrap().len() - 20;
    host.tree.scroll_to_index(&timeline_id(channel), index);
    host.frame();
    let second =
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize);
    assert_eq!(first.len() + second.len(), 8);
    assert!(
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize)
            .is_empty()
    );

    let (joinable, member, invalid) = (&first[0], &first[1], &first[2]);
    let id = |target: &InvitationTarget| messages[&target.key];
    // An answer to an older lookup of the same invitation is stale.
    let stale = joinable.0.wrapping_add(10_000);
    answer(
        &mut host,
        stale,
        &joinable.1,
        Ok(preview(ServerId(77), "Viejo")),
    );
    let found = host
        .store
        .timeline(channel)
        .unwrap()
        .get(id(&joinable.1))
        .unwrap()
        .clone();
    assert!(matches!(
        crate::invitation_cards::cards(&host.ui, &host.store, &found)[..],
        [InvitationCard::Loading]
    ));
    answer(
        &mut host,
        joinable.0,
        &joinable.1,
        Ok(preview(ServerId(77), "Jardín")),
    );
    answer(
        &mut host,
        member.0,
        &member.1,
        Ok(preview(member_of, "Taller")),
    );
    answer(
        &mut host,
        invalid.0,
        &invalid.1,
        Err(InvitationError::Invalid),
    );
    host.tree.scroll_to_index(
        &timeline_id(channel),
        host.store.timeline(channel).unwrap().len() - 1,
    );
    let painted = host.painted().join(" ");
    for part in [
        "Te invitaron a unirte a un servidor",
        "Jardín",
        "12 en línea",
        "45 miembros",
        "Unirse",
        "Ya sos miembro de",
        "Taller",
        "Ir al servidor",
        "Invitación no disponible",
    ] {
        assert!(painted.contains(part), "{part} in {painted}");
    }

    // Answers stand for five minutes; lost lookups are retried sooner.
    let later = host.now + Duration::from_secs(4 * 60);
    let retried = wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, later, recognize);
    let answered: HashSet<_> = [joinable, member, invalid]
        .iter()
        .map(|(_, target)| target.key.clone())
        .collect();
    assert!(!retried.is_empty());
    assert!(retried
        .iter()
        .all(|(_, target)| !answered.contains(&target.key)));
    let stale_now = host.now + Duration::from_secs(6 * 60);
    let refreshed =
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, stale_now, recognize);
    let refreshed: HashSet<_> = refreshed
        .iter()
        .map(|(_, target)| target.key.clone())
        .collect();
    assert!(answered.is_subset(&refreshed), "{refreshed:?}");
    // A refresh keeps showing the last answer meanwhile.
    assert!(host.painted().join(" ").contains("Jardín"));

    // «Unirse» opens the invitation's page; «Ir al servidor» opens the server.
    let join = click(&mut host, id(&joinable.1), "invitation:0:join");
    assert_eq!(join, vec![Intent::OpenInBrowser(joinable.1.url.clone())]);
    // Where the connection accepts invitations, it opens them in RUNE instead,
    // which asks again before accepting.
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_accept = true;
    let review = click(&mut host, id(&joinable.1), "invitation:0:join");
    assert!(
        matches!(&review[..], [Intent::ReviewInvitation { message, .. }] if *message == id(&joinable.1)),
        "{review:?}"
    );
    assert!(click(&mut host, id(&joinable.1), "invitation:0:open").is_empty());
    assert!(click(&mut host, id(&member.1), "invitation:0:join").is_empty());
    assert!(click(&mut host, id(&invalid.1), "invitation:0:join").is_empty());
    intents.extend(join);
    let open = click(&mut host, id(&member.1), "invitation:0:open");
    assert!(!open.is_empty());
    assert_eq!(host.ui.server, Some(member_of));
    intents.extend(open);
    host.run(intents.clone());
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::Command(_) | Intent::OpenUrl(_))));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::AcceptInvitation { .. })));
}

#[test]
fn every_invitation_of_a_message_has_its_card_and_each_kind_its_way_in() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    let friend = host
        .store
        .relationships()
        .unwrap()
        .iter()
        .find(|known| {
            known.kind == rune_model::RelationshipKind::Friend
                && host.store.direct_with(known.user).is_some()
        })
        .unwrap()
        .user;
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content =
        format!("Al grupo {BASE}grupo, sumate como amigo {BASE}amigo y también {BASE}otro.");
    message.text_references = ["grupo", "amigo", "otro"]
        .iter()
        .map(|key| TextReference {
            source: format!("{BASE}{key}"),
            target: TextTarget::Invitation,
        })
        .collect();
    let id = message.id;
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    let wanted =
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize);
    let keys: Vec<&str> = wanted
        .iter()
        .map(|(_, target)| target.key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec!["grupo", "amigo", "otro"],
        "one lookup per link, in order"
    );
    let group = rune_provider::InvitationDestination::Group(rune_model::ChannelId(4_242));
    let befriend = rune_provider::InvitationDestination::Friend(friend);
    for ((request, target), destination, name) in
        [(&wanted[0], group, ""), (&wanted[1], befriend, "Ada")]
    {
        answer(
            &mut host,
            *request,
            target,
            Ok(InvitationPreview {
                destination,
                ..preview(ServerId(1), name)
            }),
        );
    }
    answer(
        &mut host,
        wanted[2].0,
        &wanted[2].1,
        Err(InvitationError::Invalid),
    );
    let painted = host.painted().join(" ");
    for part in [
        "Te invitaron a unirte a un grupo",
        "Grupo sin nombre",
        "Ya son amigos",
        "Enviar mensaje",
        "Invitación no disponible",
    ] {
        assert!(painted.contains(part), "{part} in {painted}");
    }
    // The group's «Unirse» opens it in RUNE; the friend's card opens the
    // conversation with them; nothing is accepted from the card.
    assert!(matches!(
        &click(&mut host, id, "invitation:0:join")[..],
        [Intent::ReviewInvitation { source, .. }] if *source == format!("{BASE}grupo")
    ));
    assert!(click(&mut host, id, "invitation:1:join").is_empty());
    assert!(click(&mut host, id, "invitation:9:review").is_empty());
    // Without that kind's capability, «Unirse» opens the browser.
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .group_invitations = false;
    assert_eq!(
        click(&mut host, id, "invitation:0:join"),
        vec![Intent::OpenInBrowser(wanted[0].1.url.clone())]
    );
    let open = click(&mut host, id, "invitation:1:open");
    assert!(!open.is_empty());
    assert_eq!(host.ui.channel(), host.store.direct_with(friend));
    assert!(!open
        .iter()
        .any(|intent| matches!(intent, Intent::Command(Command::AcceptInvitation { .. }))));
}

#[test]
fn without_lookups_the_card_only_offers_review() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = false;
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = format!("{BASE}k0");
    message.text_references = vec![TextReference {
        source: message.content.clone(),
        target: TextTarget::Invitation,
    }];
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    assert!(
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize)
            .is_empty()
    );
    let painted = host.painted().join(" ");
    assert!(painted.contains("Enlace de invitación") && !painted.contains("Unirse"));
}
