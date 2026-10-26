//! A server's rules and onboarding on the personal session (2026-10-18):
//! what a new member answers and what the administration edits. The routes
//! and bodies are the official web client's (docs/DISCORD.md, «reglas y
//! onboarding»): `GET /guilds/{g}/member-verification`,
//! `PUT /guilds/{g}/requests/@me` with each field as it came and its
//! answer, `GET /guilds/{g}/onboarding`, `POST /guilds/{g}/onboarding-responses`
//! with what was seen, and `PUT /guilds/{g}/onboarding` (Discord's
//! documented edit). Serein uses the same routes; this module was written
//! from that evidence (rewritten on 2026-10-18 after a review found parts
//! too close to Serein's). Fixtures only: none was tried against Discord.

use std::collections::BTreeMap;

use rune_i18n::es;
use rune_model::onboarding::{
    MAX_OPTION_DESCRIPTION, MAX_OPTION_TITLE, MAX_PROMPTS_READ, MAX_PROMPT_OPTIONS,
    MAX_PROMPT_TITLE, MAX_RULES, MAX_RULES_CHOICES, MAX_RULES_FIELDS, MAX_RULE_CHARS,
};
use rune_model::{
    permissions, ChannelId, ChoiceId, EmojiId, JoinOutcome, Onboarding, OnboardingMode,
    Permissions, Prompt, PromptChoice, PromptEmoji, PromptId, PromptStyle, RoleId, RulesAnswer,
    RulesField, RulesFieldKind, Server, ServerId, ServerRules,
};
use rune_provider::{Command, Snapshot};
use serde::Serialize;
use serde_json::{json, Value};

use crate::profiles::text;
use crate::{wire, LoginError};

/// The most a form or an onboarding's answer may weigh: up to
/// `MAX_PROMPTS_READ` questions with their roles and channels.
pub(crate) const MAX_FORM: usize = 1024 * 1024;
/// The guild member flag of who finished the onboarding (documented).
pub(crate) const COMPLETED_ONBOARDING: u64 = 1 << 1;
/// The guild member flag of a guest, let into one voice channel
/// (documented `IS_GUEST`); the official client never onboards one.
pub(crate) const IS_GUEST: u64 = 1 << 4;
/// The features that say a server screens or asks (official client, REA).
pub(crate) const SCREENING: &str = "MEMBER_VERIFICATION_GATE_ENABLED";
pub(crate) const ONBOARDING: &str = "GUILD_ONBOARDING";

/// Whether `command` is about rules or onboarding.
pub(crate) fn is_onboarding(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadServerRules { .. }
            | Command::AnswerServerRules { .. }
            | Command::LoadOnboarding { .. }
            | Command::AnswerOnboarding { .. }
            | Command::SaveOnboarding { .. }
    )
}

/// Whether `command` only reads: a lost answer is never ambiguous.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadServerRules { .. } | Command::LoadOnboarding { .. }
    )
}

fn server_of(command: &Command) -> Option<ServerId> {
    match command {
        Command::LoadServerRules { server, .. }
        | Command::AnswerServerRules { server, .. }
        | Command::LoadOnboarding { server, .. }
        | Command::AnswerOnboarding { server, .. }
        | Command::SaveOnboarding { server, .. } => Some(*server),
        _ => None,
    }
}

/// Answers that do not go as they are: nothing left, and the reason is
/// RUNE's own, not something Discord said.
pub(crate) const INCOMPLETE: &str = es("Las respuestas no están completas: revísalas.");

/// Whether `command` may go to Discord, checked against what this session
/// knows; Discord stays the last word.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
    let server = server_of(command)
        .and_then(|id| snapshot.servers.iter().find(|known| known.id == id))
        .ok_or(LoginError::new(es("Ese servidor ya no está disponible.")))?;
    let mine = snapshot
        .members
        .iter()
        .find(|known| known.server == server.id && known.user == me);
    let owner = server.owner == Some(me);
    if mine.is_none() && !owner {
        return Err(LoginError::new(es(
            "Tu membresía en este servidor todavía no cargó.",
        )));
    }
    match command {
        Command::AnswerServerRules { rules, answers, .. } => {
            if !mine.is_some_and(|mine| mine.restrictions.pending) {
                return Err(LoginError::new(es(
                    "Ya aceptaste las reglas de este servidor.",
                )));
            }
            if !rules.bounded()
                || rules.problem(answers).is_some()
                || join_request(rules, answers).is_none()
            {
                return Err(LoginError::new(INCOMPLETE));
            }
        }
        Command::AnswerOnboarding {
            onboarding, chosen, ..
        } if !onboarding.asks() || onboarding.answer_problem(chosen).is_some() => {
            return Err(LoginError::new(INCOMPLETE));
        }
        Command::SaveOnboarding { onboarding, .. } => {
            let permissions = match mine {
                _ if owner => Permissions::ALL,
                Some(mine) => permissions::in_server(server, mine),
                None => Permissions::NONE,
            };
            if !permissions.contains(Permissions::MANAGE_SERVER | Permissions::MANAGE_ROLES) {
                return Err(LoginError::new(es(
                    "No tienes permiso para cambiar la incorporación.",
                )));
            }
            if !server.options.community {
                return Err(LoginError::new(es(
                    "Solo una comunidad tiene incorporación.",
                )));
            }
            edit_known(snapshot, server, onboarding)?;
        }
        _ => {}
    }
    Ok(())
}

/// What an edit names must be in the server: roles but `@everyone`, its
/// channels, and one channel everyone may write in when it is on.
fn edit_known(
    snapshot: &Snapshot,
    server: &Server,
    onboarding: &Onboarding,
) -> Result<(), LoginError> {
    let everyone = server.everyone_id();
    let channels: Vec<_> = snapshot
        .channels
        .iter()
        .filter(|channel| channel.server == Some(server.id))
        .collect();
    let known = |id: &ChannelId| channels.iter().any(|channel| channel.id == *id);
    let options = onboarding.prompts.iter().flat_map(|prompt| &prompt.options);
    let roles_known = options
        .clone()
        .flat_map(|option| &option.roles)
        .all(|role| *role != everyone && server.roles.iter().any(|known| known.id == *role));
    if !roles_known
        || !onboarding.default_channels.iter().all(known)
        || !options.flat_map(|option| &option.channels).all(known)
    {
        return Err(LoginError::new(es(
            "Un rol o un canal ya no está en el servidor.",
        )));
    }
    let writes = |id: ChannelId| {
        channels.iter().any(|channel| {
            channel.id == id && rune_model::onboarding::everyone_writes(server, channel)
        })
    };
    match onboarding.problem(writes) {
        Some(_) => Err(LoginError::new(es(
            "La incorporación no cumple lo que Discord pide; revísala.",
        ))),
        None => Ok(()),
    }
}

/// What an answer changes in the session before the Gateway says it: an
/// approved account takes part, and one that answered the questions is
/// done with them.
pub(crate) fn confirmed(snapshot: &mut Snapshot, updates: &[rune_provider::Event]) {
    let Some(me) = snapshot.me.as_ref().map(|me| me.id) else {
        return;
    };
    for update in updates {
        let (server, change): (ServerId, fn(&mut rune_model::MemberRestrictions)) = match update {
            rune_provider::Event::RulesAnswered {
                server,
                outcome: JoinOutcome::Approved,
                ..
            } => (*server, |restrictions| restrictions.pending = false),
            rune_provider::Event::OnboardingAnswered { server, .. } => {
                (*server, |restrictions| restrictions.onboarded = true)
            }
            _ => continue,
        };
        if let Some(member) = snapshot
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == me)
        {
            change(&mut member.restrictions);
        }
    }
}

/// The rows of `value`, at most `max`; anything else is none.
fn rows(value: &Value, max: usize) -> Result<&[Value], LoginError> {
    match value {
        Value::Null => Ok(&[]),
        Value::Array(rows) if rows.len() <= max => Ok(rows),
        _ => Err(wire::invalid()),
    }
}

fn ids(value: &Value, max: usize) -> Result<Vec<u64>, LoginError> {
    rows(value, max)?.iter().map(wire::id).collect()
}

/// `GET /guilds/{g}/member-verification`: a form without fields asks
/// nothing. What is shown is cleaned and cut; each field keeps the copy
/// it came as, which [`join_request`] gives back.
pub(crate) fn rules(value: &Value) -> Result<Option<ServerRules>, LoginError> {
    let mut fields = Vec::new();
    for field in rows(&value["form_fields"], MAX_RULES_FIELDS)? {
        let kind = match field["field_type"].as_str().ok_or_else(wire::invalid)? {
            "TERMS" => RulesFieldKind::Terms,
            "TEXT_INPUT" => RulesFieldKind::ShortAnswer,
            "PARAGRAPH" => RulesFieldKind::Paragraph,
            "MULTIPLE_CHOICE" => RulesFieldKind::MultipleChoice,
            other => RulesFieldKind::Other(other.chars().take(64).collect()),
        };
        fields.push(RulesField {
            kind,
            label: text(&field["label"], MAX_RULE_CHARS).unwrap_or_default(),
            description: text(&field["description"], MAX_RULE_CHARS),
            placeholder: text(&field["placeholder"], MAX_RULE_CHARS),
            rules: rows(&field["values"], MAX_RULES)?
                .iter()
                .filter_map(|rule| text(rule, MAX_RULE_CHARS))
                .collect(),
            // Each keeps its place: an answer is a choice's index.
            choices: rows(&field["choices"], MAX_RULES_CHOICES)?
                .iter()
                .map(|choice| text(choice, MAX_RULE_CHARS).unwrap_or_default())
                .collect(),
            required: field["required"].as_bool().unwrap_or(false),
            echo: Some(field.to_string()),
        });
    }
    if fields.is_empty() {
        return Ok(None);
    }
    Ok(Some(ServerRules {
        version: value["version"]
            .as_str()
            .filter(|version| version.len() <= 64)
            .map(str::to_owned),
        description: text(&value["description"], MAX_RULE_CHARS),
        fields,
    }))
}

/// `PUT /guilds/{g}/requests/@me`: every field as Discord sent it, with
/// its `response` (docs/DISCORD.md); `None` when a field has no such copy.
pub(crate) fn join_request(rules: &ServerRules, answers: &[RulesAnswer]) -> Option<Value> {
    let mut form_fields = Vec::with_capacity(answers.len());
    for (field, answer) in rules.fields.iter().zip(answers) {
        let Ok(Value::Object(mut sent)) = serde_json::from_str::<Value>(field.echo.as_deref()?)
        else {
            return None;
        };
        let response = match answer {
            RulesAnswer::Agreed(agreed) => Value::Bool(*agreed),
            RulesAnswer::Text(written) => match written.trim() {
                "" => Value::Null,
                kept => Value::from(kept),
            },
            RulesAnswer::Choice(index) => index.map_or(Value::Null, Value::from),
        };
        sent.insert("response".to_owned(), response);
        form_fields.push(Value::Object(sent));
    }
    Some(json!({ "version": rules.version, "form_fields": form_fields }))
}

/// What a join request's `application_status` says. Without one there was
/// nothing to review; a status RUNE does not know waits like `SUBMITTED`,
/// so the account stays out until its membership says otherwise.
pub(crate) fn outcome(value: &Value) -> JoinOutcome {
    match value["application_status"].as_str().unwrap_or("APPROVED") {
        "APPROVED" => JoinOutcome::Approved,
        "REJECTED" => JoinOutcome::Rejected,
        _ => JoinOutcome::InReview,
    }
}

/// `GET /guilds/{g}/onboarding` (also `PUT`'s answer). It reads more
/// questions than RUNE saves, so a larger onboarding still shows.
pub(crate) fn onboarding(value: &Value) -> Result<Onboarding, LoginError> {
    if !value.is_object() {
        return Err(wire::invalid());
    }
    let prompts = rows(&value["prompts"], MAX_PROMPTS_READ)?
        .iter()
        .map(prompt)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Onboarding {
        enabled: value["enabled"].as_bool().unwrap_or(false),
        mode: if value["mode"].as_u64() == Some(1) {
            OnboardingMode::Advanced
        } else {
            OnboardingMode::Default
        },
        default_channels: ids(&value["default_channel_ids"], 500)?
            .into_iter()
            .map(ChannelId)
            .collect(),
        prompts,
        chosen: ids(&value["responses"], MAX_PROMPTS_READ * MAX_PROMPT_OPTIONS)?
            .into_iter()
            .map(ChoiceId)
            .collect(),
    })
}

fn prompt(value: &Value) -> Result<Prompt, LoginError> {
    Ok(Prompt {
        id: PromptId(wire::id(&value["id"])?),
        title: text(&value["title"], MAX_PROMPT_TITLE).unwrap_or_default(),
        style: if value["type"].as_u64() == Some(1) {
            PromptStyle::Dropdown
        } else {
            PromptStyle::Choices
        },
        single: value["single_select"].as_bool().unwrap_or(false),
        required: value["required"].as_bool().unwrap_or(false),
        in_onboarding: value["in_onboarding"].as_bool().unwrap_or(true),
        options: rows(&value["options"], MAX_PROMPT_OPTIONS)?
            .iter()
            .map(|option| {
                let emoji = &option["emoji"];
                let emoji_id = match &emoji["id"] {
                    Value::Null => None,
                    id => Some(EmojiId(wire::id(id)?)),
                };
                Ok(PromptChoice {
                    id: ChoiceId(wire::id(&option["id"])?),
                    title: text(&option["title"], MAX_OPTION_TITLE).unwrap_or_default(),
                    description: text(&option["description"], MAX_OPTION_DESCRIPTION),
                    emoji: text(&emoji["name"], rune_model::onboarding::MAX_EMOJI_NAME).map(
                        |name| PromptEmoji {
                            name,
                            id: emoji_id,
                            animated: emoji["animated"].as_bool().unwrap_or(false),
                        },
                    ),
                    roles: ids(&option["role_ids"], 250)?
                        .into_iter()
                        .map(RoleId)
                        .collect(),
                    channels: ids(&option["channel_ids"], 500)?
                        .into_iter()
                        .map(ChannelId)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, LoginError>>()?,
    })
}

/// The body of `POST /guilds/{g}/onboarding-responses` (docs/DISCORD.md): the
/// options chosen, and when each question and option asked was seen.
#[derive(Serialize)]
struct Responses {
    onboarding_responses: Vec<String>,
    onboarding_prompts_seen: BTreeMap<String, u64>,
    onboarding_responses_seen: BTreeMap<String, u64>,
}

/// What answering `onboarding` with `chosen` sends: every question asked
/// and its options count as seen `now_ms` into the epoch, as the official
/// client stamps them with `Date.now()`.
pub(crate) fn responses(onboarding: &Onboarding, chosen: &[ChoiceId], now_ms: u64) -> Value {
    let mut body = Responses {
        onboarding_responses: chosen.iter().map(|id| id.0.to_string()).collect(),
        onboarding_prompts_seen: BTreeMap::new(),
        onboarding_responses_seen: BTreeMap::new(),
    };
    for prompt in onboarding.asked() {
        body.onboarding_prompts_seen
            .insert(prompt.id.0.to_string(), now_ms);
        for option in &prompt.options {
            body.onboarding_responses_seen
                .insert(option.id.0.to_string(), now_ms);
        }
    }
    serde_json::to_value(body).unwrap_or(Value::Null)
}

/// What the answer to the responses says was chosen; `None` without it.
pub(crate) fn chosen(value: &Value) -> Option<Vec<ChoiceId>> {
    let rows = value["onboarding_responses"].as_array()?;
    rows.iter()
        .map(|id| wire::id(id).ok().map(ChoiceId))
        .collect()
}

/// `PUT /guilds/{g}/onboarding`: the edit as Discord documents it, each
/// option's emoji in its three fields.
pub(crate) fn edit(onboarding: &Onboarding) -> Value {
    let strings = |ids: &mut dyn Iterator<Item = u64>| -> Vec<String> {
        ids.map(|id| id.to_string()).collect()
    };
    let prompts: Vec<Value> = onboarding
        .prompts
        .iter()
        .map(|prompt| {
            let options: Vec<Value> = prompt
                .options
                .iter()
                .map(|option| {
                    let emoji = option.emoji.as_ref();
                    json!({
                        "id": option.id.0.to_string(),
                        "title": option.title.trim(),
                        "description": option
                            .description
                            .as_deref()
                            .map(str::trim)
                            .filter(|text| !text.is_empty()),
                        "emoji_id": emoji.and_then(|emoji| emoji.id).map(|id| id.0.to_string()),
                        "emoji_name": emoji.map(|emoji| emoji.name.clone()),
                        "emoji_animated": emoji.is_some_and(|emoji| emoji.animated),
                        "role_ids": strings(&mut option.roles.iter().map(|role| role.0)),
                        "channel_ids": strings(&mut option.channels.iter().map(|channel| channel.0)),
                    })
                })
                .collect();
            json!({
                "id": prompt.id.0.to_string(),
                "type": if prompt.style == PromptStyle::Dropdown { 1 } else { 0 },
                "title": prompt.title.trim(),
                "single_select": prompt.single,
                "required": prompt.required,
                "in_onboarding": prompt.in_onboarding,
                "options": options,
            })
        })
        .collect();
    json!({
        "prompts": prompts,
        "default_channel_ids": strings(&mut onboarding.default_channels.iter().map(|channel| channel.0)),
        "enabled": onboarding.enabled,
        "mode": if onboarding.mode == OnboardingMode::Advanced { 1 } else { 0 },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A photography club's form, as the member-verification route shapes
    /// it: a paragraph, the rules (one with stray spacing and a bell), a
    /// choice and a kind RUNE does not know, each with what RUNE does not
    /// model left in.
    fn club_form() -> Value {
        json!({
            "version": "2026-09-30T18:45:12.402000+00:00",
            "description": "  Preséntate antes de publicar.  ",
            "form_fields": [
                {"field_type": "PARAGRAPH", "label": "Cuéntanos qué fotografías",
                    "placeholder": "Retratos, calle, astro…", "required": true,
                    "field_hint": "kept"},
                {"field_type": "TERMS", "label": "Normas del club", "required": true,
                    "values": ["Cita al autor de cada foto  ", "Nada de \u{7}marcas de agua ajenas"]},
                {"field_type": "MULTIPLE_CHOICE", "label": "¿Hace cuánto sacas?",
                    "choices": ["Menos de un año", "Uno a cinco", "Más"], "required": false},
                {"field_type": "SELFIE_CHECK", "label": "Verificación", "required": false},
            ],
        })
    }

    #[test]
    fn a_form_shows_cleaned_text_and_gives_each_field_back_as_it_came() {
        let rules = rules(&club_form()).unwrap().unwrap();
        assert_eq!(
            rules.description.as_deref(),
            Some("Preséntate antes de publicar.")
        );
        let kinds: Vec<_> = rules
            .fields
            .iter()
            .map(|field| field.kind.clone())
            .collect();
        assert_eq!(
            kinds,
            [
                RulesFieldKind::Paragraph,
                RulesFieldKind::Terms,
                RulesFieldKind::MultipleChoice,
                RulesFieldKind::Other("SELFIE_CHECK".into()),
            ]
        );
        assert_eq!(
            rules.fields[1].rules,
            [
                "Cita al autor de cada foto",
                "Nada de marcas de agua ajenas"
            ]
        );
        assert!(rules.reviewed());
        let answers = [
            RulesAnswer::Text("  Calle y retratos en blanco y negro \n".into()),
            RulesAnswer::Agreed(true),
            RulesAnswer::Choice(Some(2)),
            RulesAnswer::Text(String::new()),
        ];
        let body = join_request(&rules, &answers).unwrap();
        assert_eq!(body["version"], club_form()["version"]);
        let sent = body["form_fields"].as_array().unwrap();
        for (index, field) in sent.iter().enumerate() {
            let mut came = club_form()["form_fields"][index].clone();
            came["response"] = field["response"].clone();
            assert_eq!(*field, came, "field {index} as it came, with its answer");
        }
        assert_eq!(
            sent[0]["response"],
            json!("Calle y retratos en blanco y negro")
        );
        assert_eq!(sent[1]["response"], json!(true));
        assert_eq!(sent[2]["response"], json!(2));
        assert_eq!(sent[3]["response"], Value::Null);
        // Fields the session did not read have nothing to give back.
        let mut made = rules.clone();
        made.fields[0].echo = None;
        assert!(join_request(&made, &answers).is_none());
    }

    #[test]
    fn a_form_out_of_bounds_is_refused_and_an_empty_one_asks_nothing() {
        assert_eq!(
            rules(&json!({"version": null, "form_fields": null})).unwrap(),
            None
        );
        let too_many: Vec<Value> = (0..=MAX_RULES_FIELDS)
            .map(|_| json!({"field_type": "TERMS", "label": "Normas"}))
            .collect();
        assert!(rules(&json!({ "form_fields": too_many })).is_err());
        let unnamed = json!({"form_fields": [{"label": "Sin tipo"}]});
        assert!(rules(&unnamed).is_err());
        for (status, outcome_said) in [
            (
                json!({"application_status": "APPROVED"}),
                JoinOutcome::Approved,
            ),
            (json!({"id": "1290000000000000400"}), JoinOutcome::Approved),
            (
                json!({"application_status": "SUBMITTED"}),
                JoinOutcome::InReview,
            ),
            (
                json!({"application_status": "UNDER_REVIEW"}),
                JoinOutcome::InReview,
            ),
            (
                json!({"application_status": "REJECTED"}),
                JoinOutcome::Rejected,
            ),
        ] {
            assert_eq!(outcome(&status), outcome_said, "{status}");
        }
    }

    /// A club's onboarding: `count` questions, the first two asked at once,
    /// the rest only later.
    fn club_onboarding(count: usize) -> Value {
        let prompts: Vec<Value> = (0..count as u64)
            .map(|index| {
                let id = 1_290_000_000_000_000_100 + index * 10;
                json!({
                    "id": id.to_string(), "type": index % 2, "title": format!("Pregunta {index}"),
                    "in_onboarding": index < 2, "required": index == 0, "single_select": index == 1,
                    "options": [
                        {"id": (id + 1).to_string(), "title": "Revelado", "role_ids": ["1290000000000000900"],
                            "channel_ids": [], "emoji": {"id": "1290000000000000950", "name": "rollo", "animated": false}},
                        {"id": (id + 2).to_string(), "title": "Edición", "description": "Lightroom y otros",
                            "role_ids": [], "channel_ids": ["1290000000000000800"], "emoji": {"id": null, "name": "🖼️"}}
                    ]
                })
            })
            .collect();
        json!({
            "guild_id": "1290000000000000001", "enabled": true, "mode": 1,
            "default_channel_ids": ["1290000000000000801", "1290000000000000802"],
            "responses": ["1290000000000000112", "1290000000000000121"],
            "below_requirements": false, "prompts": prompts,
        })
    }

    #[test]
    fn an_onboarding_reads_beyond_what_rune_saves_and_answers_what_was_asked() {
        let read = onboarding(&club_onboarding(rune_model::onboarding::MAX_PROMPTS + 3)).unwrap();
        assert_eq!(read.prompts.len(), rune_model::onboarding::MAX_PROMPTS + 3);
        assert_eq!(read.mode, OnboardingMode::Advanced);
        assert_eq!(read.prompts[1].style, PromptStyle::Dropdown);
        assert_eq!(read.asked().count(), 2);
        let own = &read.prompts[0].options[0];
        assert_eq!(
            own.emoji.as_ref().and_then(|emoji| emoji.id),
            Some(EmojiId(1_290_000_000_000_000_950))
        );
        assert_eq!(
            read.prompts[0].options[1].description.as_deref(),
            Some("Lightroom y otros")
        );
        // The provider's answers may name a question asked later.
        assert_eq!(read.chosen.len(), 2);
        assert_eq!(read.answered(), [ChoiceId(1_290_000_000_000_000_112)]);
        let sent = responses(&read, &read.answered(), 1_760_000_000_000);
        assert_eq!(sent["onboarding_responses"], json!(["1290000000000000112"]));
        assert_eq!(
            sent["onboarding_prompts_seen"],
            json!({"1290000000000000100": 1_760_000_000_000_u64, "1290000000000000110": 1_760_000_000_000_u64})
        );
        assert_eq!(
            sent["onboarding_responses_seen"].as_object().unwrap().len(),
            4
        );
        assert_eq!(
            chosen(&json!({"onboarding_responses": ["1290000000000000112"]})),
            Some(vec![ChoiceId(1_290_000_000_000_000_112)])
        );
        assert_eq!(chosen(&json!({"onboarding_responses": ["no"]})), None);
        assert!(onboarding(&club_onboarding(MAX_PROMPTS_READ + 1)).is_err());
        assert!(onboarding(&Value::Null).is_err());
    }

    #[test]
    fn an_edit_goes_as_discord_documents_it_without_the_answers() {
        let read = onboarding(&club_onboarding(2)).unwrap();
        let body = edit(&read);
        assert_eq!(body["mode"], json!(1));
        assert_eq!(
            body["default_channel_ids"],
            json!(["1290000000000000801", "1290000000000000802"])
        );
        let first = &body["prompts"][0];
        assert_eq!(first["id"], json!("1290000000000000100"));
        assert_eq!(first["type"], json!(0));
        assert_eq!(body["prompts"][1]["type"], json!(1));
        assert_eq!(first["required"], json!(true));
        assert_eq!(
            first["options"][0]["emoji_id"],
            json!("1290000000000000950")
        );
        assert_eq!(first["options"][0]["emoji_name"], json!("rollo"));
        assert_eq!(first["options"][1]["emoji_id"], Value::Null);
        assert_eq!(
            first["options"][1]["channel_ids"],
            json!(["1290000000000000800"])
        );
        assert!(
            body.get("responses").is_none(),
            "an edit never sets answers"
        );
    }
}
