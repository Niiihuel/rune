//! What the onboarding page shows (2026-10-18): the switch, what counts
//! toward the requirements, the default channels and each question with
//! its options, whose roles and channels open below them.

use limn::{
    Button, ButtonSize, ButtonVariant, Checkbox, Component, DropdownOption, Element, Icon, Select,
    TextField,
};
use rune_components::server_settings as parts;
use rune_components::{line, strong_line, text};
use rune_i18n::{t, tr};
use rune_model::onboarding::{everyone_writes, MAX_PROMPTS, MAX_PROMPT_OPTIONS};
use rune_model::{ChannelKind, OnboardingMode, Prompt, PromptChoice, PromptStyle};

use super::View;
use crate::server_settings::onboarding as page;

impl View<'_> {
    /// The onboarding page.
    pub(super) fn onboarding(&self) -> Element {
        let look = self.look;
        let denied = self.abilities.manage_onboarding.reason();
        let state = &self.settings.onboarding;
        // While a save is on its way the page waits: its answer replaces the draft.
        let can = denied.is_none() && !state.saving();
        let mut children = vec![parts::page_title(
            t("Incorporación"),
            Some(t("Lo que se le pregunta a quien entra, y los roles y canales que recibe según lo que responda.")),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
            return parts::page("ss:scroll:onboarding", self.metrics.content, children);
        }
        let Some(draft) = &state.draft else {
            children.push(if state.failed {
                Element::row(vec![
                    text(t("No se pudo leer la incorporación."), 14.0, look.muted).centered_y(),
                    Button::new("ss:onb:reload", t("Reintentar"), look.palette())
                        .variant(ButtonVariant::Link)
                        .render(),
                ])
                .gap(8.0)
            } else {
                line(t("Cargando…"), 14.0, look.muted)
            });
            return parts::page("ss:scroll:onboarding", self.metrics.content, children);
        };
        let width = self.metrics.content;
        children.push(parts::switch_row(
            "ss:onb:enabled",
            t("Activar la incorporación"),
            Some(t(
                "Quien entra responde las preguntas antes de ver el servidor.",
            )),
            draft.enabled,
            can,
            look,
        ));
        children.push(parts::divider(look));
        children.push(parts::field_label(
            t("Qué cuenta para los requisitos"),
            false,
            look,
        ));
        children.push(text(
            t("El servicio pide canales donde todos puedan escribir; elegí si cuentan sólo los canales por defecto o también los que abren las preguntas."),
            14.0,
            look.muted,
        ));
        let mode_label = |mode: OnboardingMode| match mode {
            OnboardingMode::Default => t("Sólo los canales por defecto"),
            OnboardingMode::Advanced => t("Los canales por defecto y los de las preguntas"),
        };
        children.push(
            Select::new(
                "ss:onb:mode-pick",
                mode_label(draft.mode),
                [
                    ("default", OnboardingMode::Default),
                    ("advanced", OnboardingMode::Advanced),
                ]
                .into_iter()
                .map(|(key, mode)| {
                    DropdownOption::new(format!("ss:onb:mode:{key}"), mode_label(mode))
                        .selected(draft.mode == mode)
                })
                .collect(),
                state.picking_mode,
                "ss:onb:mode-dismiss",
                look.palette(),
            )
            .menu_palette(look.menu_palette())
            .accessibility_label(t("Qué cuenta para los requisitos"))
            .width(380.0)
            .menu_width(380.0)
            .enabled(can)
            .render(),
        );
        children.push(parts::divider(look));
        children.push(parts::field_label(t("Canales por defecto"), false, look));
        children.push(text(
            t("Los canales que recibe todo el que entra, responda lo que responda."),
            14.0,
            look.muted,
        ));
        for (channel, name) in self.channels_of(&[ChannelKind::Text]) {
            let open = self
                .store
                .channel(channel)
                .is_some_and(|found| everyone_writes(self.server, found));
            let label = if open {
                tr!("{channel} · todos pueden escribir", channel = name)
            } else {
                name
            };
            children.push(
                Checkbox::new(
                    format!("ss:onb:default:{}", channel.0),
                    draft.default_channels.contains(&channel),
                    look.palette(),
                )
                .label(label)
                .enabled(can)
                .render(),
            );
        }
        children.push(parts::divider(look));
        children.push(parts::field_label(t("Preguntas"), false, look));
        children.push(text(
            tr!(
                "Hasta {max} preguntas. Cada opción puede dar roles y abrir canales.",
                max = MAX_PROMPTS
            ),
            14.0,
            look.muted,
        ));
        for prompt in &draft.prompts {
            children.push(self.prompt_card(prompt, can, width));
        }
        children.push(
            Button::new("ss:onb:prompt-add", t("Agregar pregunta"), look.palette())
                .icon(Icon::Add)
                .variant(ButtonVariant::Outline)
                .enabled(can && draft.prompts.len() < MAX_PROMPTS)
                .render(),
        );
        if let Some(problem) = page::problem(self.settings, self.store) {
            children.push(parts::denied(&problem, look));
        }
        parts::page("ss:scroll:onboarding", width, children)
    }

    fn prompt_card(&self, prompt: &Prompt, can: bool, width: f32) -> Element {
        let look = self.look;
        let id = prompt.id.0;
        let check = |what: &str, label: &str, on: bool| {
            Checkbox::new(format!("ss:onb:prompt:{id}:{what}"), on, look.palette())
                .label(label)
                .enabled(can)
                .render()
        };
        let mut rows = vec![
            Element::row(vec![
                TextField::new(
                    format!("ss:onb:prompt:{id}:title"),
                    prompt.title.clone(),
                    t("Por ejemplo: ¿Qué te trae por acá?"),
                    look.palette(),
                )
                .accessibility_label(t("Título de la pregunta"))
                .enabled(can)
                .render()
                .flex(1.0),
                Button::new(format!("ss:onb:prompt:{id}:remove"), "", look.palette())
                    .icon(Icon::Trash)
                    .variant(ButtonVariant::Ghost)
                    .button_size(ButtonSize::Small)
                    .accessibility_label(t("Quitar la pregunta"))
                    .tooltip(t("Quitar la pregunta"))
                    .enabled(can)
                    .render()
                    .centered_y(),
            ])
            .gap(8.0),
            Element::row(vec![
                check("single", t("Una sola respuesta"), prompt.single),
                check("required", t("Obligatoria"), prompt.required),
                check("in-onboarding", t("Al entrar"), prompt.in_onboarding),
                check(
                    "dropdown",
                    t("Lista desplegable"),
                    prompt.style == PromptStyle::Dropdown,
                ),
            ])
            .gap(16.0),
        ];
        for option in &prompt.options {
            rows.push(self.option_row(option, can));
        }
        rows.push(
            Button::new(
                format!("ss:onb:prompt:{id}:option-add"),
                t("Agregar opción"),
                look.palette(),
            )
            .icon(Icon::Add)
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .enabled(can && prompt.options.len() < MAX_PROMPT_OPTIONS)
            .render(),
        );
        Element::panel(Element::column(rows).gap(10.0).padding(14.0))
            .background(look.raised)
            .radius(8.0)
            .size(Some(width), None)
    }

    fn option_row(&self, option: &PromptChoice, can: bool) -> Element {
        let look = self.look;
        let id = option.id.0;
        let open = self.settings.onboarding.open == Some(option.id);
        let emoji = option.emoji.as_ref().map(|emoji| emoji.name.clone());
        let summary = tr!(
            "Roles: {roles} · Canales: {channels}",
            roles = option.roles.len(),
            channels = option.channels.len()
        );
        let title = if option.title.trim().is_empty() {
            t("Opción sin título").to_owned()
        } else {
            match &emoji {
                Some(emoji) => format!("{emoji} {}", option.title.trim()),
                None => option.title.trim().to_owned(),
            }
        };
        let mut rows = vec![Element::row(vec![
            Element::column(vec![
                strong_line(title, 14.0, look.text),
                line(summary, 12.0, look.muted),
            ])
            .gap(2.0)
            .flex(1.0),
            Button::new(
                format!("ss:onb:option:{id}:open"),
                if open { t("Cerrar") } else { t("Editar") },
                look.palette(),
            )
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .render()
            .centered_y(),
            Button::new(format!("ss:onb:option:{id}:remove"), "", look.palette())
                .icon(Icon::Close)
                .variant(ButtonVariant::Ghost)
                .button_size(ButtonSize::Small)
                .accessibility_label(t("Quitar la opción"))
                .tooltip(t("Quitar la opción"))
                .enabled(can)
                .render()
                .centered_y(),
        ])
        .gap(8.0)];
        if open {
            rows.push(
                Element::row(vec![
                    TextField::new(
                        format!("ss:onb:option:{id}:emoji"),
                        emoji.unwrap_or_default(),
                        "🙂",
                        look.palette(),
                    )
                    .accessibility_label(t("Emoji de la opción"))
                    .enabled(can)
                    .render()
                    .size(Some(64.0), None),
                    TextField::new(
                        format!("ss:onb:option:{id}:title"),
                        option.title.clone(),
                        t("Título de la opción"),
                        look.palette(),
                    )
                    .accessibility_label(t("Título de la opción"))
                    .enabled(can)
                    .render()
                    .flex(1.0),
                ])
                .gap(8.0),
            );
            rows.push(
                TextField::new(
                    format!("ss:onb:option:{id}:description"),
                    option.description.clone().unwrap_or_default(),
                    t("Descripción (opcional)"),
                    look.palette(),
                )
                .accessibility_label(t("Descripción de la opción"))
                .enabled(can)
                .render(),
            );
            rows.push(line(t("Roles que da"), 12.0, look.muted));
            let roles: Vec<Element> = self
                .server
                .assignable_roles()
                .iter()
                .map(|role| {
                    let locked = super::super::grant_lock(self.store, self.server.id, role);
                    Checkbox::new(
                        format!("ss:onb:option:{id}:role:{}", role.id.0),
                        option.roles.contains(&role.id),
                        look.palette(),
                    )
                    .label(role.name.clone())
                    .enabled(can && (locked.is_none() || option.roles.contains(&role.id)))
                    .render()
                })
                .collect();
            rows.push(if roles.is_empty() {
                line(
                    t("Este servidor no tiene roles para dar."),
                    12.0,
                    look.faint,
                )
            } else {
                Element::column(roles).gap(4.0)
            });
            rows.push(line(t("Canales que abre"), 12.0, look.muted));
            rows.push(
                Element::column(
                    self.channels_of(&[ChannelKind::Text])
                        .into_iter()
                        .map(|(channel, name)| {
                            Checkbox::new(
                                format!("ss:onb:option:{id}:channel:{}", channel.0),
                                option.channels.contains(&channel),
                                look.palette(),
                            )
                            .label(name)
                            .enabled(can)
                            .render()
                        })
                        .collect(),
                )
                .gap(4.0),
            );
        }
        Element::panel(Element::column(rows).gap(8.0).padding(10.0))
            .background(look.canvas)
            .border(look.border, 1.0)
            .radius(6.0)
    }
}
