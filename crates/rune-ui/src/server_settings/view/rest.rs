//! What the rest of the administration (P7) shows: the safety page, the
//! page that reorders channels, the picture and traits on the profile, a
//! role's picture, emoji and gradient, the members' order and filter, and
//! the button that pauses invitations.

use limn::{
    Button, ButtonSize, ButtonVariant, Component, DropdownOption, Element, Icon, Select, TextField,
    Toggle,
};
use rune_components::server_admin::{self as admin_parts, ChannelSlot};
use rune_components::server_settings::{self as parts, DropEdge, MenuAction, Swatches};
use rune_components::{line, text};
use rune_i18n::{t, tr};
use rune_model::{
    ChannelId, ChannelKind, JoinMethod, MediaScan, MemberOrder, Role, UserId, Verification,
    AFK_TIMEOUTS, MAX_TRAITS,
};
use rune_provider::ProviderKind;

use super::{View, FIELD_HEIGHT};
use crate::server_settings::rest::{self, ROLE_EMOJI};

impl View<'_> {
    fn simulated_provider(&self) -> bool {
        self.store
            .provider
            .as_ref()
            .is_some_and(|info| info.kind == ProviderKind::Simulated)
    }

    /// A dropdown of these pages, open while `key` names it.
    fn pick(
        &self,
        key: &'static str,
        current: String,
        options: Vec<(String, String, bool)>,
        label: &str,
        enabled: bool,
    ) -> Element {
        Select::new(
            format!("ss:pick:{key}"),
            current,
            options
                .into_iter()
                .map(|(value, name, selected)| {
                    DropdownOption::new(format!("ss:set:{key}:{value}"), name).selected(selected)
                })
                .collect(),
            self.settings.rest.select == Some(key),
            "ss:pick-dismiss",
            self.look.palette(),
        )
        .menu_palette(self.look.menu_palette())
        .accessibility_label(label)
        // Narrower in a narrow window (2026-10-19).
        .width(self.metrics.content.min(380.0))
        .menu_width(self.metrics.content.min(380.0))
        .enabled(enabled)
        .render()
    }

    /// The channels of `kinds`, in the list's order, named as the list
    /// names them: `#` before a text channel's name.
    pub(super) fn channels_of(&self, kinds: &[ChannelKind]) -> Vec<(ChannelId, String)> {
        self.store
            .channel_tree(self.server.id)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .filter(|channel| kinds.contains(&channel.kind))
            .map(|channel| {
                let name = if channel.kind.is_voice() {
                    channel.name.clone()
                } else {
                    format!("#{}", channel.name)
                };
                (channel.id, name)
            })
            .collect()
    }

    /// A channel to choose, or none, as a dropdown of `key`.
    fn channel_pick(
        &self,
        key: &'static str,
        chosen: Option<ChannelId>,
        kinds: &[ChannelKind],
        none: &str,
        label: &str,
        enabled: bool,
    ) -> Element {
        let channels = self.channels_of(kinds);
        let current = chosen
            .and_then(|chosen| channels.iter().find(|(id, _)| *id == chosen))
            .map_or_else(|| none.to_owned(), |(_, name)| name.clone());
        let mut options = vec![("none".to_owned(), none.to_owned(), chosen.is_none())];
        options.extend(
            channels
                .iter()
                .map(|(id, name)| (id.to_string(), name.clone(), chosen == Some(*id))),
        );
        self.pick(key, current, options, label, enabled)
    }

    /// The safety and engagement page.
    pub(super) fn safety(&self) -> Element {
        let look = self.look;
        let denied = self.abilities.edit_safety.reason();
        let can = denied.is_none();
        let options = rest::shown_options(self.server, &self.settings.profile);
        let mut children = vec![parts::page_title(
            t("Seguridad y participación"),
            Some(t("Dónde avisa el servicio, cómo se notifica a quien llega, adónde va quien queda inactivo y qué se pide y se revisa.")),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        let messages = options.system_messages;
        children.extend([
            parts::field_label(t("Canal de mensajes del sistema"), false, look),
            text(
                t("Donde se da la bienvenida a quien entra y se agradece a quien mejora el servidor."),
                14.0,
                look.muted,
            ),
            self.channel_pick(
                "system",
                options.system_channel,
                &[ChannelKind::Text],
                t("Sin mensajes del sistema"),
                t("Canal de mensajes del sistema"),
                can,
            ),
            parts::switch_row(
                "ss:system-messages:joins",
                t("Dar la bienvenida a quien entra"),
                None,
                messages.joins,
                can,
                look,
            ),
            parts::switch_row(
                "ss:system-messages:join-replies",
                t("Ofrecer responder la bienvenida con un sticker"),
                None,
                messages.join_replies,
                can,
                look,
            ),
            parts::switch_row(
                "ss:system-messages:boosts",
                t("Agradecer a quien mejora el servidor"),
                None,
                messages.boosts,
                can,
                look,
            ),
            parts::switch_row(
                "ss:system-messages:tips",
                t("Mostrar consejos para configurar el servidor"),
                None,
                messages.tips,
                can,
                look,
            ),
            parts::divider(look),
            parts::field_label(t("Notificaciones predeterminadas"), false, look),
            text(
                t("Cómo se avisa a quien entra, hasta que elija otra cosa."),
                14.0,
                look.muted,
            ),
            self.pick(
                "notify",
                if options.mentions_only {
                    t("Solo @menciones")
                } else {
                    t("Todos los mensajes")
                }
                .into(),
                vec![
                    ("all".into(), t("Todos los mensajes").into(), !options.mentions_only),
                    ("mentions".into(), t("Solo @menciones").into(), options.mentions_only),
                ],
                t("Notificaciones predeterminadas"),
                can,
            ),
            parts::divider(look),
            parts::field_label(t("Inactividad en voz"), false, look),
            text(
                t("Adónde se mueve a quien queda en silencio en un canal de voz, y después de cuánto."),
                14.0,
                look.muted,
            ),
            self.channel_pick(
                "afk",
                options.afk_channel,
                &[ChannelKind::Voice],
                t("Sin canal de inactividad"),
                t("Canal de inactividad"),
                can,
            ),
            self.pick(
                "afk-timeout",
                afk_label(options.afk_timeout),
                AFK_TIMEOUTS
                    .iter()
                    .map(|secs| {
                        (
                            secs.to_string(),
                            afk_label(*secs),
                            *secs == options.afk_timeout,
                        )
                    })
                    .collect(),
                t("Tiempo de inactividad"),
                can,
            ),
            parts::divider(look),
            parts::field_label(t("Nivel de verificación"), false, look),
            text(
                t("Lo que alguien necesita antes de poder escribir."),
                14.0,
                look.muted,
            ),
            self.pick(
                "verification",
                verification_label(options.verification).into(),
                Verification::ALL
                    .iter()
                    .enumerate()
                    .map(|(index, level)| {
                        (
                            index.to_string(),
                            verification_label(*level).into(),
                            *level == options.verification,
                        )
                    })
                    .collect(),
                t("Nivel de verificación"),
                can,
            ),
            parts::divider(look),
            parts::field_label(t("Filtro de contenido explícito"), false, look),
            text(
                t("De quién se revisan las imágenes y los archivos que se envían."),
                14.0,
                look.muted,
            ),
            self.pick(
                "media-scan",
                scan_label(options.media_scan).into(),
                MediaScan::ALL
                    .iter()
                    .enumerate()
                    .map(|(index, scan)| {
                        (
                            index.to_string(),
                            scan_label(*scan).into(),
                            *scan == options.media_scan,
                        )
                    })
                    .collect(),
                t("Filtro de contenido explícito"),
                can,
            ),
        ]);
        // Icon, banner and community (2026-10-19).
        children.extend(self.community(&options));
        parts::page("ss:scroll:safety", self.metrics.content, children)
    }

    // Icon, banner and community (2026-10-19).
    /// Being a community, and its rules and updates channels: at the end of
    /// the safety page, where its requirements are.
    fn community(&self, options: &rune_model::ServerOptions) -> Vec<Element> {
        let look = self.look;
        let capable = self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.community);
        let needs = t(
            "Una comunidad necesita al menos verificación baja y revisar el contenido de todos.",
        );
        if !capable {
            return options
                .community
                .then(|| text(needs, 13.0, look.muted))
                .into_iter()
                .collect();
        }
        let toggle = self.abilities.toggle_community.reason();
        let channels = self.abilities.edit_community.reason();
        let mut out = vec![
            parts::divider(look),
            parts::field_label(t("Comunidad"), false, look),
            text(
                t("Tiene un canal de reglas y otro donde el servicio avisa novedades a la moderación. Al activarla, la verificación sube a baja si no tenía y se revisa el contenido de todos."),
                14.0,
                look.muted,
            ),
            parts::switch_row(
                "ss:community",
                t("Es una comunidad"),
                Some(toggle.unwrap_or(needs)),
                options.community,
                toggle.is_none(),
                look,
            ),
        ];
        if !options.community {
            return out;
        }
        out.extend([
            parts::field_label(t("Canal de reglas"), false, look),
            self.channel_pick(
                "rules",
                options.rules_channel,
                &[ChannelKind::Text],
                t("Elegí un canal"),
                t("Canal de reglas"),
                channels.is_none(),
            ),
            parts::field_label(t("Canal de novedades de la comunidad"), false, look),
            text(
                t("Donde el servicio avisa a la moderación de cambios y novedades."),
                14.0,
                look.muted,
            ),
            self.channel_pick(
                "updates",
                options.updates_channel,
                &[ChannelKind::Text],
                t("Elegí un canal"),
                t("Canal de novedades de la comunidad"),
                channels.is_none(),
            ),
        ]);
        if !options.community_ready() {
            out.push(text(
                t("Falta algo de lo que una comunidad necesita: los dos canales, verificación y revisar el contenido de todos."),
                13.0,
                look.danger,
            ));
        }
        out
    }

    /// The banner the card shows: the one chosen, else the server's own
    /// unless it is being taken away.
    pub(super) fn shown_banner(&self) -> Option<limn::RasterImage> {
        match &self.settings.profile.banner {
            Some(Some(_)) => self.settings.rest.banner_preview.clone(),
            Some(None) => None,
            None => self.ui.image(&self.server.options.banner),
        }
    }

    /// The banner's buttons on the profile, where the provider has banners,
    /// and what the server's boosts say of moving ones.
    fn banner_fields(&self) -> Vec<Element> {
        let look = self.look;
        if !self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.server_banner)
        {
            return Vec::new();
        }
        let denied = self.abilities.edit_banner.reason();
        let can = denied.is_none();
        let has = match &self.settings.profile.banner {
            Some(banner) => banner.is_some(),
            None => self.server.options.banner.is_some(),
        };
        let mut fields = vec![
            parts::divider(look),
            parts::field_label(t("Banner"), false, look),
            text(
                if self.simulated_provider() {
                    t("Sobre la lista de canales. En la simulación se guarda su nombre, no la imagen.")
                } else {
                    t("Sobre la lista de canales, en 16:9; se recorta al centro.")
                },
                14.0,
                look.muted,
            ),
        ];
        match denied {
            Some(reason) => fields.push(parts::denied(reason, look)),
            None => fields.extend(still_note(self.abilities.animated_banner.reason(), look)),
        }
        fields.push(
            Element::row(vec![
                Button::new("ss:banner:pick", t("Cambiar banner"), look.palette())
                    .icon(Icon::Image)
                    .variant(ButtonVariant::Outline)
                    .enabled(can && self.settings.rest.picking.is_none())
                    .render(),
                Button::new("ss:banner:remove", t("Quitar banner"), look.palette())
                    .variant(ButtonVariant::Ghost)
                    .enabled(can && has)
                    .render(),
            ])
            .gap(10.0),
        );
        fields
    }

    /// The page that reorders channels and moves them between categories.
    pub(super) fn channels(&self) -> Element {
        let look = self.look;
        let rest = &self.settings.rest;
        let denied = self.abilities.reorder_channels.reason();
        let can = denied.is_none();
        let slots = rest::shown_layout(self.settings, self.store);
        let mut children = vec![parts::page_title(
            t("Canales"),
            Some(t("Arrastrá los canales para ordenarlos o pasarlos a otra categoría; también desde el menú de cada uno. Se guarda todo junto.")),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        let categories: Vec<(ChannelId, String)> = slots
            .iter()
            .filter(|slot| slot.category)
            .filter_map(|slot| Some((slot.channel, self.store.channel(slot.channel)?.name.clone())))
            .collect();
        let dragged_at = rest
            .dragging
            .and_then(|dragged| slots.iter().position(|slot| slot.channel == dragged));
        let rows: Vec<Element> = slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let channel = self.store.channel(slot.channel)?;
                let id = slot.channel;
                let mut menu = vec![
                    MenuAction::new(format!("ss:chan:{id}:up"), t("Subir")).icon(Icon::ArrowUp),
                    MenuAction::new(format!("ss:chan:{id}:down"), t("Bajar")).icon(Icon::ArrowDown),
                ];
                if !slot.category {
                    menu.push(
                        MenuAction::new(
                            format!("ss:chan:{id}:to:none"),
                            t("Sacar de la categoría"),
                        )
                        .denied(
                            slot.parent
                                .is_none()
                                .then(|| t("Ya está fuera de las categorías.").to_owned()),
                        )
                        .separated(),
                    );
                    for (category, name) in &categories {
                        menu.push(
                            MenuAction::new(
                                format!("ss:chan:{id}:to:{category}"),
                                tr!("Mover a {category}", category = name.to_uppercase()),
                            )
                            .denied(
                                (slot.parent == Some(*category))
                                    .then(|| t("Ya está en esa categoría.").to_owned()),
                            ),
                        );
                    }
                }
                let drop = match (dragged_at, rest.drop_over) {
                    (Some(from), Some(over)) if over == id && from != index => {
                        Some(if from > index {
                            DropEdge::Above
                        } else {
                            DropEdge::Below
                        })
                    }
                    _ => None,
                };
                Some(admin_parts::channel_slot(
                    &ChannelSlot {
                        channel: id,
                        name: channel.name.clone(),
                        category: slot.category,
                        nested: slot.parent.is_some(),
                        glyph: match channel.kind {
                            ChannelKind::Category => Icon::ChevronDown,
                            ChannelKind::Forum => Icon::Chat,
                            kind if kind.is_voice() => Icon::Volume,
                            _ => Icon::Hash,
                        },
                        movable: can,
                        menu: if can { menu } else { Vec::new() },
                        menu_open: rest.channel_menu == Some(id),
                        drop,
                    },
                    look,
                ))
            })
            .collect();
        if rows.is_empty() {
            children.push(text(t("Este servidor no tiene canales."), 14.0, look.muted));
        } else {
            children.push(Element::column(rows));
        }
        parts::page("ss:scroll:channels", self.metrics.content, children)
    }

    /// The picture and the traits of the profile's form.
    pub(super) fn identity_fields(&self) -> Vec<Element> {
        let look = self.look;
        let rest = &self.settings.rest;
        let capable = self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.server_identity);
        if !capable {
            return Vec::new();
        }
        let denied = self.abilities.edit_identity.reason();
        let can = denied.is_none();
        let has_icon = match &self.settings.profile.icon {
            Some(icon) => icon.is_some(),
            None => self.server.icon.is_some(),
        };
        let mut fields = vec![
            parts::divider(look),
            parts::field_label(t("Ícono"), false, look),
            text(
                if self.simulated_provider() {
                    t("La imagen del servidor. En la simulación se guarda su nombre, no la imagen.")
                } else {
                    t("La imagen del servidor, cuadrada; se recorta al centro.")
                },
                14.0,
                look.muted,
            ),
            Element::row(vec![
                Button::new("ss:icon:pick", t("Cambiar ícono"), look.palette())
                    .icon(Icon::Image)
                    .variant(ButtonVariant::Outline)
                    .enabled(can && rest.picking.is_none())
                    .tooltip(denied.unwrap_or(t("Elegir una imagen")))
                    .render(),
                Button::new("ss:icon:remove", t("Quitar ícono"), look.palette())
                    .variant(ButtonVariant::Ghost)
                    .enabled(can && has_icon)
                    .render(),
            ])
            .gap(10.0),
        ];
        // Icon, banner and community (2026-10-19).
        if can {
            fields.extend(still_note(self.abilities.animated_icon.reason(), look));
        }
        fields.extend(self.banner_fields());
        fields.extend([
            parts::divider(look),
            parts::field_label(t("Rasgos"), false, look),
            text(
                t("Hasta cinco cosas que lo definen, con un emoji o «:nombre:» de uno del servidor. Se muestran en su perfil."),
                14.0,
                look.muted,
            ),
        ]);
        let loaded = self.server.options.traits.is_some() || self.settings.profile.traits.is_some();
        if !loaded {
            if rest.profile_failed {
                fields.push(
                    Element::row(vec![
                        text(t("No se pudieron leer los rasgos."), 14.0, look.muted).centered_y(),
                        Button::new("ss:profile:retry", t("Reintentar"), look.palette())
                            .variant(ButtonVariant::Link)
                            .render(),
                    ])
                    .gap(8.0),
                );
            } else {
                fields.push(line(t("Cargando…"), 14.0, look.muted));
            }
            return fields;
        }
        let traits = rest::shown_traits(self.settings, self.store);
        for (index, found) in traits.iter().enumerate() {
            fields.push(admin_parts::trait_row(
                index,
                &rest::trait_emoji(found),
                &found.label,
                can,
                look,
            ));
        }
        fields.push(
            Button::new("ss:trait:add", t("Agregar rasgo"), look.palette())
                .icon(Icon::Add)
                .variant(ButtonVariant::Ghost)
                .button_size(ButtonSize::Small)
                .enabled(can && traits.len() < MAX_TRAITS)
                .render(),
        );
        fields
    }

    /// What the server card shows of the traits: each one's emoji and label.
    pub(super) fn trait_chips(&self) -> Vec<String> {
        rest::shown_traits(self.settings, self.store)
            .iter()
            .filter(|found| !found.label.trim().is_empty())
            .map(|found| match rest::trait_emoji(found) {
                emoji if emoji.is_empty() => found.label.trim().to_owned(),
                emoji => format!("{emoji} {}", found.label.trim()),
            })
            .collect()
    }

    /// A role's picture, emoji and gradient, in its editor.
    pub(super) fn role_style_fields(&self, role: &Role, can: bool) -> Vec<Element> {
        let look = self.look;
        let rest = &self.settings.rest;
        if !self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.role_styles)
        {
            return Vec::new();
        }
        let icons = self.abilities.role_icons.reason();
        let gradients = self.abilities.role_gradients.reason();
        let preview = rest
            .role_icon_preview
            .as_ref()
            .filter(|(of, _)| *of == role.id)
            .map(|(_, preview)| preview.clone());
        let mark = match (&preview, &role.style.emoji, &role.style.icon) {
            (Some(preview), _, _) => {
                Element::raster_image(preview.clone()).size(Some(32.0), Some(32.0))
            }
            (None, Some(emoji), _) => line(emoji.clone(), 24.0, look.text),
            (None, None, Some(_)) => line(t("Imagen"), 13.0, look.muted),
            (None, None, None) => line(t("Sin ícono"), 13.0, look.muted),
        };
        let gradient_can = can && gradients.is_none();
        let mut fields = vec![parts::divider(look)];
        fields.push(parts::switch_row(
            "ss:gradient",
            t("Degradé"),
            Some(gradients.unwrap_or(t(
                "El nombre pasa del color del rol a uno o dos colores más.",
            ))),
            !role.style.gradient.is_empty(),
            gradient_can,
            look,
        ));
        if let Some(second) = role.style.gradient.first() {
            // How names look with it.
            let ink = self.role_color(role);
            let stops: Vec<limn::Color> = role
                .style
                .gradient
                .iter()
                .map(|color| look.ink(*color))
                .collect();
            fields.push(rune_components::identity::inked(
                rune_components::strong_line(role.name.clone(), 16.0, ink),
                ink,
                &stops,
            ));
            let custom = |which: u8| {
                rest.gradient_hex
                    .as_ref()
                    .filter(|(of, _)| *of == which)
                    .map(|(_, hex)| hex.clone())
            };
            let narrow = self.editor_width() < parts::swatches_width(true);
            fields.push(parts::field_label(t("Segundo color"), false, look));
            fields.push(parts::swatches(
                &Swatches {
                    prefix: "ss:gradient2".into(),
                    chosen: Some(*second),
                    default: None,
                    custom: custom(2),
                    enabled: gradient_can,
                    narrow,
                    picked: crate::color_picker::picked(self.ui, "ss:gradient2"),
                },
                look,
            ));
            fields.push(parts::field_label(
                t("Tercer color (opcional)"),
                false,
                look,
            ));
            fields.push(parts::swatches(
                &Swatches {
                    prefix: "ss:gradient3".into(),
                    chosen: role.style.gradient.get(1).copied(),
                    // The default swatch is «none»: two colors only.
                    default: Some(*second),
                    custom: custom(3),
                    enabled: gradient_can,
                    narrow,
                    picked: crate::color_picker::picked(self.ui, "ss:gradient3"),
                },
                look,
            ));
        }
        let icon_can = can && icons.is_none();
        fields.extend([
            parts::divider(look),
            parts::field_label(t("Ícono del rol"), false, look),
            text(
                t("Una imagen o un emoji al lado del nombre de quienes lo tienen."),
                14.0,
                look.muted,
            ),
        ]);
        if let Some(reason) = icons {
            fields.push(parts::denied(reason, look));
        }
        fields.push(
            Element::row(vec![
                mark.centered_y(),
                Button::new("ss:role-icon:pick", t("Subir imagen"), look.palette())
                    .icon(Icon::Image)
                    .variant(ButtonVariant::Outline)
                    .enabled(icon_can && rest.picking.is_none())
                    .render()
                    .centered_y(),
                TextField::new(
                    ROLE_EMOJI,
                    role.style.emoji.clone().unwrap_or_default(),
                    t("o un emoji: 🛡️"),
                    look.palette(),
                )
                .height(FIELD_HEIGHT)
                .accessibility_label(t("Emoji del rol"))
                .enabled(icon_can)
                .render()
                .size(Some(150.0), None),
                Button::new("ss:role-icon:remove", t("Quitar"), look.palette())
                    .variant(ButtonVariant::Ghost)
                    .enabled(icon_can && (role.style.has_mark() || preview.is_some()))
                    .render()
                    .centered_y(),
            ])
            .gap(10.0),
        );
        fields
    }

    /// How many have `role`: as the provider counted, else as known here.
    pub(super) fn role_members(&self, role: rune_model::RoleId) -> usize {
        self.store
            .role_count(self.server.id, role)
            .map_or_else(|| self.members_with(role), |count| count as usize)
    }

    /// The order and filter of the members page, when the provider has them.
    pub(super) fn member_tools(&self) -> Option<Element> {
        if !self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.member_queries)
        {
            return None;
        }
        let query = self.settings.rest.query;
        let can = self.abilities.member_queries.allowed();
        let order = self.pick(
            "member-order",
            order_label(query.order).into(),
            MemberOrder::ALL
                .iter()
                .enumerate()
                .map(|(index, order)| {
                    (
                        index.to_string(),
                        order_label(*order).into(),
                        *order == query.order,
                    )
                })
                .collect(),
            t("Ordenar miembros"),
            can,
        );
        let recent = Toggle::new(
            "ss:members:recent",
            t("Últimos 7 días"),
            query.recent,
            self.look.palette(),
        )
        .enabled(can)
        .accessibility_label(t("Solo quienes entraron en los últimos 7 días"))
        .render();
        // One under the other in a narrow window (2026-10-19).
        Some(if self.metrics.content >= 380.0 + 10.0 + 170.0 {
            Element::row(vec![order, recent.centered_y()]).gap(10.0)
        } else {
            Element::column(vec![order, Element::row(vec![recent])]).gap(8.0)
        })
    }

    /// The members in the order the provider listed them for an order or
    /// a filter chosen here, once it did; `None` to list everyone known by
    /// name, as without a choice.
    pub(super) fn ordered_members(&self) -> Option<Vec<UserId>> {
        let query = self.settings.rest.query;
        if query == rune_model::MemberQuery::default() {
            return None;
        }
        let pages = self.store.member_pages(self.server.id)?;
        (self.abilities.member_queries.allowed() && pages.query == query)
            .then(|| pages.listed.clone())
    }

    /// How `user` joined, as the members page says it.
    pub(super) fn joined_note(&self, user: UserId) -> Option<String> {
        let method = self.store.member_pages(self.server.id)?.joins.get(&user)?;
        let said = match method {
            JoinMethod::Invite(Some(code)) => tr!("entró con la invitación {code}", code),
            JoinMethod::Invite(None) => t("entró con una invitación").into(),
            JoinMethod::VanityLink => t("entró con el enlace propio").into(),
            JoinMethod::Bot => t("lo agregó un bot").into(),
            JoinMethod::Integration => t("entró por una integración").into(),
            JoinMethod::Discovery => t("lo encontró en el directorio").into(),
            JoinMethod::Hub => t("entró por un centro educativo").into(),
            JoinMethod::App => t("entró por una aplicación").into(),
            JoinMethod::Other => return None,
        };
        Some(if self.simulated_provider() {
            tr!("{said} (simulado)", said)
        } else {
            said
        })
    }

    /// The button that pauses or resumes invitations, and the notice that
    /// they are paused.
    pub(super) fn invites_pause(&self) -> Vec<Element> {
        let look = self.look;
        if !self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.pause_invites)
        {
            return Vec::new();
        }
        let paused = self.server.options.invites_paused;
        let denied = self.abilities.pause_invites.reason();
        let button = Button::new(
            if paused {
                "ss:invites:resume"
            } else {
                "ss:invites:pause"
            },
            if paused {
                t("Reanudar invitaciones")
            } else {
                t("Pausar invitaciones")
            },
            look.palette(),
        )
        .icon(if paused { Icon::Play } else { Icon::Pause })
        .variant(ButtonVariant::Outline)
        .enabled(denied.is_none())
        .tooltip(denied.unwrap_or(if paused {
            t("Que se pueda volver a entrar con invitaciones")
        } else {
            t("Que nadie nuevo entre con invitaciones por ahora")
        }))
        .render();
        let mut out = vec![button];
        if paused {
            out.push(parts::denied(
                t("Las invitaciones están pausadas: nadie nuevo puede entrar con ellas hasta que se reanuden."),
                look,
            ));
        }
        out
    }
}

fn afk_label(secs: u32) -> String {
    match secs {
        60 => t("1 minuto").into(),
        3600 => t("1 hora").into(),
        secs if secs % 60 == 0 => tr!("{minutes} minutos", minutes = secs / 60),
        secs => tr!("{seconds} segundos", seconds = secs),
    }
}

fn verification_label(level: Verification) -> &'static str {
    match level {
        Verification::None => t("Ninguno: sin restricciones"),
        Verification::Low => t("Bajo: correo verificado"),
        Verification::Medium => t("Medio: y una cuenta de más de 5 minutos"),
        Verification::High => t("Alto: y 10 minutos en el servidor"),
        Verification::Highest => t("Máximo: y un teléfono verificado"),
    }
}

fn scan_label(scan: MediaScan) -> &'static str {
    match scan {
        MediaScan::Off => t("No revisar nada"),
        MediaScan::WithoutRoles => t("Revisar a quienes no tienen rol"),
        MediaScan::Everyone => t("Revisar a todos"),
    }
}

fn order_label(order: MemberOrder) -> &'static str {
    match order {
        MemberOrder::NewestMembers => t("Últimos en entrar"),
        MemberOrder::OldestMembers => t("Primeros en entrar"),
        MemberOrder::NewestAccounts => t("Cuentas más nuevas"),
        MemberOrder::OldestAccounts => t("Cuentas más antiguas"),
    }
}

// Icon, banner and community (2026-10-19).
/// Why a moving picture chosen would be used still, when it would.
fn still_note(reason: Option<&str>, look: &rune_components::Look) -> Option<Element> {
    reason.map(|reason| {
        text(
            tr!(
                "{reason}: un GIF se usa quieto.",
                reason = reason.trim_end_matches('.')
            ),
            13.0,
            look.muted,
        )
    })
}
