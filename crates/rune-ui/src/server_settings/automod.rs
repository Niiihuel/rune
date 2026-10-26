//! The page of a server's AutoMod rules (automod): the list, each with its
//! switch, and the editor of one, new or saved, in place of the list. The
//! editor keeps the words and patterns as typed and reads them on save; a
//! rule the service would refuse is not sent, and the permit is asked of
//! `Store::server_abilities` again then. Nothing changes before the
//! provider says so. Its actions are `ss:am:…`.

use std::time::Instant;

use rune_model::automod::{patterns, words};
use rune_model::{
    AutoModAction, AutoModKind, AutoModPreset, AutoModRule, AutoModRuleId, AutoModTrigger,
    ChannelId, ChannelKind, RoleId,
};
use rune_provider::{Command, Failure, RequestId};
use rune_state::{Applied, Store};

use super::{Page, ServerSettings};
use crate::{Intent, Ui};

/// The editor's fields, whose text limn keeps by id.
pub(crate) const NAME: &str = "ss:am:name";
pub(crate) const KEYWORDS: &str = "ss:am:keywords";
pub(crate) const PATTERNS: &str = "ss:am:patterns";
pub(crate) const ALLOWED: &str = "ss:am:allowed";
pub(crate) const LIMIT: &str = "ss:am:limit";
pub(crate) const NOTICE: &str = "ss:am:notice";

/// What each kind is called in the page's ids.
pub(crate) fn kind_key(kind: AutoModKind) -> &'static str {
    match kind {
        AutoModKind::Keywords => "keywords",
        AutoModKind::Spam => "spam",
        AutoModKind::Presets => "presets",
        AutoModKind::Mentions => "mentions",
        AutoModKind::Profile => "profile",
    }
}

pub(crate) fn preset_key(preset: AutoModPreset) -> &'static str {
    match preset {
        AutoModPreset::Profanity => "profanity",
        AutoModPreset::Sexual => "sexual",
        AutoModPreset::Slurs => "slurs",
    }
}

/// A dropdown of the editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Picker {
    Alert,
    Timeout,
}

/// A rule as the editor holds it: what is chosen in `rule`, and what is
/// typed as it was typed.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleDraft {
    pub rule: AutoModRule,
    pub name: String,
    pub keywords: String,
    pub patterns: String,
    pub allowed: String,
    pub limit: String,
    pub notice: String,
}

impl RuleDraft {
    fn open(rule: AutoModRule) -> Self {
        let (keywords, found_patterns, allowed) = match &rule.trigger {
            AutoModTrigger::Keywords(filter) | AutoModTrigger::Profile(filter) => (
                filter.keywords.join(", "),
                filter.patterns.join("\n"),
                filter.allowed.join(", "),
            ),
            AutoModTrigger::Presets { allowed, .. } => {
                (String::new(), String::new(), allowed.join(", "))
            }
            _ => Default::default(),
        };
        let limit = match rule.trigger {
            AutoModTrigger::Mentions { limit, .. } => limit.to_string(),
            _ => String::new(),
        };
        let notice = rule
            .actions
            .iter()
            .find_map(|action| match action {
                AutoModAction::Block { notice } => notice.clone(),
                _ => None,
            })
            .unwrap_or_default();
        Self {
            name: rule.name.clone(),
            keywords,
            patterns: found_patterns,
            allowed,
            limit,
            notice,
            rule,
        }
    }

    /// The rule as edited, with what is typed read.
    pub fn built(&self) -> AutoModRule {
        let mut rule = self.rule.clone();
        rule.name = self.name.trim().to_owned();
        match &mut rule.trigger {
            AutoModTrigger::Keywords(filter) | AutoModTrigger::Profile(filter) => {
                filter.keywords = words(&self.keywords);
                filter.patterns = patterns(&self.patterns);
                filter.allowed = words(&self.allowed);
            }
            AutoModTrigger::Presets { allowed, .. } => *allowed = words(&self.allowed),
            // Not a number is no limit: the rule's check says the range.
            AutoModTrigger::Mentions { limit, .. } => {
                *limit = self.limit.trim().parse().unwrap_or(0)
            }
            AutoModTrigger::Spam | AutoModTrigger::Other(_) => {}
        }
        let notice = Some(self.notice.trim().to_owned()).filter(|notice| !notice.is_empty());
        for action in &mut rule.actions {
            if let AutoModAction::Block { notice: kept } = action {
                *kept = notice.clone();
            }
        }
        rule
    }

    /// Whether it does `action`'s kind of thing.
    pub fn does(&self, action: &str) -> bool {
        self.rule.actions.iter().any(|known| key(known) == action)
    }
}

/// What an action is called in the page's ids.
fn key(action: &AutoModAction) -> &'static str {
    match action {
        AutoModAction::Block { .. } => "block",
        AutoModAction::Alert { .. } => "alert",
        AutoModAction::Timeout { .. } => "timeout",
        AutoModAction::Quarantine => "quarantine",
        AutoModAction::Other(_) => "other",
    }
}

/// What the page keeps while the settings are open.
#[derive(Debug, Default)]
pub struct AutoModPage {
    /// The read under way, and whether the last one failed.
    loading: Option<RequestId>,
    pub failed: bool,
    /// The rule open in the editor, in place of the list.
    pub editing: Option<RuleDraft>,
    /// The save under way.
    saving: Option<RequestId>,
    /// The rule being deleted, while that dialog is open.
    pub deleting: Option<AutoModRuleId>,
    /// The editor's dropdown that is open.
    pub picking: Option<Picker>,
}

impl AutoModPage {
    pub fn saving(&self) -> bool {
        self.saving.is_some()
    }

    /// Close what floats on the page, then the editor; false when nothing
    /// was open.
    pub(super) fn close(&mut self) -> bool {
        self.picking.take().is_some()
            || self.deleting.take().is_some()
            || (self.saving.is_none() && self.editing.take().is_some())
    }
}

fn allowed(settings: &ServerSettings, store: &Store) -> bool {
    store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.manage_automod.allowed())
}

/// Ask for the rules when the page opens and the account may manage them.
pub(super) fn load(
    settings: &mut ServerSettings,
    page: Page,
    ui: &mut Ui,
    store: &Store,
) -> Vec<Command> {
    if page != Page::AutoMod || !allowed(settings, store) || settings.automod.loading.is_some() {
        return Vec::new();
    }
    let request = ui.fresh_request();
    settings.automod.loading = Some(request);
    settings.automod.failed = false;
    vec![Command::LoadAutoModRules {
        request,
        server: settings.server,
    }]
}

/// Why the editor's rule cannot be saved as it is, if it cannot.
pub(crate) fn problem(
    draft: &RuleDraft,
    store: &Store,
    settings: &ServerSettings,
) -> Option<String> {
    let rule = draft.built();
    rule.problem().or_else(|| {
        // What it names must still be in the server.
        let server = store.server(settings.server)?;
        let gone_channel = |channel: &ChannelId| {
            store
                .channel(*channel)
                .is_none_or(|found| found.server != Some(server.id))
        };
        let gone_role = |role: &RoleId| server.roles.iter().all(|known| known.id != *role);
        let alert_gone = rule.actions.iter().any(
            |action| matches!(action, AutoModAction::Alert { channel } if gone_channel(channel)),
        );
        (alert_gone
            || rule.exempt_channels.iter().any(gone_channel)
            || rule.exempt_roles.iter().any(gone_role))
        .then(|| rune_i18n::t("Un canal o un rol de la regla ya no existe").to_owned())
    })
}

/// A text of the page was changed: true when it was one of its fields.
pub(super) fn typed(settings: &mut ServerSettings, id: &str, value: &str) -> bool {
    if !id.starts_with("ss:am:") {
        return false;
    }
    let page = &mut settings.automod;
    // While a save is on its way nothing changes: its answer closes it.
    let Some(draft) = page.editing.as_mut().filter(|_| page.saving.is_none()) else {
        return true;
    };
    let field = match id {
        NAME => &mut draft.name,
        KEYWORDS => &mut draft.keywords,
        PATTERNS => &mut draft.patterns,
        ALLOWED => &mut draft.allowed,
        LIMIT => &mut draft.limit,
        NOTICE => &mut draft.notice,
        _ => return true,
    };
    *field = value.to_owned();
    true
}

fn toggle<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if let Some(index) = list.iter().position(|known| *known == item) {
        list.remove(index);
    } else {
        list.push(item);
    }
}

/// The first text channel of the server, where an alert goes until another
/// is chosen.
fn first_text(store: &Store, settings: &ServerSettings) -> ChannelId {
    store
        .channel_tree(settings.server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find(|channel| channel.kind == ChannelKind::Text)
        .map_or(ChannelId(0), |channel| channel.id)
}

/// The fields of the editor, set to what `draft` holds.
fn fill(draft: &RuleDraft, intents: &mut Vec<Intent>) {
    for (id, value) in [
        (NAME, &draft.name),
        (KEYWORDS, &draft.keywords),
        (PATTERNS, &draft.patterns),
        (ALLOWED, &draft.allowed),
        (LIMIT, &draft.limit),
        (NOTICE, &draft.notice),
    ] {
        intents.push(Intent::SetInput {
            id: id.to_owned(),
            value: value.clone(),
        });
    }
}

/// An `ss:am:` click: true when it was the page's.
pub(super) fn clicked(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    parts: &[&str],
    now: Instant,
    intents: &mut Vec<Intent>,
) -> bool {
    let ["am", rest @ ..] = parts else {
        return false;
    };
    let can = allowed(settings, store);
    let server = settings.server;
    match rest {
        ["reload"] => {
            settings.automod.failed = false;
            for command in load(settings, Page::AutoMod, ui, store) {
                intents.push(Intent::Command(command));
            }
            return true;
        }
        ["save"] => {
            let page = &settings.automod;
            let Some(draft) = page.editing.as_ref().filter(|_| page.saving.is_none()) else {
                return true;
            };
            if !can {
                ui.notify(
                    rune_i18n::t("Tus permisos cambiaron"),
                    Some(rune_i18n::t("Ya no puedes administrar AutoMod en este servidor.").into()),
                    true,
                    now,
                );
                return true;
            }
            if problem(draft, store, settings).is_some() {
                return true;
            }
            let rule = draft.built();
            let request = ui.fresh_request();
            settings.automod.saving = Some(request);
            settings.automod.picking = None;
            intents.push(Intent::Command(Command::SaveAutoModRule {
                request,
                server,
                rule,
            }));
            return true;
        }
        _ => {}
    }
    let number = |part: &str| part.parse::<u64>().ok();
    let rules = store.automod_rules(server).unwrap_or_default();
    let alert_default = first_text(store, settings);
    let page = &mut settings.automod;
    match rest {
        ["new", kind] if can && page.editing.is_none() => {
            let kind = AutoModKind::ALL
                .into_iter()
                .find(|known| kind_key(*known) == *kind);
            if let Some(kind) = kind {
                let count = rules
                    .iter()
                    .filter(|rule| rule.trigger.kind() == Some(kind))
                    .count();
                if count < kind.max_rules() {
                    let draft = RuleDraft::open(AutoModRule::fresh(kind));
                    fill(&draft, intents);
                    page.editing = Some(draft);
                    intents.push(Intent::Focus(NAME.into()));
                }
            }
        }
        ["rule", id, what] => {
            let Some(id) = number(id).map(AutoModRuleId) else {
                return true;
            };
            let Some(rule) = rules.iter().find(|rule| rule.id == Some(id)) else {
                return true;
            };
            match *what {
                "enabled" if can => intents.push(Intent::Command(Command::SetAutoModRuleEnabled {
                    server,
                    rule: id,
                    enabled: !rule.enabled,
                })),
                "edit" if can && rule.editable() && page.editing.is_none() => {
                    let draft = RuleDraft::open(rule.clone());
                    fill(&draft, intents);
                    page.editing = Some(draft);
                }
                "delete" if can => page.deleting = Some(id),
                _ => {}
            }
        }
        ["delete", "cancel"] => page.deleting = None,
        ["delete", "confirm"] => {
            // The permission is asked again: it may have gone meanwhile.
            if let Some(rule) = page.deleting.take().filter(|_| can) {
                intents.push(Intent::Command(Command::DeleteAutoModRule { server, rule }));
            }
        }
        ["cancel"] if page.saving.is_none() => {
            page.editing = None;
            page.picking = None;
        }
        ["alert-pick"] => {
            page.picking = (page.picking != Some(Picker::Alert)).then_some(Picker::Alert)
        }
        ["timeout-pick"] => {
            page.picking = (page.picking != Some(Picker::Timeout)).then_some(Picker::Timeout)
        }
        ["pick-dismiss"] => page.picking = None,
        edit => {
            // What follows changes the editor's rule, while nothing is on its way.
            let Some(draft) = page.editing.as_mut().filter(|_| page.saving.is_none()) else {
                return true;
            };
            let rule = &mut draft.rule;
            match edit {
                ["enabled"] => rule.enabled = !rule.enabled,
                ["raids"] => {
                    if let AutoModTrigger::Mentions { raids, .. } = &mut rule.trigger {
                        *raids = !*raids;
                    }
                }
                ["preset", which] => {
                    let preset = AutoModPreset::ALL
                        .into_iter()
                        .find(|known| preset_key(*known) == *which);
                    if let (AutoModTrigger::Presets { lists, .. }, Some(preset)) =
                        (&mut rule.trigger, preset)
                    {
                        toggle(lists, preset);
                    }
                }
                ["action", which] => {
                    let present = rule.actions.iter().position(|action| key(action) == *which);
                    match present {
                        Some(index) => {
                            rule.actions.remove(index);
                        }
                        None => rule.actions.push(match *which {
                            "block" => AutoModAction::Block { notice: None },
                            "alert" => AutoModAction::Alert {
                                channel: alert_default,
                            },
                            "timeout" => AutoModAction::Timeout {
                                seconds: super::advanced::TIMEOUTS[0].0 as u32 * 60,
                            },
                            "quarantine" => AutoModAction::Quarantine,
                            _ => return true,
                        }),
                    }
                }
                ["alert", channel] => {
                    if let Some(channel) = number(channel).map(ChannelId) {
                        for action in &mut rule.actions {
                            if let AutoModAction::Alert { channel: kept } = action {
                                *kept = channel;
                            }
                        }
                    }
                    page.picking = None;
                }
                ["timeout", seconds] => {
                    if let Some(chosen) = number(seconds).and_then(|secs| u32::try_from(secs).ok())
                    {
                        for action in &mut rule.actions {
                            if let AutoModAction::Timeout { seconds } = action {
                                *seconds = chosen;
                            }
                        }
                    }
                    page.picking = None;
                }
                ["exempt-role", role] => {
                    if let Some(role) = number(role) {
                        toggle(&mut rule.exempt_roles, RoleId(role));
                    }
                }
                ["exempt-channel", channel] => {
                    if let Some(channel) = number(channel) {
                        toggle(&mut rule.exempt_channels, ChannelId(channel));
                    }
                }
                _ => {}
            }
        }
    }
    true
}

/// Follow what the provider answered: the read ends, a save closes the
/// editor, and a new session reads again.
pub(super) fn on_applied(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
) -> Vec<Command> {
    let server = settings.server;
    let page = &mut settings.automod;
    match applied {
        Applied::AutoModRules {
            request,
            server: of,
        } if *of == server && page.loading == Some(*request) => {
            page.loading = None;
        }
        Applied::AutoMod {
            server: of,
            request: Some(request),
        } if *of == server && page.saving == Some(*request) => {
            page.saving = None;
            page.editing = None;
            page.picking = None;
        }
        Applied::Failed {
            failure: Failure::AutoModRules { request, .. },
            ..
        } if page.loading == Some(*request) => {
            page.loading = None;
            page.failed = true;
        }
        // The notice says why; the editor stays as it was.
        Applied::Failed {
            failure: Failure::AutoModSave { request, .. },
            ..
        } if page.saving == Some(*request) => page.saving = None,
        Applied::AutoMod { server: of, .. } if *of == server => {
            // A rule deleted elsewhere closes its dialog.
            let gone = |id: AutoModRuleId| {
                store
                    .automod_rules(server)
                    .is_some_and(|rules| rules.iter().all(|rule| rule.id != Some(id)))
            };
            if page.deleting.is_some_and(gone) {
                page.deleting = None;
            }
        }
        Applied::Ready => {
            page.loading = None;
            page.saving = None;
            page.deleting = None;
            return load(settings, settings.page, ui, store);
        }
        _ => {}
    }
    Vec::new()
}
