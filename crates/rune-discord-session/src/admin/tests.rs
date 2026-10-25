//! What may leave and what an answer keeps, against a session built here.

use rune_model::{permissions::in_channel, AuditQuery, Channel, Overwrite, Rgb, Role};
use rune_provider::{ChannelEdit, Created, RoleEdit, ServerEdit};

use super::*;

const ME: UserId = UserId(42);
const PEER: UserId = UserId(43);
const BOSS: UserId = UserId(44);
const OWNER: UserId = UserId(99);
const SERVER: ServerId = ServerId(10);
const MOD: RoleId = RoleId(11);
const HIGH: RoleId = RoleId(12);
const LOW: RoleId = RoleId(13);
const CHAT: ChannelId = ChannelId(30);

fn moderation() -> Permissions {
    Permissions::KICK
        | Permissions::BAN
        | Permissions::TIMEOUT_MEMBERS
        | Permissions::MANAGE_ROLES
        | Permissions::MANAGE_NICKS
        | Permissions::MANAGE_CHANNELS
        | Permissions::MANAGE_SERVER
        | Permissions::MANAGE_EMOJI
        | Permissions::VIEW_AUDIT_LOG
        | Permissions::MANAGE_WEBHOOKS
}

/// The account moderates (MOD, position 2) a server someone else owns;
/// PEER ranks below it (LOW, 1) and BOSS above (HIGH, 3).
fn session() -> Snapshot {
    let role = |id: RoleId, position, permissions| Role {
        style: Default::default(),
        id,
        name: format!("rol {}", id.0),
        color: None,
        position,
        hoisted: false,
        mentionable: false,
        permissions,
    };
    let server = Server {
        emojis: Vec::new(),
        id: SERVER,
        name: "Taller".into(),
        accent: Rgb(0),
        icon: None,
        roles: vec![
            Role::everyone(SERVER, Permissions::EVERYONE),
            role(MOD, 2, moderation()),
            role(HIGH, 3, Permissions::NONE),
            role(LOW, 1, Permissions::NONE),
        ],
        stickers: Vec::new(),
        owner: Some(OWNER),
        description: None,
        options: Default::default(),
    };
    let member = |user: UserId, roles: &[RoleId]| Member {
        user,
        server: SERVER,
        roles: roles.to_vec(),
        ..Default::default()
    };
    let mut snapshot = Snapshot {
        me: Some(wire::user(&json!({"id": "42", "username": "Owner"})).unwrap()),
        servers: vec![server],
        members: vec![
            member(ME, &[MOD]),
            member(PEER, &[LOW]),
            member(BOSS, &[HIGH]),
            member(OWNER, &[]),
        ],
        channels: vec![Channel {
            id: CHAT,
            server: Some(SERVER),
            kind: ChannelKind::Text,
            name: "general".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
            icon: None,
        }],
        ..Default::default()
    };
    recompute(&mut snapshot);
    snapshot
}

fn recompute(snapshot: &mut Snapshot) {
    let mine = snapshot.members[0].clone();
    let server = snapshot.servers[0].clone();
    for channel in &mut snapshot.channels {
        channel.permissions = in_channel(&server, &mine, channel);
    }
}

fn refused(snapshot: &Snapshot, command: Command) -> String {
    allowed(snapshot, &command)
        .expect_err(&format!("{command:?}"))
        .to_string()
}

fn kick(user: UserId) -> Command {
    Command::Kick {
        server: SERVER,
        user,
        reason: Some("spam".into()),
    }
}

#[test]
fn permission_bits_round_trip_and_keep_what_rune_does_not_model() {
    let read = |bits: u128| {
        bootstrap::role(&json!({"id": "1", "permissions": bits.to_string()}))
            .unwrap()
            .permissions
    };
    for (bit, permission) in bootstrap::PERMISSION_BITS {
        assert_eq!(discord_bits(permission), 1 << bit);
        assert_eq!(read(discord_bits(permission)), permission);
    }
    assert_eq!(read(discord_bits(Permissions::ALL)), Permissions::ALL);
    assert_eq!(discord_bits(Permissions::MANAGE_WEBHOOKS), 1 << 29);
    // Text-to-speech (12) and the soundboard (42) stay; the bits RUNE
    // models (here: invitations, 0) are what RUNE says.
    let raw = 1 << 12 | 1 << 42 | 1;
    assert_eq!(merged_bits(raw, Permissions::NONE), 1 << 12 | 1 << 42);
    assert_eq!(
        merged_bits(raw, Permissions::VIEW),
        1 << 12 | 1 << 42 | 1 << 10
    );
    // External stickers are modeled since 2026-10-18.
    assert_eq!(discord_bits(Permissions::EXTERNAL_STICKERS), 1 << 37);
    // Application commands are modeled since P3.
    assert_eq!(
        discord_bits(Permissions::USE_APP_COMMANDS),
        1 << 31,
        "use application commands"
    );
    assert_eq!(raw_bits(&json!("1099511627776")), Some(1 << 40));
    assert_eq!(raw_bits(&json!("-1")), None);
    assert_eq!(raw_bits(&json!("")), None);
}

#[test]
fn a_server_color_reads_from_its_profile() {
    assert_eq!(accent(&json!("#5865f2")), Some(Rgb(0x5865f2)));
    assert_eq!(accent(&json!(0xff0000)), Some(Rgb(0xff0000)));
    for bad in [
        json!("5865f2"),
        json!("#+12345"),
        json!(0x1000000),
        json!(null),
    ] {
        assert_eq!(accent(&bad), None, "{bad}");
    }
}

#[test]
fn moderation_needs_the_permission_and_a_higher_rank() {
    let snapshot = session();
    assert!(allowed(&snapshot, &kick(PEER)).is_ok());
    assert!(refused(&snapshot, kick(BOSS)).contains("altura"));
    assert!(refused(&snapshot, kick(OWNER)).contains("dueño"));
    assert!(refused(&snapshot, kick(ME)).contains("vos"));
    // Someone this session never loaded ranks lowest: Discord decides.
    assert!(allowed(&snapshot, &kick(UserId(77))).is_ok());
    let long = Command::Ban {
        server: SERVER,
        user: PEER,
        reason: Some("x".repeat(REASON_MAX + 1)),
        delete_message_seconds: 0,
    };
    assert!(refused(&snapshot, long).contains("motivo"));
    let too_much = Command::Ban {
        server: SERVER,
        user: PEER,
        reason: None,
        delete_message_seconds: MAX_DELETE_SECONDS + 1,
    };
    assert!(allowed(&snapshot, &too_much).is_err());
    let most = Command::Ban {
        server: SERVER,
        user: PEER,
        reason: None,
        delete_message_seconds: MAX_DELETE_SECONDS,
    };
    assert!(allowed(&snapshot, &most).is_ok());
    // Without the role, nothing.
    let mut plain = session();
    plain.members[0].roles.clear();
    assert!(refused(&plain, kick(PEER)).contains("permiso"));
    // The owner acts on anyone but itself.
    let mut owned = session();
    owned.servers[0].owner = Some(ME);
    assert!(allowed(&owned, &kick(BOSS)).is_ok());
    assert!(refused(&owned, Command::LeaveServer { server: SERVER }).contains("dueño"));
    assert!(allowed(&snapshot, &Command::LeaveServer { server: SERVER }).is_ok());
    assert!(refused(&snapshot, Command::DeleteServer { server: SERVER }).contains("dueño"));
}

#[test]
fn a_timeout_lasts_up_to_28_days_and_a_timed_out_account_moderates_nothing() {
    let snapshot = session();
    let timeout = |minutes: i64| Command::TimeoutMember {
        server: SERVER,
        user: PEER,
        until: Some(wall_now().plus_minutes(minutes)),
        reason: None,
    };
    assert!(allowed(&snapshot, &timeout(60)).is_ok());
    assert!(refused(&snapshot, timeout(-1)).contains("28 días"));
    assert!(refused(&snapshot, timeout(29 * 24 * 60)).contains("28 días"));
    let lift = Command::TimeoutMember {
        server: SERVER,
        user: PEER,
        until: None,
        reason: None,
    };
    assert!(allowed(&snapshot, &lift).is_ok());
    let mut restricted = session();
    restricted.members[0].restrictions.timeout_until = Some(wall_now().plus_minutes(5));
    assert!(refused(&restricted, kick(PEER)).contains("restringida"));
    // Leaving is still possible.
    assert!(allowed(&restricted, &Command::LeaveServer { server: SERVER }).is_ok());
}

#[test]
fn roles_change_only_below_and_never_grant_more_than_the_account_has() {
    let snapshot = session();
    let edit = |role, edit| Command::EditRole {
        server: SERVER,
        role,
        edit,
    };
    let rename = RoleEdit {
        name: Some("Nuevo".into()),
        ..Default::default()
    };
    assert!(allowed(&snapshot, &edit(LOW, rename.clone())).is_ok());
    assert!(refused(&snapshot, edit(HIGH, rename.clone())).contains("altura"));
    assert!(refused(&snapshot, edit(MOD, rename.clone())).contains("altura"));
    let admin = RoleEdit {
        permissions: Some(Permissions::ADMINISTRATOR),
        ..Default::default()
    };
    assert!(refused(&snapshot, edit(LOW, admin)).contains("no tenés"));
    let kick = RoleEdit {
        permissions: Some(Permissions::KICK),
        ..Default::default()
    };
    assert!(allowed(&snapshot, &edit(LOW, kick.clone())).is_ok());
    // @everyone changes only its permissions.
    assert!(allowed(&snapshot, &edit(RoleId(SERVER.0), kick)).is_ok());
    assert!(refused(&snapshot, edit(RoleId(SERVER.0), rename)).contains("@everyone"));
    let up = RoleEdit {
        position: Some(2),
        ..Default::default()
    };
    assert!(refused(&snapshot, edit(LOW, up)).contains("subir"));
    assert!(refused(&snapshot, edit(LOW, RoleEdit::default())).contains("cambios"));
    let create = |name: &str| Command::CreateRole {
        request: 1,
        server: SERVER,
        name: name.into(),
    };
    assert!(allowed(&snapshot, &create("Lectores")).is_ok());
    assert!(refused(&snapshot, create("  ")).contains("nombre"));
    let give = |roles: Vec<RoleId>| Command::SetMemberRoles {
        server: SERVER,
        user: PEER,
        roles,
    };
    assert!(allowed(&snapshot, &give(vec![LOW])).is_ok());
    assert!(allowed(&snapshot, &give(vec![])).is_ok());
    assert!(refused(&snapshot, give(vec![LOW, HIGH])).contains("altura"));
    assert!(allowed(&snapshot, &give(vec![LOW, LOW])).is_err());
    let stranger = Command::SetMemberRoles {
        server: SERVER,
        user: UserId(77),
        roles: vec![LOW],
    };
    assert!(refused(&snapshot, stranger).contains("Cargá"));
    assert!(refused(
        &snapshot,
        Command::DeleteRole {
            server: SERVER,
            role: RoleId(SERVER.0)
        }
    )
    .contains("@everyone"));
}

#[test]
fn channels_overwrites_and_names_are_checked_before_leaving() {
    let snapshot = session();
    let overwrite = |target, allow| Command::SetOverwrite {
        channel: CHAT,
        overwrite: Overwrite {
            target,
            allow,
            deny: Permissions::NONE,
        },
    };
    assert!(allowed(
        &snapshot,
        &overwrite(OverwriteTarget::Role(LOW), Permissions::SEND)
    )
    .is_ok());
    assert!(refused(
        &snapshot,
        overwrite(OverwriteTarget::Role(HIGH), Permissions::SEND)
    )
    .contains("altura"));
    assert!(refused(
        &snapshot,
        overwrite(OverwriteTarget::Member(PEER), Permissions::ADMINISTRATOR)
    )
    .contains("no tenés"));
    assert!(refused(
        &snapshot,
        Command::RemoveOverwrite {
            channel: CHAT,
            target: OverwriteTarget::Member(PEER)
        }
    )
    .contains("ya no"));
    let create = |kind, name: &str| Command::CreateChannel {
        request: 1,
        server: SERVER,
        kind,
        name: name.into(),
        parent: None,
        private: true,
    };
    assert!(allowed(&snapshot, &create(ChannelKind::Text, "anuncios")).is_ok());
    assert!(refused(&snapshot, create(ChannelKind::Thread, "x")).contains("tipo"));
    assert!(refused(&snapshot, create(ChannelKind::Text, "")).contains("nombre"));
    let slow = Command::EditChannel {
        channel: CHAT,
        edit: ChannelEdit {
            slowmode_secs: Some(21_601),
            ..Default::default()
        },
    };
    assert!(refused(&snapshot, slow).contains("6 horas"));
    let unknown = Command::DeleteChannel {
        channel: ChannelId(31),
    };
    assert!(refused(&snapshot, unknown).contains("ya no"));
    let server = Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            description: Some(Some("x".repeat(301))),
            ..Default::default()
        },
    };
    assert!(refused(&snapshot, server).contains("300"));
    let hook = |name: &str| Command::CreateWebhook {
        server: SERVER,
        channel: CHAT,
        name: name.into(),
    };
    assert!(allowed(&snapshot, &hook("Avisos de CI")).is_ok());
    assert!(refused(&snapshot, hook("Mi Discord")).contains("discord"));
    let invite = |max_age_secs| Command::CreateInvite {
        request: 1,
        server: SERVER,
        channel: CHAT,
        max_age_secs,
        max_uses: 0,
        temporary: false,
    };
    assert!(allowed(&snapshot, &invite(INVITE_MAX_AGE)).is_ok());
    assert!(refused(&snapshot, invite(INVITE_MAX_AGE + 1)).contains("7 días"));
    let prune = |days| Command::PruneMembers {
        request: 1,
        server: SERVER,
        days,
        remove: false,
    };
    assert!(allowed(&snapshot, &prune(7)).is_ok());
    assert!(refused(&snapshot, prune(31)).contains("30"));
    let audit = |action| Command::LoadAudit {
        request: 1,
        server: SERVER,
        query: AuditQuery {
            actor: None,
            action,
        },
        before: None,
        limit: 50,
    };
    assert!(allowed(&snapshot, &audit(Some(AuditActionKind::NickChanged))).is_ok());
    assert!(refused(&snapshot, audit(Some(AuditActionKind::Other))).contains("filtro"));
    let page = |after: &str| Command::LoadMembers {
        request: 1,
        server: SERVER,
        after: Some(after.into()),
        query: Default::default(),
    };
    assert!(allowed(&snapshot, &page("43:1000")).is_ok());
    assert!(refused(&snapshot, page("nope")).contains("cursor"));
    let foreign = Command::LoadBans {
        request: 1,
        server: ServerId(11),
        after: None,
        limit: 50,
    };
    assert!(refused(&snapshot, foreign).contains("ya no"));
}

#[test]
fn an_answer_keeps_what_it_made_and_reports_it_before_its_creation() {
    let mut snapshot = session();
    let created = Event::Created {
        request: 1,
        created: Created::Role(SERVER, RoleId(20)),
    };
    let echo = || {
        Echo::Dispatch(
            "GUILD_ROLE_CREATE",
            json!({"guild_id": "10", "role": {"id": "20", "name": "Nuevo", "permissions": "0", "position": 1}}),
        )
    };
    let events = confirmed(&mut snapshot, vec![echo()], vec![created.clone()]);
    assert!(matches!(&events[0], Event::ServerChanged(server)
        if server.roles.iter().any(|role| role.id == RoleId(20))));
    assert_eq!(events.last(), Some(&created));
    // An edit right after validates against what the answer kept.
    let rename = Command::EditRole {
        server: SERVER,
        role: RoleId(20),
        edit: RoleEdit {
            name: Some("Otro".into()),
            ..Default::default()
        },
    };
    assert!(allowed(&snapshot, &rename).is_ok());
    // The Gateway told it first: its copy stays and nothing repeats.
    assert_eq!(confirmed(&mut snapshot, vec![echo()], Vec::new()), []);
    // An echo that cannot apply leaves the session as it was.
    let before = snapshot.clone();
    let broken = Echo::Dispatch(
        "GUILD_ROLE_UPDATE",
        json!({"guild_id": "10", "role": {"id": "x"}}),
    );
    assert_eq!(confirmed(&mut snapshot, vec![broken], Vec::new()), []);
    assert_eq!(snapshot, before);
    // A name an audit entry left out comes from the session.
    let page = Event::AuditLoaded {
        request: 2,
        server: SERVER,
        query: AuditQuery::default(),
        before: None,
        entries: vec![rune_model::AuditEntry {
            id: 5,
            actor: ME,
            action: rune_model::AuditAction::ChannelUpdated {
                channel: CHAT,
                name: String::new(),
            },
            when: Default::default(),
            reason: None,
        }],
        reached_end: true,
    };
    let named = confirmed(&mut snapshot, Vec::new(), vec![page]);
    assert!(matches!(&named[..], [Event::AuditLoaded { entries, .. }]
        if entries[0].action == rune_model::AuditAction::ChannelUpdated { channel: CHAT, name: "general".into() }));
    // An emoji the answer names joins its server.
    let emoji = Echo::Expression {
        server: SERVER,
        expression: ExpressionId::Emoji(rune_model::EmojiId(7)),
        answer: Some(json!({"id": "7", "name": "ferris"})),
    };
    let events = confirmed(&mut snapshot, vec![emoji], Vec::new());
    assert!(
        matches!(&events[..], [Event::ServerChanged(server)] if server.emojis[0].name == "ferris")
    );
    assert!(allowed(
        &snapshot,
        &Command::RenameExpression {
            server: SERVER,
            expression: ExpressionId::Emoji(rune_model::EmojiId(7)),
            name: "cangrejo".into(),
        }
    )
    .is_ok());
}

#[test]
fn bans_from_the_gateway_and_requests_in_flight() {
    let snapshot = session();
    let added = dispatch(
        &snapshot,
        "GUILD_BAN_ADD",
        &json!({"guild_id": "10", "user": {"id": "43", "username": "peer"}}),
    )
    .unwrap();
    assert!(
        matches!(&added[..], [Event::UserChanged(_), Event::BanAdded { ban, .. }] if ban.user == PEER)
    );
    let foreign = dispatch(
        &snapshot,
        "GUILD_BAN_REMOVE",
        &json!({"guild_id": "11", "user": {"id": "43"}}),
    );
    assert_eq!(foreign, Some(Vec::new()));
    assert_eq!(dispatch(&snapshot, "GUILD_UPDATE", &json!({})), None);
    // A request in flight stands while its server does.
    assert!(in_flight(&snapshot, &kick(PEER)).is_none());
    let mut left = session();
    left.servers.clear();
    assert!(in_flight(&left, &kick(PEER)).is_some());
    assert!(in_flight(&left, &Command::LeaveServer { server: SERVER }).is_none());
    // A channel the Gateway just deleted: its own answer says what happened.
    let gone = Command::DeleteChannel {
        channel: ChannelId(31),
    };
    assert!(in_flight(&snapshot, &gone).is_none());
}
