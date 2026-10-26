//! What the AutoMod page shows (automod): the rules, each with its
//! switch; the kinds of rule to make; what AutoMod did lately; and, in
//! place of all that, the editor of one rule.

use limn::{
    AlertDialog, Button, ButtonVariant, Checkbox, Component, DropdownOption, Element, Empty, Icon,
    Select, TextArea, TextField,
};
use rune_components::automod::{kind_glyph, new_rule_row, rule_row, RuleLine};
use rune_components::server_settings as parts;
use rune_components::{line, strong_line, text};
use rune_i18n::{t, tr};
use rune_model::automod::{MAX_MENTIONS, MAX_NOTICE, MAX_PATTERNS};
use rune_model::{
    AutoModAction, AutoModExecution, AutoModKind, AutoModPreset, AutoModRule, AutoModTrigger,
    ChannelId, ChannelKind,
};
use rune_provider::ProviderKind;

use super::{footer, View};
use crate::server_settings::advanced::TIMEOUTS;
use crate::server_settings::automod::{
    self as page, kind_key, preset_key, Picker, RuleDraft, ALLOWED, KEYWORDS, LIMIT, NAME, NOTICE,
    PATTERNS,
};

/// What a kind of rule is called, and what it does.
fn kind_label(kind: Option<AutoModKind>) -> &'static str {
    match kind {
        Some(AutoModKind::Keywords) => t("Palabras propias"),
        Some(AutoModKind::Mentions) => t("Spam de menciones"),
        Some(AutoModKind::Spam) => t("Spam"),
        Some(AutoModKind::Presets) => t("Listas de palabras"),
        Some(AutoModKind::Profile) => t("Perfiles"),
        None => t("Otro tipo de regla"),
    }
}

fn kind_description(kind: AutoModKind) -> &'static str {
    match kind {
        AutoModKind::Keywords => {
            t("Bloquea las palabras, frases y expresiones regulares que elijas.")
        }
        AutoModKind::Mentions => {
            t("Cuando un mensaje menciona a demasiadas personas o roles.")
        }
        AutoModKind::Spam => t("Mensajes que el servicio toma por spam."),
        AutoModKind::Presets => t(
            "Groserías, contenido sexual o insultos discriminatorios, según las listas del servicio.",
        ),
        AutoModKind::Profile => t("Palabras en los nombres y perfiles de quienes entran."),
    }
}

fn preset_label(preset: AutoModPreset) -> &'static str {
    match preset {
        AutoModPreset::Profanity => t("Groserías"),
        AutoModPreset::Sexual => t("Contenido sexual"),
        AutoModPreset::Slurs => t("Insultos discriminatorios"),
    }
}

/// How long a timeout lasts, as the timeouts of members are named.
fn timeout_label(seconds: u32) -> String {
    TIMEOUTS
        .iter()
        .find(|(minutes, _)| *minutes * 60 == i64::from(seconds))
        .map_or_else(
            || tr!("{minutes} minutos", minutes = seconds / 60),
            |(_, label)| t(label).to_owned(),
        )
}

impl View<'_> {
    fn channel_name(&self, channel: ChannelId) -> String {
        self.store.channel(channel).map_or_else(
            || t("un canal que ya no está").to_owned(),
            |found| format!("#{}", found.name),
        )
    }

    fn action_label(&self, action: &AutoModAction) -> String {
        match action {
            AutoModAction::Block { .. } => t("Bloquea el mensaje").into(),
            AutoModAction::Alert { channel } => {
                tr!("Avisa en {channel}", channel = self.channel_name(*channel))
            }
            AutoModAction::Timeout { seconds } => {
                tr!("Aísla {time}", time = timeout_label(*seconds))
            }
            AutoModAction::Quarantine => t("Bloquea la participación").into(),
            AutoModAction::Other(_) => t("Otra acción").into(),
        }
    }

    /// The AutoMod page.
    pub(super) fn automod(&self) -> Element {
        let look = self.look;
        let state = &self.settings.automod;
        let width = self.metrics.content;
        let denied = self.abilities.manage_automod.reason();
        if let (None, Some(draft)) = (denied, &state.editing) {
            return self.automod_editor(draft);
        }
        let mut children = vec![parts::page_title(
            t("AutoMod"),
            Some(t("Reglas que miran lo que se escribe en el servidor, o los perfiles de quienes entran, y lo bloquean, avisan a la moderación o aíslan a quien lo escribió, sin esperar a nadie.")),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
            return parts::page("ss:scroll:automod", width, children);
        }
        let Some(rules) = self.store.automod_rules(self.server.id) else {
            children.push(if state.failed {
                Element::row(vec![
                    text(t("No se pudieron leer las reglas."), 14.0, look.muted).centered_y(),
                    Button::new("ss:am:reload", t("Reintentar"), look.palette())
                        .variant(ButtonVariant::Link)
                        .render(),
                ])
                .gap(8.0)
            } else {
                line(t("Cargando…"), 14.0, look.muted)
            });
            return parts::page("ss:scroll:automod", width, children);
        };
        children.push(text(
            t("Quien puede gestionar el servidor, o lo administra, no está sujeto a estas reglas."),
            13.0,
            look.muted,
        ));
        if rules.is_empty() {
            children.push(
                Empty::new(
                    t("No hay reglas"),
                    t("Crea una abajo para que AutoMod empiece a cuidar el servidor."),
                    look.palette(),
                )
                .icon(Icon::Bolt)
                .framed()
                .render(),
            );
        } else {
            let rows = rules
                .iter()
                .filter_map(|rule| {
                    let id = rule.id?;
                    let mut notes = vec![kind_label(rule.trigger.kind()).to_owned()];
                    notes.extend(rule.actions.iter().map(|action| self.action_label(action)));
                    Some(rule_row(
                        &RuleLine {
                            id: id.0,
                            name: rule.name.clone(),
                            notes,
                            glyph: kind_glyph(rule.trigger.kind()),
                            enabled: rule.enabled,
                            editable: rule.editable(),
                            can: true,
                        },
                        look,
                    ))
                })
                .collect();
            children.push(Element::column(rows));
        }
        children.push(parts::divider(look));
        children.push(strong_line(t("Crear una regla"), 16.0, look.text));
        for kind in AutoModKind::ALL {
            let count = rules
                .iter()
                .filter(|rule| rule.trigger.kind() == Some(kind))
                .count();
            let room = count < kind.max_rules();
            // Without room, the row says why it cannot make one.
            let description = if room {
                kind_description(kind).to_owned()
            } else {
                format!(
                    "{} {}",
                    kind_description(kind),
                    t("Ya tiene todas las que permite Discord.")
                )
            };
            children.push(new_rule_row(
                kind_key(kind),
                kind,
                kind_label(Some(kind)),
                &description,
                room,
                look,
            ));
        }
        let done: Vec<&AutoModExecution> = self.store.automod_executions(self.server.id).collect();
        if !done.is_empty() {
            children.push(parts::divider(look));
            children.push(strong_line(t("Lo que hizo hace poco"), 16.0, look.text));
            children.push(text(
                t("Desde que se abrió RUNE, lo más nuevo primero."),
                13.0,
                look.muted,
            ));
            children.extend(done.into_iter().map(|done| self.execution_row(done, rules)));
        }
        if self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.kind == ProviderKind::Simulated)
        {
            children.push(text(
                t("Simulada: sólo las palabras propias se comparan con lo que se escribe; las expresiones regulares, las listas, el spam y las menciones se guardan pero no actúan."),
                13.0,
                look.muted,
            ));
        }
        parts::page("ss:scroll:automod", width, children)
    }

    fn execution_row(&self, done: &AutoModExecution, rules: &[AutoModRule]) -> Element {
        let look = self.look;
        let name = self.name_of(done.user);
        let what = match done.action {
            AutoModAction::Block { .. } => tr!("Bloqueó un mensaje de {name}", name),
            AutoModAction::Alert { .. } => tr!("Avisó por un mensaje de {name}", name),
            AutoModAction::Timeout { .. } => tr!("Aisló a {name}", name),
            AutoModAction::Quarantine => tr!("Bloqueó la participación de {name}", name),
            AutoModAction::Other(_) => tr!("Actuó sobre {name}", name),
        };
        let mut notes = Vec::new();
        if let Some(rule) = rules.iter().find(|rule| rule.id == Some(done.rule)) {
            notes.push(rule.name.clone());
        }
        if let Some(channel) = done.channel {
            notes.push(self.channel_name(channel));
        }
        if let Some(matched) = &done.matched {
            notes.push(format!("«{matched}»"));
        }
        Element::column(vec![
            line(what, 14.0, look.text),
            line(notes.join(" · "), 12.0, look.muted),
        ])
        .gap(2.0)
    }

    /// The editor of one rule, in place of the list.
    fn automod_editor(&self, draft: &RuleDraft) -> Element {
        let look = self.look;
        let state = &self.settings.automod;
        let can = !state.saving();
        let rule = &draft.rule;
        let kind = rule.trigger.kind();
        let width = self.metrics.content;
        let title = if rule.id.is_some() {
            t("Editar la regla")
        } else {
            t("Nueva regla")
        };
        // The buttons and what keeps it from saving go on top: the form is long.
        let problem = page::problem(draft, self.store, self.settings);
        let save = parts::primary_button(
            "ss:am:save",
            if can { t("Guardar") } else { t("Guardando…") },
            can && problem.is_none(),
            look,
        );
        let mut children = vec![Element::row(vec![
            parts::page_title(title, Some(kind_label(kind)), look).flex(1.0),
            footer("ss:am:cancel", save, look).centered_y(),
        ])
        .gap(16.0)];
        if let Some(problem) = &problem {
            children.push(parts::denied(problem, look));
        }
        let field = |id: &'static str, value: &str, hint: &str, label: &str| {
            TextField::new(id, value.to_owned(), hint, look.palette())
                .accessibility_label(label)
                .enabled(can)
                .render()
        };
        let area = |id: &'static str, value: &str, hint: &str, label: &str| {
            TextArea::new(id, value.to_owned(), hint, look.palette())
                .accessibility_label(label)
                .height(88.0)
                .enabled(can)
                .render()
        };
        let check = |id: String, label: &str, on: bool| {
            Checkbox::new(id, on, look.palette())
                .label(label)
                .enabled(can)
                .render()
        };
        children.push(parts::field_label(t("Nombre de la regla"), true, look));
        children.push(field(NAME, &draft.name, "", t("Nombre de la regla")));
        // What it looks for.
        match &rule.trigger {
            AutoModTrigger::Keywords(_) | AutoModTrigger::Profile(_) => {
                children.push(parts::field_label(t("Palabras"), false, look));
                children.push(text(
                    t("Separadas por comas. Un * al principio o al final deja seguir la palabra: «spoiler*» también encuentra «spoilers»."),
                    13.0,
                    look.muted,
                ));
                children.push(area(
                    KEYWORDS,
                    &draft.keywords,
                    "spoiler*, *gratis*",
                    t("Palabras"),
                ));
                children.push(parts::field_label(t("Expresiones regulares"), false, look));
                children.push(text(
                    tr!(
                        "Una por línea, hasta {max}, con la sintaxis de Rust.",
                        max = MAX_PATTERNS
                    ),
                    13.0,
                    look.muted,
                ));
                children.push(area(
                    PATTERNS,
                    &draft.patterns,
                    r"\bnitro\s+gratis\b",
                    t("Expresiones regulares"),
                ));
            }
            AutoModTrigger::Presets { lists, .. } => {
                children.push(parts::field_label(t("Listas"), false, look));
                for preset in AutoModPreset::ALL {
                    children.push(check(
                        format!("ss:am:preset:{}", preset_key(preset)),
                        preset_label(preset),
                        lists.contains(&preset),
                    ));
                }
            }
            AutoModTrigger::Mentions { raids, .. } => {
                children.push(parts::field_label(t("Límite de menciones"), true, look));
                children.push(text(
                    tr!(
                        "Cuántas personas y roles distintos puede mencionar un mensaje, de 1 a {max}.",
                        max = MAX_MENTIONS
                    ),
                    13.0,
                    look.muted,
                ));
                children.push(
                    field(LIMIT, &draft.limit, "20", t("Límite de menciones"))
                        .size(Some(120.0), None),
                );
                children.push(check(
                    "ss:am:raids".into(),
                    t("Detectar ataques de menciones"),
                    *raids,
                ));
            }
            AutoModTrigger::Spam | AutoModTrigger::Other(_) => children.push(text(
                t("No hay nada que elegir: el servicio decide qué es spam."),
                14.0,
                look.muted,
            )),
        }
        if matches!(
            rule.trigger,
            AutoModTrigger::Keywords(_)
                | AutoModTrigger::Profile(_)
                | AutoModTrigger::Presets { .. }
        ) {
            children.push(parts::field_label(t("Permitidas"), false, look));
            children.push(text(
                t("Palabras que nunca cuentan, separadas por comas."),
                13.0,
                look.muted,
            ));
            children.push(area(ALLOWED, &draft.allowed, "", t("Permitidas")));
        }
        // What it does.
        children.push(parts::divider(look));
        children.push(parts::field_label(t("Acciones"), true, look));
        let profile = kind == Some(AutoModKind::Profile);
        if !profile {
            children.push(parts::switch_row(
                "ss:am:action:block",
                t("Bloquear el mensaje"),
                Some(t("No se publica, y quien lo escribió lo sabe.")),
                draft.does("block"),
                can,
                look,
            ));
            if draft.does("block") {
                children.push(field(
                    NOTICE,
                    &draft.notice,
                    t("Mensaje para quien lo escribió (opcional)"),
                    t("Mensaje para quien lo escribió"),
                ));
                children.push(line(
                    tr!("Hasta {max} caracteres.", max = MAX_NOTICE),
                    12.0,
                    look.faint,
                ));
            }
        } else {
            children.push(parts::switch_row(
                "ss:am:action:quarantine",
                t("Bloquear su participación"),
                Some(t(
                    "No puede escribir, reaccionar ni hablar hasta que cambie su perfil.",
                )),
                draft.does("quarantine"),
                can,
                look,
            ));
        }
        children.push(parts::switch_row(
            "ss:am:action:alert",
            t("Avisar en un canal"),
            Some(t("La moderación ve qué se encontró y quién lo escribió.")),
            draft.does("alert"),
            can,
            look,
        ));
        if let Some(channel) = rule.actions.iter().find_map(|action| match action {
            AutoModAction::Alert { channel } => Some(*channel),
            _ => None,
        }) {
            let options = self
                .channels_of(&[ChannelKind::Text])
                .into_iter()
                .map(|(id, name)| {
                    DropdownOption::new(format!("ss:am:alert:{}", id.0), name)
                        .selected(id == channel)
                })
                .collect();
            children.push(self.automod_select(
                "ss:am:alert-pick",
                self.channel_name(channel),
                options,
                state.picking == Some(Picker::Alert),
                t("Canal de la alerta"),
                can,
            ));
        }
        if matches!(kind, Some(AutoModKind::Keywords | AutoModKind::Mentions)) {
            children.push(parts::switch_row(
                "ss:am:action:timeout",
                t("Aislar a quien lo escribió"),
                Some(t("No puede escribir, reaccionar ni hablar por un tiempo.")),
                draft.does("timeout"),
                can,
                look,
            ));
            if let Some(seconds) = rule.actions.iter().find_map(|action| match action {
                AutoModAction::Timeout { seconds } => Some(*seconds),
                _ => None,
            }) {
                let options = TIMEOUTS
                    .iter()
                    .map(|(minutes, label)| {
                        let secs = *minutes * 60;
                        DropdownOption::new(format!("ss:am:timeout:{secs}"), t(label))
                            .selected(secs == i64::from(seconds))
                    })
                    .collect();
                children.push(self.automod_select(
                    "ss:am:timeout-pick",
                    timeout_label(seconds),
                    options,
                    state.picking == Some(Picker::Timeout),
                    t("Duración del aislamiento"),
                    can,
                ));
            }
        }
        // What it leaves alone.
        children.push(parts::divider(look));
        children.push(parts::field_label(t("Exenciones"), false, look));
        children.push(text(
            t("Roles y canales que la regla deja tranquilos."),
            13.0,
            look.muted,
        ));
        children.push(line(t("Roles"), 12.0, look.muted));
        let roles: Vec<Element> = self
            .server
            .assignable_roles()
            .into_iter()
            .map(|role| {
                check(
                    format!("ss:am:exempt-role:{}", role.id.0),
                    &role.name,
                    rule.exempt_roles.contains(&role.id),
                )
            })
            .collect();
        children.push(if roles.is_empty() {
            line(t("Este servidor no tiene roles."), 12.0, look.faint)
        } else {
            Element::column(roles).gap(4.0)
        });
        if !profile {
            children.push(line(t("Canales"), 12.0, look.muted));
            children.push(
                Element::column(
                    self.channels_of(&[ChannelKind::Category, ChannelKind::Text])
                        .into_iter()
                        .map(|(id, name)| {
                            check(
                                format!("ss:am:exempt-channel:{}", id.0),
                                &name,
                                rule.exempt_channels.contains(&id),
                            )
                        })
                        .collect(),
                )
                .gap(4.0),
            );
        }
        children.push(parts::divider(look));
        children.push(parts::switch_row(
            "ss:am:enabled",
            t("Regla activada"),
            None,
            rule.enabled,
            can,
            look,
        ));
        parts::page("ss:scroll:automod", width, children)
    }

    fn automod_select(
        &self,
        id: &str,
        current: String,
        options: Vec<DropdownOption>,
        open: bool,
        label: &str,
        enabled: bool,
    ) -> Element {
        Select::new(
            id,
            current,
            options,
            open,
            "ss:am:pick-dismiss",
            self.look.palette(),
        )
        .menu_palette(self.look.menu_palette())
        .accessibility_label(label)
        .width(380.0)
        .menu_width(380.0)
        .enabled(enabled)
        .render()
    }

    /// The dialog that asks before a rule is deleted.
    pub(super) fn automod_dialogs(&self, root: Element) -> Element {
        let look = self.look;
        let Some(id) = self.settings.automod.deleting else {
            return root;
        };
        let name = self
            .store
            .automod_rules(self.server.id)
            .and_then(|rules| rules.iter().find(|rule| rule.id == Some(id)))
            .map_or_else(String::new, |rule| rule.name.clone());
        AlertDialog::new(
            root,
            true,
            t("Eliminar la regla"),
            tr!(
                "¿Seguro que quieres eliminar la regla «{name}»? No se puede deshacer.",
                name
            ),
            "ss:am:delete:confirm",
            "ss:am:delete:cancel",
            look.menu_palette(),
        )
        .confirm_label(t("Eliminar"))
        .destructive(look.danger)
        .width(425.0)
        .render()
    }
}
