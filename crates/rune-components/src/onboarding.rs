//! What a server asks of someone who joins it (2026-10-18): its questions,
//! whose options give roles and open channels, and its rules, to accept
//! (with any other field it asks) before taking part. Pure views: what is
//! chosen and written lives in `rune-ui`.

use limn::{
    Button, ButtonVariant, Checkbox, Component, ControlSize, Element, FontFamily, Icon, Modal,
    RadioGroup, RadioOption, TextField,
};
use rune_i18n::{t, tr};
use rune_model::{ChoiceId, Onboarding, PromptChoice, RulesAnswer, RulesFieldKind, ServerRules};

use crate::{icon, line, strong_line, text, Look};

/// The actions of the dialog, by the domain's ids.
pub mod ids {
    use rune_model::ChoiceId;

    pub const CLOSE: &str = "onb:close";
    pub const SEND: &str = "onb:send";
    pub const RETRY: &str = "onb:retry";

    /// An option of a question, chosen or let go.
    pub fn option(choice: ChoiceId) -> String {
        format!("onb:option:{}", choice.0)
    }

    /// The box that agrees to the rules of the `field`th field.
    pub fn agree(field: usize) -> String {
        format!("onb:agree:{field}")
    }

    /// What is written in the `field`th field.
    pub fn text(field: usize) -> String {
        format!("onb:text:{field}")
    }

    /// The `index`th choice of the `field`th field.
    pub fn choice(field: usize, index: usize) -> String {
        format!("onb:choice:{field}:{index}")
    }

    /// The bar over the composer that opens the dialog for `server`.
    pub fn open(server: rune_model::ServerId) -> String {
        format!("onb:open:{}", server.0)
    }
}

/// The dialog as it is being answered.
pub struct JoinView<'a> {
    /// The server's name.
    pub server: &'a str,
    /// Its questions, while the account still owes them.
    pub onboarding: Option<&'a Onboarding>,
    pub chosen: &'a [ChoiceId],
    /// Its rules, while the account still owes them.
    pub rules: Option<&'a ServerRules>,
    pub answers: &'a [RulesAnswer],
    /// Still asking the provider what there is to answer.
    pub loading: bool,
    /// The answers are on their way.
    pub sending: bool,
    /// It was sent once: each part says what is missing.
    pub tried: bool,
    /// Why it did not go, when it did not.
    pub problem: Option<&'a str>,
    /// What came of it, once everything was answered.
    pub done: Option<&'a str>,
    /// What to answer never arrived: the footer offers to ask again.
    pub retry: bool,
    /// A line under the title: the simulation says it is one.
    pub note: Option<&'a str>,
}

/// The dialog over `root`, `width` wide.
pub fn join_dialog(root: Element, view: &JoinView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width) - 2.0;
    let mut body = Vec::new();
    if let Some(note) = view.note {
        body.push(line(note, 12.0, look.muted));
    }
    let mut sendable = !view.sending && !view.loading;
    if view.loading {
        body.push(
            Element::row(vec![
                Element::spinner("onb:loading", look.accent, limn::SpinnerVariant::Circle)
                    .size(Some(16.0), Some(16.0))
                    .centered_y(),
                line(t("Cargando lo que pide el servidor…"), 14.0, look.muted).centered_y(),
            ])
            .gap(8.0),
        );
    }
    if let Some(onboarding) = view.onboarding {
        if let Some(problem) = onboarding.answer_problem(view.chosen) {
            sendable = false;
            if view.tried {
                body.push(line(problem, 12.0, look.danger));
            }
        }
        for prompt in onboarding.asked() {
            let mut rows = vec![Element::row(vec![
                strong_line(prompt.title.clone(), 15.0, look.text).shrink_to_fit(),
                if prompt.required {
                    line(" *", 15.0, look.danger)
                } else {
                    Element::spacer().size(Some(0.0), None)
                },
            ])];
            rows.push(line(
                if prompt.single {
                    t("Elige una opción")
                } else {
                    t("Elige todas las que quieras")
                },
                12.0,
                look.muted,
            ));
            if prompt.single {
                let picked = prompt
                    .options
                    .iter()
                    .find(|option| view.chosen.contains(&option.id))
                    .map(|option| ids::option(option.id));
                rows.push(
                    RadioGroup::new(
                        format!("onb:prompt:{}", prompt.id.0),
                        prompt
                            .options
                            .iter()
                            .map(|option| {
                                let radio =
                                    RadioOption::new(ids::option(option.id), titled(option));
                                match &option.description {
                                    Some(description) => radio.description(description.clone()),
                                    None => radio,
                                }
                            })
                            .collect(),
                        picked.as_deref(),
                        palette,
                    )
                    .accessibility_label(prompt.title.clone())
                    .enabled(!view.sending)
                    .render(),
                );
            } else {
                for option in &prompt.options {
                    let mut lines = vec![Checkbox::new(
                        ids::option(option.id),
                        view.chosen.contains(&option.id),
                        palette,
                    )
                    .label(titled(option))
                    .enabled(!view.sending)
                    .render()];
                    if let Some(description) = &option.description {
                        lines.push(
                            Element::row(vec![
                                Element::spacer().size(Some(24.0), None),
                                text(description.clone(), 12.0, look.muted).flex(1.0),
                            ])
                            .size(Some(inner), None),
                        );
                    }
                    rows.push(Element::column(lines).gap(2.0));
                }
            }
            body.push(Element::column(rows).gap(6.0));
        }
    }
    if let Some(rules) = view.rules {
        if view.onboarding.is_some() {
            body.push(
                Element::panel(Element::spacer().size(Some(inner), Some(1.0)))
                    .background(look.border),
            );
        }
        body.push(strong_line(t("Reglas del servidor"), 15.0, look.text));
        if let Some(description) = &rules.description {
            body.push(text(description.clone(), 14.0, look.muted).size(Some(inner), None));
        }
        for (index, field) in rules.fields.iter().enumerate() {
            let answer = view
                .answers
                .get(index)
                .cloned()
                .unwrap_or_else(|| field.starting());
            let problem = field.problem(&answer);
            sendable &= problem.is_none();
            let mut rows = Vec::new();
            match (&field.kind, &answer) {
                (RulesFieldKind::Terms, RulesAnswer::Agreed(agreed)) => {
                    let list: Vec<Element> = field
                        .rules
                        .iter()
                        .enumerate()
                        .map(|(number, rule)| {
                            Element::row(vec![
                                strong_line(format!("{}.", number + 1), 14.0, look.muted),
                                text(rule.clone(), 14.0, look.text).flex(1.0),
                            ])
                            .gap(8.0)
                        })
                        .collect();
                    rows.push(
                        Element::panel(Element::column(list).gap(8.0).padding(12.0))
                            .background(look.canvas)
                            .border(look.border, 1.0)
                            .radius(6.0)
                            .size(Some(inner), None),
                    );
                    let label = if field.label.is_empty() {
                        t("Leí y acepto las reglas").to_owned()
                    } else {
                        field.label.clone()
                    };
                    rows.push(
                        Checkbox::new(ids::agree(index), *agreed, palette)
                            .label(label)
                            .enabled(!view.sending)
                            .render(),
                    );
                }
                (
                    RulesFieldKind::ShortAnswer | RulesFieldKind::Paragraph,
                    RulesAnswer::Text(written),
                ) => {
                    rows.push(heading(&field.label, field.required, look));
                    if let Some(description) = &field.description {
                        rows.push(
                            text(description.clone(), 12.0, look.muted).size(Some(inner), None),
                        );
                    }
                    let placeholder = field.placeholder.clone().unwrap_or_default();
                    rows.push(if field.kind == RulesFieldKind::Paragraph {
                        Element::panel(
                            Element::text_area(ids::text(index), written, placeholder)
                                .text_area_auto_grow(88.0, 220.0)
                                .foreground(look.text)
                                .font_family(FontFamily::Sans)
                                .font_size(14.0)
                                .padding(10.0),
                        )
                        .background(look.canvas)
                        .border(look.border, 1.0)
                        .radius(4.0)
                        .size(Some(inner), None)
                    } else {
                        TextField::new(ids::text(index), written.clone(), placeholder, palette)
                            .width(inner)
                            .enabled(!view.sending)
                            .render()
                    });
                    if let Some(max) = field.max_chars() {
                        rows.push(line(
                            format!("{}/{max}", written.chars().count()),
                            11.0,
                            look.faint,
                        ));
                    }
                }
                (RulesFieldKind::MultipleChoice, RulesAnswer::Choice(picked)) => {
                    rows.push(heading(&field.label, field.required, look));
                    rows.push(
                        RadioGroup::new(
                            format!("onb:field:{index}"),
                            field
                                .choices
                                .iter()
                                .enumerate()
                                .map(|(choice, label)| {
                                    RadioOption::new(ids::choice(index, choice), label.clone())
                                })
                                .collect(),
                            picked.map(|choice| ids::choice(index, choice)).as_deref(),
                            palette,
                        )
                        .accessibility_label(field.label.clone())
                        .enabled(!view.sending)
                        .render(),
                    );
                }
                _ => {
                    rows.push(heading(&field.label, field.required, look));
                    rows.push(
                        text(
                            t("RUNE todavía no completa este campo: respóndelo desde el sitio del servicio."),
                            12.0,
                            look.muted,
                        )
                        .size(Some(inner), None),
                    );
                }
            }
            if let Some(problem) = problem.filter(|_| view.tried) {
                rows.push(line(problem, 12.0, look.danger));
            }
            body.push(Element::column(rows).gap(6.0));
        }
    }
    if let Some(done) = view.done {
        body.push(
            Element::row(vec![
                icon(Icon::CheckCircle, look.accent).centered_y(),
                text(done.to_owned(), 14.0, look.text).flex(1.0),
            ])
            .gap(6.0),
        );
    }
    if let Some(problem) = view.problem {
        body.push(
            Element::row(vec![
                icon(Icon::WarningTriangle, look.danger).centered_y(),
                text(problem.to_owned(), 12.0, look.danger).flex(1.0),
            ])
            .gap(6.0),
        );
    }
    let finished =
        view.done.is_some() || (view.onboarding.is_none() && view.rules.is_none() && !view.retry);
    let mut buttons = vec![Button::new(
        ids::CLOSE,
        if finished {
            t("Cerrar")
        } else {
            t("Más tarde")
        },
        palette,
    )
    .variant(ButtonVariant::Secondary)
    .size(ControlSize::Spacious)
    .enabled(!view.sending)
    .render()
    .flex(1.0)];
    if view.retry && !finished && !view.loading {
        buttons.push(
            Button::new(ids::RETRY, t("Reintentar"), look.accent_palette())
                .size(ControlSize::Spacious)
                .render()
                .flex(1.0),
        );
    } else if !finished && !view.loading {
        buttons.push(
            Button::new(
                ids::SEND,
                if view.sending {
                    t("Enviando…")
                } else {
                    t("Terminar")
                },
                look.accent_palette(),
            )
            .size(ControlSize::Spacious)
            .enabled(sendable || !view.tried)
            .loading(view.sending)
            .render()
            .flex(1.0),
        );
    }
    let footer = Element::row(buttons).gap(9.0);
    Modal::new(
        root,
        true,
        tr!("Antes de participar en {server}", server = view.server),
        "",
        ids::CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(16.0))
    .footer(footer)
    .initial_focus(if finished || view.loading || view.retry {
        ids::CLOSE
    } else {
        ids::SEND
    })
    .render()
}

/// An option as it is listed: its emoji, if it has a Unicode one, and its
/// title; a server's own emoji shows by its name.
fn titled(option: &PromptChoice) -> String {
    match &option.emoji {
        Some(emoji) if emoji.id.is_none() => format!("{} {}", emoji.name, option.title),
        Some(emoji) => format!(":{}: {}", emoji.name, option.title),
        None => option.title.clone(),
    }
}

fn heading(label: &str, required: bool, look: &Look) -> Element {
    Element::row(vec![
        strong_line(label.to_uppercase(), 12.0, look.muted).shrink_to_fit(),
        if required {
            line(" *", 12.0, look.danger)
        } else {
            Element::spacer().size(Some(0.0), None)
        },
    ])
}

/// The line over the composer of a server that still asks something of
/// the account: what, and a button that opens the dialog.
pub fn steps_bar(
    message: &str,
    button: &str,
    server: rune_model::ServerId,
    blocking: bool,
    look: &Look,
) -> Element {
    let ink = if blocking { look.text } else { look.muted };
    Element::row(vec![
        icon(
            if blocking {
                Icon::Lock
            } else {
                Icon::InfoCircle
            },
            look.accent,
        )
        .centered_y(),
        line(message, 13.0, ink).flex(1.0).centered_y(),
        Button::new(ids::open(server), button, look.accent_palette())
            .size(ControlSize::Compact)
            .render()
            .centered_y(),
    ])
    .gap(8.0)
    .padding(6.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;
    use rune_model::{Prompt, PromptId, PromptStyle, RulesField};

    fn view<'a>(
        onboarding: Option<&'a Onboarding>,
        chosen: &'a [ChoiceId],
        rules: Option<&'a ServerRules>,
        answers: &'a [RulesAnswer],
    ) -> JoinView<'a> {
        JoinView {
            server: "Club",
            onboarding,
            chosen,
            rules,
            answers,
            loading: false,
            sending: false,
            tried: false,
            problem: None,
            done: None,
            retry: false,
            note: None,
        }
    }

    #[test]
    fn the_dialog_offers_each_option_and_the_rules_to_agree_to() {
        let look = Look::DARK;
        let onboarding = Onboarding {
            enabled: true,
            prompts: vec![Prompt {
                id: PromptId(1),
                title: "¿Qué te trae?".into(),
                style: PromptStyle::Choices,
                single: false,
                required: true,
                in_onboarding: true,
                options: vec![PromptChoice {
                    id: ChoiceId(10),
                    title: "Fotos".into(),
                    description: Some("Para quien saca fotos".into()),
                    emoji: None,
                    roles: Vec::new(),
                    channels: Vec::new(),
                }],
            }],
            ..Onboarding::default()
        };
        let rules = ServerRules {
            version: None,
            description: None,
            fields: vec![RulesField {
                kind: RulesFieldKind::Terms,
                label: String::new(),
                description: None,
                placeholder: None,
                rules: vec!["Respeto".into()],
                choices: Vec::new(),
                required: true,
                echo: None,
            }],
        };
        let answers = [RulesAnswer::Agreed(false)];
        let (_, shown) = frame(
            join_dialog(
                Element::spacer(),
                &view(Some(&onboarding), &[], Some(&rules), &answers),
                480.0,
                &look,
            ),
            Size::new(900.0, 800.0),
        );
        let found = controls(&shown);
        for id in [
            ids::option(ChoiceId(10)),
            ids::agree(0),
            ids::SEND.into(),
            ids::CLOSE.into(),
        ] {
            assert!(found.contains(&id), "{id} in {found:?}");
        }
        // Sent once with nothing answered: each part says what is missing,
        // and «Terminar» waits for them.
        let (_, shown) = frame(
            join_dialog(
                Element::spacer(),
                &JoinView {
                    tried: true,
                    ..view(Some(&onboarding), &[], Some(&rules), &answers)
                },
                480.0,
                &look,
            ),
            Size::new(900.0, 800.0),
        );
        assert!(!controls(&shown).contains(&ids::SEND.to_owned()));
        let painted = texts(&shown).join(" ");
        for said in [
            "Antes de participar en Club",
            "¿Qué te trae?",
            "Fotos",
            "Respeto",
            "Leí y acepto las reglas",
            "Responde «¿Qué te trae?»",
            "Acepta las reglas para seguir",
        ] {
            assert!(painted.contains(said), "{said} in {painted}");
        }
        // Everything answered: only closing is left.
        let (_, done) = frame(
            join_dialog(
                Element::spacer(),
                &JoinView {
                    done: Some("Listo"),
                    ..view(None, &[], None, &[])
                },
                480.0,
                &look,
            ),
            Size::new(900.0, 800.0),
        );
        let found = controls(&done);
        assert!(found.contains(&ids::CLOSE.to_owned()) && !found.contains(&ids::SEND.to_owned()));
    }
}
