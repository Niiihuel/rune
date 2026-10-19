//! The page that edits a community's onboarding (2026-10-18): whether it is
//! on, what counts toward Discord's requirements, the channels everyone
//! gets, and the questions, whose options give roles and open channels.
//! The settings' rules hold: the draft is a copy of what the provider sent
//! and only differs while something is changed here, the bar of unsaved
//! changes saves it, and the permit is asked of `Store::server_abilities`
//! again on save. Its actions are `ss:onb:…`.

use std::time::Instant;

use rune_i18n::{t, tr};
use rune_model::{
    ChannelId, ChoiceId, Onboarding, OnboardingMode, Prompt, PromptChoice, PromptEmoji, PromptId,
    PromptStyle, RoleId,
};
use rune_provider::{Command, Failure, RequestId};
use rune_state::{Applied, Store};

use super::{Page, ServerSettings};
use crate::{Intent, Ui};

/// What the page keeps while the settings are open.
#[derive(Debug, Default)]
pub struct OnboardingPage {
    /// The read under way, and whether the last one failed.
    loading: Option<RequestId>,
    pub failed: bool,
    /// The onboarding as edited here; `None` until it arrives.
    pub draft: Option<Onboarding>,
    /// The save under way.
    saving: Option<RequestId>,
    /// The option open to its roles and channels.
    pub open: Option<ChoiceId>,
    /// The mode's dropdown is open.
    pub picking_mode: bool,
}

impl OnboardingPage {
    pub(super) fn saving(&self) -> bool {
        self.saving.is_some()
    }

    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    /// Close what floats on the page; false when nothing did.
    pub(super) fn close(&mut self) -> bool {
        std::mem::take(&mut self.picking_mode)
    }
}

/// `onboarding` as it is compared: without what the account chose.
fn edited(onboarding: &Onboarding) -> Onboarding {
    Onboarding {
        chosen: Vec::new(),
        ..onboarding.clone()
    }
}

/// Whether the draft differs from what the provider has.
pub(super) fn changed(settings: &ServerSettings, store: &Store) -> bool {
    let Some(draft) = &settings.onboarding.draft else {
        return false;
    };
    store
        .onboarding(settings.server)
        .is_some_and(|known| edited(&known.cloned().unwrap_or_default()) != edited(draft))
}

/// Ask for the onboarding when its page opens and the account may edit it.
pub(super) fn load(
    settings: &mut ServerSettings,
    page: Page,
    ui: &mut Ui,
    store: &Store,
) -> Vec<Command> {
    let server = settings.server;
    let allowed = store
        .server_abilities(server)
        .is_some_and(|abilities| abilities.manage_onboarding.allowed());
    if page != Page::Onboarding || !allowed || settings.onboarding.loading.is_some() {
        return Vec::new();
    }
    let request = ui.fresh_request();
    settings.onboarding.loading = Some(request);
    settings.onboarding.failed = false;
    vec![Command::LoadOnboarding { request, server }]
}

/// Go back to what the provider has.
pub(super) fn reset(settings: &mut ServerSettings, store: &Store) {
    if settings.onboarding.draft.is_some() {
        settings.onboarding.draft = store
            .onboarding(settings.server)
            .map(|known| known.cloned().unwrap_or_default());
    }
}

/// Why the draft cannot be saved as it is, if it cannot.
pub(super) fn problem(settings: &ServerSettings, store: &Store) -> Option<String> {
    if !changed(settings, store) {
        return None;
    }
    let draft = settings.onboarding.draft.as_ref()?;
    let server = store.server(settings.server)?;
    let problem = draft.problem(|id| {
        store
            .channel(id)
            .is_some_and(|channel| rune_model::onboarding::everyone_writes(server, channel))
    });
    // What is typed as an option's emoji is one Unicode emoji; a server's
    // own (with its id) came from the provider.
    let lone = |name: &str| rune_components::twemoji::emoji_len(name) == Some(name.len());
    let typed = draft
        .prompts
        .iter()
        .flat_map(|prompt| &prompt.options)
        .filter_map(|option| option.emoji.as_ref())
        .find(|emoji| emoji.id.is_none() && !lone(&emoji.name));
    problem.or_else(|| {
        typed.map(|emoji| {
            tr!(
                "«{emoji}» no es un solo emoji: escribí uno, como 📷.",
                emoji = emoji.name
            )
        })
    })
}

/// Send the draft, if it changed and the account may; false when the
/// permission was lost.
pub(super) fn save(
    settings: &mut ServerSettings,
    store: &Store,
    intents: &mut Vec<Intent>,
    ui: &mut Ui,
) -> bool {
    if !changed(settings, store) || settings.onboarding.saving.is_some() {
        return true;
    }
    let allowed = store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.manage_onboarding.allowed());
    let Some(draft) = settings.onboarding.draft.clone().filter(|_| allowed) else {
        return false;
    };
    let request = ui.fresh_request();
    settings.onboarding.saving = Some(request);
    intents.push(Intent::Command(Command::SaveOnboarding {
        request,
        server: settings.server,
        onboarding: draft,
    }));
    true
}

/// A text of the page was changed: true when it was one of its fields.
pub(super) fn typed(settings: &mut ServerSettings, id: &str, value: &str) -> bool {
    let Some(rest) = id.strip_prefix("ss:onb:") else {
        return false;
    };
    // While a save is on its way nothing changes: its answer replaces the draft.
    let Some(draft) = settings
        .onboarding
        .draft
        .as_mut()
        .filter(|_| settings.onboarding.saving.is_none())
    else {
        return true;
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let number = |part: &str| part.parse::<u64>().ok();
    match parts.as_slice() {
        ["prompt", prompt, "title"] => {
            if let Some(found) = number(prompt).and_then(|id| prompt_mut(draft, PromptId(id))) {
                found.title = value.to_owned();
            }
        }
        ["option", option, part] => {
            if let Some(found) = number(option).and_then(|id| option_mut(draft, ChoiceId(id))) {
                match *part {
                    "title" => found.title = value.to_owned(),
                    "description" => {
                        found.description =
                            Some(value.to_owned()).filter(|text| !text.trim().is_empty())
                    }
                    "emoji" => {
                        let emoji = value.trim();
                        // A server's emoji stays while its name is not changed.
                        if found.emoji.as_ref().is_none_or(|known| known.name != emoji) {
                            found.emoji = (!emoji.is_empty()).then(|| PromptEmoji {
                                name: emoji.to_owned(),
                                id: None,
                                animated: false,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    true
}

fn prompt_mut(draft: &mut Onboarding, id: PromptId) -> Option<&mut Prompt> {
    draft.prompts.iter_mut().find(|prompt| prompt.id == id)
}

fn option_mut(draft: &mut Onboarding, id: ChoiceId) -> Option<&mut PromptChoice> {
    draft
        .prompts
        .iter_mut()
        .flat_map(|prompt| prompt.options.iter_mut())
        .find(|option| option.id == id)
}

fn toggle<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if let Some(index) = list.iter().position(|known| *known == item) {
        list.remove(index);
    } else {
        list.push(item);
    }
}

/// An empty option, named as the official client names new ones.
fn new_option(id: u64) -> PromptChoice {
    PromptChoice {
        id: ChoiceId(id),
        title: String::new(),
        description: None,
        emoji: None,
        roles: Vec::new(),
        channels: Vec::new(),
    }
}

/// An `ss:onb:` click: true when it was the page's.
pub(super) fn clicked(
    settings: &mut ServerSettings,
    store: &Store,
    parts: &[&str],
    intents: &mut Vec<Intent>,
) -> bool {
    let ["onb", rest @ ..] = parts else {
        return false;
    };
    let can = store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.manage_onboarding.allowed())
        && settings.onboarding.saving.is_none();
    let page = &mut settings.onboarding;
    if rest == ["mode-pick"] {
        page.picking_mode = !page.picking_mode;
        return true;
    }
    if rest == ["mode-dismiss"] {
        page.picking_mode = false;
        return true;
    }
    let (Some(draft), true) = (page.draft.as_mut(), can) else {
        return true;
    };
    let number = |part: &str| part.parse::<u64>().ok();
    // New ids follow the wall clock, as the official client's do.
    let now_ms = store.wall().0.max(0) as u64;
    match rest {
        ["enabled"] => draft.enabled = !draft.enabled,
        ["mode", "default"] => {
            draft.mode = OnboardingMode::Default;
            page.picking_mode = false;
        }
        ["mode", "advanced"] => {
            draft.mode = OnboardingMode::Advanced;
            page.picking_mode = false;
        }
        ["default", channel] => {
            if let Some(channel) = number(channel) {
                toggle(&mut draft.default_channels, ChannelId(channel));
            }
        }
        ["prompt-add"] if draft.prompts.len() < rune_model::onboarding::MAX_PROMPTS => {
            let id = draft.fresh_id(now_ms);
            draft.prompts.push(Prompt {
                id: PromptId(id),
                title: String::new(),
                style: PromptStyle::Choices,
                single: false,
                required: false,
                in_onboarding: true,
                options: Vec::new(),
            });
            let option = draft.fresh_id(now_ms);
            if let Some(prompt) = draft.prompts.last_mut() {
                prompt.options.push(new_option(option));
            }
            page.open = Some(ChoiceId(option));
            intents.push(Intent::Focus(format!("ss:onb:prompt:{id}:title")));
        }
        ["prompt", prompt, what] => {
            let Some(id) = number(prompt).map(PromptId) else {
                return true;
            };
            if *what == "remove" {
                draft.prompts.retain(|found| found.id != id);
                return true;
            }
            if *what == "option-add" {
                let option = draft.fresh_id(now_ms);
                if let Some(found) = prompt_mut(draft, id).filter(|found| {
                    found.options.len() < rune_model::onboarding::MAX_PROMPT_OPTIONS
                }) {
                    found.options.push(new_option(option));
                    page.open = Some(ChoiceId(option));
                    intents.push(Intent::Focus(format!("ss:onb:option:{option}:title")));
                }
                return true;
            }
            if let Some(found) = prompt_mut(draft, id) {
                match *what {
                    "single" => found.single = !found.single,
                    "required" => found.required = !found.required,
                    "in-onboarding" => found.in_onboarding = !found.in_onboarding,
                    "dropdown" => {
                        found.style = if found.style == PromptStyle::Dropdown {
                            PromptStyle::Choices
                        } else {
                            PromptStyle::Dropdown
                        }
                    }
                    _ => {}
                }
            }
        }
        ["option", option, "open"] => {
            if let Some(id) = number(option).map(ChoiceId) {
                page.open = (page.open != Some(id)).then_some(id);
            }
        }
        ["option", option, "remove"] => {
            if let Some(id) = number(option).map(ChoiceId) {
                for prompt in &mut draft.prompts {
                    prompt.options.retain(|found| found.id != id);
                }
            }
        }
        ["option", option, "role", role] => {
            if let (Some(found), Some(role)) = (
                number(option).and_then(|id| option_mut(draft, ChoiceId(id))),
                number(role),
            ) {
                toggle(&mut found.roles, RoleId(role));
            }
        }
        ["option", option, "channel", channel] => {
            if let (Some(found), Some(channel)) = (
                number(option).and_then(|id| option_mut(draft, ChoiceId(id))),
                number(channel),
            ) {
                toggle(&mut found.channels, ChannelId(channel));
            }
        }
        _ => {}
    }
    true
}

/// Follow what the provider answered: the read fills the draft, a save
/// replaces it with what was saved, and a change from elsewhere shows
/// while nothing is edited here.
pub(super) fn on_applied(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Command> {
    let server = settings.server;
    let page = &mut settings.onboarding;
    match applied {
        Applied::Onboarding {
            request,
            server: of,
        } if *of == server => {
            let ours = page.loading == Some(*request) || page.saving == Some(*request);
            if page.loading == Some(*request) {
                page.loading = None;
            }
            if page.saving == Some(*request) {
                page.saving = None;
            }
            let known = store
                .onboarding(server)
                .map(|known| known.cloned().unwrap_or_default());
            let untouched = page.draft.is_none() || ours;
            if untouched {
                page.draft = known;
            }
        }
        Applied::Failed {
            failure: Failure::Onboarding { request, .. },
            ..
        } if page.loading == Some(*request) => {
            page.loading = None;
            page.failed = true;
        }
        Applied::Failed {
            failure: Failure::OnboardingEdit { request, .. },
            reason,
            ambiguous,
        } if page.saving == Some(*request) => {
            page.saving = None;
            let title = if *ambiguous {
                t("Puede que la incorporación se haya guardado")
            } else {
                t("No se guardó la incorporación")
            };
            ui.notify(title, Some(reason.clone()), true, now);
        }
        Applied::Ready => {
            page.loading = None;
            page.saving = None;
            page.draft = None;
            return load(settings, settings.page, ui, store);
        }
        _ => {}
    }
    Vec::new()
}
