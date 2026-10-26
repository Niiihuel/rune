//! The whole window, driven as a person would, over the simulated world.
//! A small host carries out the intents at once, without a runtime.

use std::time::{Duration, Instant};

use limn::{Frame, Key, Point, Size, UiAction, UiEvent, UiTree};
use rune_model::{ChannelId, ChannelKind, MessageId, Timestamp};
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::{Command, Event, Provider};
use rune_state::Store;

use super::*;
mod account;
mod inbox;

const WINDOW: Size = Size::new(1280.0, 800.0);

struct Host {
    ui: Ui,
    store: Store,
    tree: UiTree,
    provider: MockProvider,
    sent: Vec<Command>,
    upload_intents: Vec<Intent>,
    local_searches: Vec<(
        u64,
        rune_model::SearchQuery,
        Option<rune_model::SearchCursor>,
    )>,
    context_reads: Vec<(u64, ChannelId, MessageId, rune_model::HistorySource)>,
    nonce: u64,
    now: Instant,
    /// Focus asked for, given after the next layout: a control that was
    /// just opened is not in the tree before it.
    focus: Option<(String, Option<usize>)>,
    /// Sent messages are confirmed at once.
    confirm: bool,
    active: bool,
    confirm_reads: bool,
    // Voice protocol (voz-protocolo).
    /// The servers the provider was told the column shows, apart from
    /// `sent`: every server switch tells one.
    servers_shown: Vec<Option<rune_model::ServerId>>,
}

impl Host {
    fn new() -> Self {
        let provider = MockProvider::new(MockConfig {
            seed: 11,
            history_end: Timestamp(1_790_000_000_000),
            clock: || Timestamp(1_790_000_100_000),
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
        });
        let mut store = Store::new();
        store.provider = Some(provider.info());
        // Restrictions are read at the simulation's clock.
        store.clock = Some(|| Timestamp(1_790_000_100_000));
        let mut tree = UiTree::new(limn::Element::spacer());
        tree.set_reduced_motion(true);
        tree.set_text_measurer(|text, size, _| text.chars().count() as f32 * size * 0.5);
        let mut host = Self {
            ui: Ui::default(),
            store,
            tree,
            provider,
            sent: Vec::new(),
            upload_intents: Vec::new(),
            local_searches: Vec::new(),
            context_reads: Vec::new(),
            nonce: 0,
            now: Instant::now(),
            focus: None,
            confirm: false,
            active: true,
            confirm_reads: true,
            servers_shown: Vec::new(),
        };
        let snapshot = host.provider.snapshot();
        set_upload_limits(&mut host.ui, host.provider.upload_limits());
        host.event(Event::Ready(Box::new(snapshot)));
        host
    }

    /// Let the provider leave notification settings to this machine, as
    /// one without `synced_notifications` does.
    fn local_notifications(&mut self) {
        if let Some(info) = self.store.provider.as_mut() {
            info.capabilities.synced_notifications = false;
        }
    }

    /// Carry out the account settings commands sent since `from` on the
    /// simulation, as its session would answer them.
    fn answer_account(&mut self, from: usize) {
        let sent: Vec<Command> = self.sent[from..].to_vec();
        for command in sent {
            for event in self.provider.manage(command) {
                self.event(event);
            }
        }
    }

    fn event(&mut self, event: Event) {
        let changed = match &event {
            Event::MessageCreated { message, .. } | Event::MessageUpdated(message) => {
                Some(message.channel)
            }
            Event::MessagePatched(patch) => Some(patch.channel),
            Event::MessageDeleted { channel, .. }
            | Event::HistoryLoaded { channel, .. }
            | Event::ContextLoaded { channel, .. } => Some(*channel),
            _ => None,
        };
        let applied = self.store.apply(event, self.now);
        let mut intents = changed
            .map(|channel| local_messages_changed(&mut self.ui, channel))
            .unwrap_or_default();
        intents.extend(on_applied(
            &mut self.ui,
            &mut self.store,
            &applied,
            self.now,
        ));
        self.run(intents);
    }

    /// Carry out what the interface asked, as the desktop host does.
    fn run(&mut self, intents: Vec<Intent>) {
        for intent in intents {
            match intent {
                intent @ (Intent::PickAttachment { .. }
                | Intent::PasteAttachments { .. }
                | Intent::CancelUpload { .. }
                | Intent::RemoveAttachment { .. }
                | Intent::SendAttachments { .. }
                | Intent::RetryAttachmentSend { .. }
                | Intent::CreatePost { .. }
                | Intent::InteractWithFiles { .. }
                // The host's own selector, as for attachments.
                | Intent::PickProfileImage { .. }
                | Intent::PickExpressionImage { .. }
                | Intent::CancelProfileImage
                | Intent::FrameProfileImage { .. }) => self.upload_intents.push(intent),
                Intent::LoadLatest(channel) => self.page(channel, None),
                Intent::LoadOlder(channel, before) => self.page(channel, Some(before)),
                Intent::SearchLocal {
                    request,
                    query,
                    cursor,
                } => self.local_searches.push((request, query, cursor)),
                Intent::LoadContext {
                    request,
                    channel,
                    anchor,
                    source,
                } => {
                    let permit = self.store.begin_context(channel, request, anchor, source);
                    if let Some(reason) = permit.reason() {
                        let applied = rune_state::Applied::Failed {
                            failure: rune_provider::Failure::Context { request, channel },
                            reason: reason.into(),
                            ambiguous: false,
                        };
                        let intents = on_applied(&mut self.ui, &mut self.store, &applied, self.now);
                        self.run(intents);
                    } else {
                        self.context_reads.push((request, channel, anchor, source));
                    }
                }
                Intent::Send {
                    channel,
                    content,
                    reply_to,
                    mention_reply,
                } => {
                    self.nonce += 1;
                    self.store.add_pending(
                        channel,
                        content.clone(),
                        reply_to,
                        self.nonce,
                        Timestamp(1_790_000_100_000),
                    );
                    self.sent.push(Command::SendMessage {
                        channel,
                        content: content.clone(),
                        reply_to,
                        nonce: self.nonce,
                        mention_reply,
                    });
                    if self.confirm {
                        // The provider confirms it, after every message it had.
                        let newest = self
                            .store
                            .timeline(channel)
                            .and_then(|timeline| timeline.newest())
                            .map_or(1, |message| message.id.0 + 1);
                        let mut message = (*self
                            .store
                            .timeline(channel)
                            .unwrap()
                            .messages()
                            .last()
                            .unwrap()
                            .clone())
                        .clone();
                        message.id = MessageId(newest);
                        message.delivery = rune_model::Delivery::Sent;
                        self.event(Event::MessageCreated {
                            message,
                            nonce: Some(self.nonce),
                        });
                    }
                }
                Intent::SendSticker { channel, sticker } => {
                    self.nonce += 1;
                    self.store.add_pending_sticker(
                        channel,
                        sticker.clone(),
                        self.nonce,
                        Timestamp(1_790_000_100_000),
                    );
                    self.sent.push(Command::SendSticker {
                        channel,
                        sticker: sticker.id,
                        nonce: self.nonce,
                    });
                }
                Intent::Resend { channel, message } => {
                    let unsent = self
                        .store
                        .unsent(channel, message)
                        .expect("only unsent messages are retried");
                    if !unsent.files.is_empty() {
                        self.upload_intents.push(Intent::RetryAttachmentSend {
                            request: self.ui.fresh_request(),
                            channel,
                            message,
                        });
                        continue;
                    }
                    self.nonce += 1;
                    self.store.resend(channel, message, self.nonce);
                    self.sent.push(if let Some(sticker) = unsent.sticker {
                        Command::SendSticker {
                            channel,
                            sticker,
                            nonce: self.nonce,
                        }
                    } else {
                        Command::SendMessage {
                            channel,
                            content: unsent.content,
                            reply_to: unsent.reply_to,
                            nonce: self.nonce,
                            mention_reply: unsent.mention_reply,
                        }
                    });
                }
                Intent::Command(command) => {
                    match &command {
                        Command::LoadPins { request, channel } => {
                            let permit = self.store.begin_pins(*channel, *request);
                            if let Some(reason) = permit.reason() {
                                let applied = rune_state::Applied::Failed {
                                    failure: rune_provider::Failure::Pins {
                                        request: *request,
                                        channel: *channel,
                                    },
                                    reason: reason.to_owned(),
                                    ambiguous: false,
                                };
                                let intents =
                                    on_applied(&mut self.ui, &mut self.store, &applied, self.now);
                                self.run(intents);
                                continue;
                            }
                        }
                        Command::Cancel(request) => {
                            self.store.cancel_pins(*request);
                            self.store.cancel_context(*request);
                        }
                        Command::WatchServer(server) => {
                            self.servers_shown.push(*server);
                            continue;
                        }
                        // Scheduled events (eventos-programados): the list
                        // the column's entry counts, answered as the
                        // simulation would, apart from `sent`.
                        Command::LoadScheduledEvents { .. } => {
                            for event in self.provider.manage(command.clone()) {
                                self.event(event);
                            }
                            continue;
                        }
                        _ => {}
                    }
                    let acknowledgement = match &command {
                        Command::MarkRead {
                            request,
                            account,
                            channel,
                            message,
                        } if self.confirm_reads => Some(Event::ReadStateConfirmed {
                            account: *account,
                            request: Some(*request),
                            channel: *channel,
                            message: *message,
                            version: self.store.read(*channel).confirmed_version + 1,
                        }),
                        _ => None,
                    };
                    self.sent.push(command);
                    if let Some(acknowledgement) = acknowledgement {
                        self.event(acknowledgement);
                    }
                }
                Intent::SetInput { id, value } => {
                    let end = value.chars().count();
                    self.tree.set_input_value(&id, value);
                    self.tree.set_input_cursor(&id, end);
                }
                Intent::Focus(id) => self.focus = Some((id, None)),
                Intent::FocusInput { id, cursor } => {
                    let focused = self.tree.focus(&id);
                    self.tree.set_input_cursor(&id, cursor);
                    self.focus = (!focused).then_some((id, Some(cursor)));
                }
                Intent::ScrollToEnd(id) => self.tree.scroll_to_end(&id),
                Intent::ScrollToIndex { list, index } => self.tree.scroll_to_index(&list, index),
                Intent::SetScrollOffset { list, offset } => {
                    self.tree.set_scroll_offset(&list, offset)
                }
                Intent::Reveal { id, margin } => self.tree.reveal_at_start(&id, margin),
                // Message actions (P4).
                Intent::SelectInput { id, start, end } => {
                    let focused = self.tree.focus(&id);
                    self.tree.set_input_selection(&id, start, end);
                    self.focus = (!focused).then_some((id, Some(end)));
                }
                _ => {}
            }
        }
    }

    fn page(&mut self, channel: ChannelId, before: Option<MessageId>) {
        let (messages, reached_start) = self.provider.history(channel, before, 50);
        self.event(Event::HistoryLoaded {
            request: 0,
            channel,
            before,
            messages,
            reached_start,
        });
    }

    fn context(&self) -> ViewContext {
        ViewContext {
            window: WINDOW,
            now: self.now,
            wall: Timestamp(1_790_000_100_000),
            at_end: true,
            loading_older: false,
            loading_latest: false,
            metrics: Vec::new(),
        }
    }

    fn frame(&mut self) -> Frame {
        self.tree
            .set_root(view(&self.ui, &self.store, &self.context()));
        let mut frame = self.tree.layout(WINDOW);
        if let Some((id, cursor)) = self.focus.take() {
            self.tree.focus(&id);
            if let Some(cursor) = cursor {
                self.tree.set_input_cursor(&id, cursor);
            }
            frame = self.tree.layout(WINDOW);
        }
        let intents = after_layout(&mut self.ui, &mut self.store, &self.tree, self.active);
        self.run(intents);
        frame
    }

    /// Lay out without the effects of a frame: the tree matches the state.
    fn layout_only(&mut self) {
        self.tree
            .set_root(view(&self.ui, &self.store, &self.context()));
        self.tree.layout(WINDOW);
    }

    /// Move the list `id` a pixel up and back to its end, as a scroll does:
    /// its bar, hidden 600 ms after the list last moved, shows on the next
    /// layout however long the run took to get there.
    fn stir(&mut self, id: &str) {
        let end = self.tree.scroll_offset(id);
        self.tree.set_scroll_offset(id, end - 1.0);
        self.layout_only();
        self.tree.scroll_to_end(id);
    }

    /// Open the channel `name` the way a person does: its server, then it.
    fn open_channel(&mut self, name: &str) -> ChannelId {
        let channel = self.channel_named(name);
        let server = self
            .store
            .channel(channel)
            .and_then(|channel| channel.server)
            .unwrap();
        self.click(&format!("server:{server}"));
        self.click(&format!("channel:{channel}"));
        self.frame();
        // Message-action fixtures act on the latest rows explicitly; opening
        // an unread channel now reveals its first actual unread anchor.
        self.tree.scroll_to_end(&timeline_id(channel));
        self.frame();
        channel
    }

    /// Deliver `event` as the desktop host does: keys the window takes first,
    /// then the tree, then the actions it reports.
    fn send(&mut self, event: UiEvent) {
        self.frame();
        stir(&mut self.ui, &event, self.now);
        capture_composer_selection(&mut self.ui, &self.tree);
        let focused = self.tree.focused_id().map(str::to_owned);
        if let UiEvent::ContextMenu { position } = &event {
            let target = self.tree.frame().hit_test(*position).map(str::to_owned);
            if let Some(intents) = target
                .and_then(|target| secondary_click(&mut self.ui, &self.store, &target, *position))
            {
                self.run(intents);
                return;
            }
        }
        if let UiEvent::PointerDownEditor {
            position,
            click_count: 2,
            ..
        } = &event
        {
            let target = self.tree.frame().hit_test(*position).map(str::to_owned);
            if let Some(intents) = double_click(&mut self.ui, &self.store, target.as_deref()) {
                self.run(intents);
                return;
            }
        }
        if let Some(intents) = intercept(&mut self.ui, &mut self.store, &event, focused.as_deref())
        {
            self.run(intents);
            return;
        }
        for action in self.tree.handle_event(event) {
            let intents = update(&mut self.ui, &mut self.store, &action, self.now);
            self.run(intents);
        }
    }

    fn click(&mut self, id: &str) {
        let mut frame = self.frame();
        let shown = |frame: &Frame| {
            frame
                .regions
                .iter()
                .any(|region| region.id == id && region.kind == limn::HitKind::Button)
        };
        // A control scrolled out of its list is brought into view first, as
        // a person would (AutoMod made the settings' sidebar longer).
        if !shown(&frame) {
            self.tree.reveal(id);
            frame = self.frame();
        }
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id && region.kind == limn::HitKind::Button)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        let center = Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        );
        self.send(UiEvent::PointerDown(center));
        self.send(UiEvent::PointerUp(center));
    }

    fn key(&mut self, key: Key, shift: bool, ctrl: bool) {
        self.send(UiEvent::Key {
            key,
            shift,
            ctrl,
            alt: false,
        });
    }

    fn type_text(&mut self, text: &str) {
        self.send(UiEvent::Text(text.into()));
    }

    fn channel_named(&self, name: &str) -> ChannelId {
        self.store
            .servers()
            .iter()
            .flat_map(|server| self.store.channel_tree(server.id))
            .flat_map(|(_, channels)| channels)
            .find(|channel| channel.name == name)
            .map(|channel| channel.id)
            .unwrap()
    }

    fn painted(&mut self) -> Vec<String> {
        let frame = self.frame();
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn ready_opens_the_first_channel_and_loads_its_newest_page() {
    let mut host = Host::new();
    let channel = host.ui.channel().expect("a channel is open");
    assert_eq!(host.store.channel(channel).unwrap().kind, ChannelKind::Text);
    let timeline = host.store.timeline(channel).unwrap();
    assert!(
        timeline.latest && timeline.len() == 14,
        "bienvenida has its 14 messages: {}",
        timeline.len()
    );
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "Taller de Rust"),
        "the server's name heads the column"
    );
    assert!(
        painted
            .iter()
            .any(|text| text.contains("se unió al servidor")),
        "the welcome channel shows who joined"
    );
}

#[test]
fn clicking_a_channel_opens_it_and_shows_its_messages() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    host.click(&format!("channel:{general}"));
    assert_eq!(host.ui.channel(), Some(general));
    assert_eq!(host.store.timeline(general).unwrap().len(), 50);
    host.frame();
    let shown = host
        .tree
        .visible_rows(&timeline_id(general))
        .expect("the timeline is laid out");
    let first = host.store.read(general).first_unread.unwrap();
    let index = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .position(|message| message.id == first)
        .unwrap();
    assert!(
        shown.contains(&index),
        "opening reveals the real first unread message"
    );
    assert!(shown.len() < 50, "only what fits is built: {shown:?}");
}

#[test]
fn scrolling_to_the_top_loads_older_pages_without_moving_what_is_read() {
    let mut host = Host::new();
    let channel = host.open_channel("rendimiento");
    for _ in 0..3 {
        host.frame();
        host.tree.scroll_to_start(&timeline_id(channel));
        host.frame();
    }
    let loaded = host.store.timeline(channel).unwrap().len();
    assert!(
        loaded >= 150,
        "pages came in as the top was reached: {loaded}"
    );
    // Older pages arrive above what is in view and leave it in place.
    host.layout_only();
    let first = host.tree.visible_rows(&timeline_id(channel)).unwrap().start;
    let watched = host.store.timeline(channel).unwrap().messages()[first].id;
    let anchor = format!("anchor:{watched}");
    let before = host
        .tree
        .anchor_offset(&anchor)
        .expect("the watched row is laid out");
    let oldest = host.store.timeline(channel).unwrap().oldest().unwrap();
    host.page(channel, Some(oldest));
    host.layout_only();
    assert_eq!(host.tree.anchor_offset(&anchor), Some(before));
}

#[test]
fn enter_sends_what_was_written_and_empties_the_composer() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("hola **mundo**");
    host.key(Key::Enter, false, false);
    assert!(
        matches!(host.sent.last(), Some(Command::SendMessage { content, .. }) if content == "hola **mundo**")
    );
    let newest = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .cloned()
        .unwrap();
    assert!(
        newest.delivery == rune_model::Delivery::Sending && newest.content == "hola **mundo**",
        "it shows at once, pending"
    );
    host.frame();
    assert_eq!(host.tree.input_value(&editor_id(general)), Some(""));
    host.key(Key::Enter, true, false);
    assert_eq!(
        host.tree.input_value(&editor_id(general)),
        Some("\n"),
        "Shift+Enter breaks the line"
    );
}

#[test]
fn up_in_an_empty_composer_edits_the_last_own_message_and_escape_cancels() {
    let mut host = Host::new();
    host.confirm = true;
    let general = host.open_channel("general");
    host.type_text("primero");
    host.key(Key::Enter, false, false);
    let mine = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    host.key(Key::ArrowUp, false, false);
    assert_eq!(host.ui.editing, Some(mine));
    host.frame();
    let editor = rune_components::message::editor_id(mine);
    assert_eq!(host.tree.input_value(&editor), Some("primero"));
    assert_eq!(
        host.tree.focused_id(),
        Some(editor.as_str()),
        "it is edited in place"
    );
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.editing, None);
}

#[test]
fn an_at_sign_offers_people_and_tab_completes() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let someone = host
        .store
        .member_groups(host.store.channel(general).unwrap().server.unwrap())
        .into_iter()
        .flat_map(|group| group.members)
        .filter_map(|user| host.store.user(user).cloned())
        .find(|user| !user.bot && Some(user.id) != host.store.me())
        .unwrap();
    host.type_text(&format!("hola @{}", &someone.name[..2]));
    let offered = completions(&host.ui, &host.store, general).expect("people are offered");
    let person = offered
        .items
        .iter()
        .find(|completion| completion.insert == format!("@{}", someone.name))
        .expect("the person is offered");
    // As the official client's: a face, and the handle at the end of the row.
    assert!(person.face.is_some());
    assert_eq!(
        (person.aside.as_str(), person.detail.as_str()),
        (someone.name.as_str(), "")
    );
    let first = offered.items[0].insert.clone();
    host.key(Key::Tab, false, false);
    host.frame();
    assert_eq!(
        host.tree.input_value(&editor_id(general)),
        Some(format!("hola {first} ").as_str())
    );
}

#[test]
fn reactions_toggle_at_once_and_reach_the_provider() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    host.click(&format!("channel:{general}"));
    let message = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("react:{message}:🦀"),
        },
        host.now,
    );
    let reacted = host
        .store
        .timeline(general)
        .unwrap()
        .get(message)
        .unwrap()
        .clone();
    assert!(reacted
        .reactions
        .iter()
        .any(|reaction| reaction.emoji == "🦀" && reaction.me));
    assert!(
        matches!(&intents[..], [Intent::Command(Command::ToggleReaction { emoji, .. })] if emoji == "🦀")
    );
}

#[test]
fn ctrl_k_finds_a_conversation_by_name_and_goes_there() {
    let mut host = Host::new();
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_some());
    host.frame();
    host.type_text("galer");
    host.frame();
    let found = settings::destinations(&host.ui, &host.store);
    assert_eq!(found.len(), 1, "{found:?}");
    host.key(Key::Enter, false, false);
    assert_eq!(host.ui.channel(), Some(host.channel_named("galería")));
    assert!(host.ui.switcher.is_none());
}

#[test]
fn alt_arrows_walk_the_channels_of_the_server() {
    let mut host = Host::new();
    let first = host.ui.channel().unwrap();
    run_navigate(&mut host, true);
    let second = host.ui.channel().unwrap();
    assert_ne!(first, second);
    run_navigate(&mut host, false);
    assert_eq!(host.ui.channel(), Some(first));
}

fn run_navigate(host: &mut Host, down: bool) {
    let intents = navigate(&mut host.ui, &mut host.store, down);
    host.run(intents);
}

#[test]
fn a_read_only_channel_says_why_nothing_can_be_written() {
    let mut host = Host::new();
    let announcements = host.open_channel("anuncios");
    let frame = host.frame();
    assert!(!frame
        .regions
        .iter()
        .any(|region| region.id == editor_id(announcements)
            && region.kind == limn::HitKind::TextArea));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("No tienes permiso")));
}

#[test]
fn the_window_lays_out_at_every_width_without_spilling() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    host.click(&format!("channel:{general}"));
    for width in [720.0, 1024.0, 1600.0] {
        let size = Size::new(width, 700.0);
        host.tree.set_root(view(
            &host.ui,
            &host.store,
            &ViewContext {
                window: size,
                ..host.context()
            },
        ));
        let frame = host.tree.layout(size);
        for region in frame
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
        {
            assert!(
                region.rect.x + region.rect.width <= width + 1.0,
                "{} spills at {width}: {:?}",
                region.id,
                region.rect
            );
        }
    }
}

#[test]
fn the_simulation_is_labelled_as_such() {
    let mut host = Host::new();
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("sin conexión real")));
    let _ = Duration::ZERO;
}

#[test]
fn five_thousand_messages_cost_what_a_screenful_costs() {
    let mut host = Host::new();
    let channel = host.open_channel("rendimiento");
    while !host.store.timeline(channel).unwrap().reached_start {
        let oldest = host.store.timeline(channel).unwrap().oldest().unwrap();
        host.page(channel, Some(oldest));
    }
    assert_eq!(host.store.timeline(channel).unwrap().len(), 5_000);
    let list = timeline_id(channel);
    for (place, index) in [("end", None), ("middle", Some(2_500)), ("start", Some(0))] {
        if let Some(index) = index {
            host.tree.scroll_to_index(&list, index);
        }
        let started = Instant::now();
        host.layout_only();
        let took = started.elapsed();
        let rows = host.tree.visible_rows(&list).unwrap();
        println!("{place}: rows {rows:?} built, view and layout {took:?}");
        assert!(
            rows.len() < 40,
            "{place}: only what is in view is built, not {rows:?}"
        );
        if let Some(index) = index {
            assert!(rows.contains(&index), "{place}: row {index} is in view");
        }
    }
}

#[test]
fn a_refused_message_says_so_and_can_be_sent_again_or_taken_back() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("¿llega?");
    host.key(Key::Enter, false, false);
    let shown = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    let failure = rune_provider::Failure::Send {
        channel: general,
        nonce: host.nonce,
    };
    host.event(Event::Failed {
        failure,
        reason: "rechazado".into(),
        ambiguous: false,
    });
    assert!(
        host.ui.notices.is_empty(),
        "the row says it; no notice in the corner"
    );
    let painted = host.painted();
    assert!(
        painted
            .iter()
            .any(|text| text.contains("No se envió: rechazado")),
        "{painted:?}"
    );
    // Both ways out stay inside the timeline, whatever the reason says.
    let frame = host.frame();
    let discard = frame
        .regions
        .iter()
        .find(|region| region.id == format!("msg:{shown}:discard"))
        .unwrap();
    assert!(
        discard.rect.x + discard.rect.width <= WINDOW.width - 16.0,
        "{:?}",
        discard.rect
    );
    host.click(&format!("msg:{shown}:retry"));
    assert!(
        matches!(host.sent.last(), Some(Command::SendMessage { content, .. }) if content == "¿llega?")
    );
    let retried = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .clone();
    assert_eq!(retried.delivery, rune_model::Delivery::Sending);
    // Refused again, and taken back: the text returns to the empty composer.
    let failure = rune_provider::Failure::Send {
        channel: general,
        nonce: host.nonce,
    };
    host.event(Event::Failed {
        failure,
        reason: "otra vez".into(),
        ambiguous: false,
    });
    host.click(&format!("msg:{}:discard", retried.id));
    host.frame();
    assert_eq!(host.tree.input_value(&editor_id(general)), Some("¿llega?"));
    assert!(host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .all(|message| !message.delivery.is_local()));
}

#[test]
fn deafening_mutes_and_hearing_again_restores_the_microphone_as_it_was() {
    let mut host = Host::new();
    host.click("me:deafen");
    assert!(host.ui.deafened && host.ui.muted);
    host.click("me:deafen");
    assert!(
        !host.ui.deafened && !host.ui.muted,
        "it was not muted before"
    );
    host.click("me:mute");
    host.click("me:deafen");
    host.click("me:deafen");
    assert!(host.ui.muted, "it was muted before, so it stays muted");
    // As in Discord: the microphone, while deafened, brings back both.
    host.click("me:deafen");
    host.click("me:mute");
    assert!(!host.ui.deafened && !host.ui.muted);
    host.click("me:mute");
    host.click("me:deafen");
    host.click("me:mute");
    assert!(
        !host.ui.deafened && !host.ui.muted,
        "even if it was muted before"
    );
    // The headphones alone only flip the sound; the microphone follows.
    host.click("me:deafen");
    host.click("me:deafen");
    assert!(!host.ui.deafened && !host.ui.muted);
}

#[test]
fn joining_a_voice_channel_shows_the_call_at_once_and_asks_with_the_microphone_state() {
    let mut host = Host::new();
    let channel = host
        .store
        .servers()
        .iter()
        .flat_map(|server| host.store.channel_tree(server.id))
        .flat_map(|(_, channels)| channels)
        .find(|channel| channel.kind == ChannelKind::Voice)
        .map(|channel| channel.id)
        .unwrap();
    host.click("me:mute");
    host.click(&format!("voice:{channel}"));
    assert_eq!(
        host.store.call().unwrap().phase,
        rune_model::CallPhase::Joining
    );
    assert!(matches!(
        host.sent.last(),
        Some(Command::JoinVoice {
            muted: true,
            deafened: false,
            ..
        })
    ));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Conectando a la voz")));
    host.event(Event::CallPhase {
        channel,
        phase: rune_model::CallPhase::Connected,
        quality: None,
    });
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Voz conectada (simulada)")));
    host.click("vz:leave");
    assert!(host.store.call().is_none());
    assert!(matches!(host.sent.last(), Some(Command::LeaveVoice)));
}

#[test]
fn dirty_drafts_are_retained_until_the_matching_revision_is_accepted() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("borrador");
    let kept: Vec<_> = host.ui.dirty_drafts().collect();
    assert_eq!(kept.len(), 1);
    let (channel, revision, text) = kept[0];
    assert_eq!((channel, text), (general, Some("borrador")));
    assert_eq!(host.ui.dirty_drafts().count(), 1, "Busy did not accept it");
    host.ui.acknowledge_draft(channel, revision);
    assert_eq!(host.ui.dirty_drafts().count(), 0);
    host.key(Key::Enter, false, false);
    assert_eq!(
        host.ui
            .dirty_drafts()
            .map(|(channel, _, text)| (channel, text))
            .collect::<Vec<_>>(),
        vec![(general, None)],
        "sent: the kept draft goes"
    );
}

#[test]
fn an_old_draft_acknowledgement_cannot_clear_a_new_edit_or_deletion() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("primero");
    let (_, first, _) = host.ui.dirty_drafts().next().unwrap();
    host.type_text(" segundo");
    host.ui.acknowledge_draft(general, first);
    let (_, second, text) = host.ui.dirty_drafts().next().unwrap();
    assert_eq!(text, Some("primero segundo"));
    assert_ne!(first, second);
    host.key(Key::Enter, false, false);
    host.ui.acknowledge_draft(general, second);
    let (_, deletion, text) = host.ui.dirty_drafts().next().unwrap();
    assert_eq!(text, None);
    assert_ne!(second, deletion);
    host.ui.acknowledge_draft(general, deletion);
    assert_eq!(host.ui.dirty_drafts().count(), 0);
}

#[test]
fn the_member_list_builds_only_what_is_in_view() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let total: usize = host
        .store
        .member_groups(server)
        .iter()
        .map(|group| group.members.len() + 1)
        .sum();
    host.ui.members_open = true;
    host.frame();
    let shown = host
        .tree
        .visible_rows("members")
        .expect("the list is laid out");
    assert!(shown.len() < total, "{shown:?} of {total} rows");
    // What someone is doing shows under their name.
    let doing: Vec<String> = host
        .store
        .member_groups(server)
        .iter()
        .flat_map(|group| group.members.iter())
        .filter_map(|user| host.store.presence(*user).activity().cloned())
        .map(|activity| rune_components::look::activity_line(&activity))
        .collect();
    let painted = host.painted();
    assert!(
        doing.iter().any(|line| painted.contains(line)),
        "{doing:?} not in {painted:?}"
    );
}

#[test]
fn a_provider_member_list_shows_its_counts_holes_and_follows_the_view() {
    use rune_model::{ListGroup, ListRow, MemberList};
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    host.ui.members_open = true;
    host.frame();
    let watched = host.sent.iter().rev().find_map(|command| match command {
        Command::WatchMemberList {
            server,
            channel,
            first,
            last,
        } => Some((*server, *channel, *first, *last)),
        _ => None,
    });
    assert!(
        matches!(watched, Some((s, c, 0, last)) if s == server && c == general && last > 0),
        "{watched:?}"
    );
    let sent = host.sent.len();
    host.frame();
    assert_eq!(host.sent.len(), sent, "the same view asks nothing again");

    let role = host
        .store
        .server(server)
        .unwrap()
        .roles
        .iter()
        .find(|role| role.hoisted)
        .unwrap()
        .clone();
    let ada = host.store.member_groups(server)[0].members[0];
    host.event(Event::MemberListChanged(MemberList {
        server,
        channel: general,
        groups: vec![(ListGroup::Role(role.id), 1), (ListGroup::Online, 3000)],
        rows: vec![(1, ListRow::Member(ada))],
    }));
    let painted = host.painted();
    assert!(
        painted.contains(&format!("{} — 1", role.name)),
        "{painted:?}"
    );
    assert!(
        painted.contains(&"En línea — 3000".to_owned()),
        "{painted:?}"
    );
    assert!(painted.contains(&host.store.display_name(ada, Some(server))));
    let shown = host.tree.visible_rows("members").unwrap();
    assert!(shown.len() < 100, "only what is in view: {shown:?}");

    host.ui.members_open = false;
    host.frame();
    assert_eq!(host.sent.last(), Some(&Command::UnwatchMemberList));
}

mod alerts;
mod attachments;
mod auth;
mod cards;
mod challenges;
mod channel_list;
mod channel_settings;
mod composer_selection;
mod images;
mod person_menu;
// Commands and interactions (P3).
mod interactions;
// Presence, identity and expressions (P5).
mod identity;
mod invitation_cards;
mod invitations;
mod layout;
mod media;
mod messages;
// Message actions and moderation (P4).
mod message_actions;
mod navigation;
mod notices;
mod players;
mod profile;
mod profile_card;
mod providers;
mod relationships;
mod remote;
mod rich;
mod search;
mod server_menu;
mod server_settings;
mod stage;
mod threads;
mod update_settings;
mod uploads;
mod user_settings;
mod voice;
mod voice_messages;
// Desktop, language and local voice (P6).
mod desktop;
// Interface languages (i18n).
mod english;
// Private calls.
mod private_calls;
// The screen until the session is loaded (pulido-ui-real).
mod splash;
// The connection's pill over the conversation.
mod connection;
// Message content (contenido-de-mensajes).
mod message_content;
// Rail and notifications.
mod rail;
// Reading settings and pictures on disk (imagenes-y-lectura).
mod reading;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Editable shortcuts (atajos-y-navegacion).
mod shortcuts;
// Own themes, main color and transparency (temas-propios).
mod themes;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;
// The window's own title bar (barra-e-icono).
mod title_bar;
// The image and video viewer (visor-de-medios).
mod media_viewer;
