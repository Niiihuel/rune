//! Presence, identity and expressions in the simulation (P5): made-up
//! activities with their pictures, times and platforms; marks around some
//! names; role emoji, pictures and gradients; the account's perks; two
//! standard sticker packs; and a GIF search that answers from the query.
//! Pictures are named on `simulacion.invalid`, which hosts nothing (a
//! capture draws them itself), and every name says it is simulated. The
//! same input gets the same answer.

use rune_i18n::{es, t, tr};
use rune_model::{
    Activity, ActivityKind, ChannelId, Identity, ImageKind, Message, Nameplate, Perks, Platforms,
    RemoteImage, Rgb, RoleStyle, ServerId, ServerTag, Sticker, StickerId, StickerPack, Timestamp,
    UserId,
};

use super::{Session, World, ME};
use crate::{
    Command, Event, Failure, GifAttribution, GifCategory, GifItem, GifPage, GifQuery, GifResult,
};

/// What the simulated plan unlocks the rest with: kept in Spanish and
/// translated where shown, so it follows a change of language.
const UPGRADE: &str = es("Plus (simulado)");
/// GIFs on a simulated page, and the last offset answered.
const GIF_PAGE: u32 = 12;
const GIF_END: u32 = 48;
/// What is trending to search for.
const TRENDING: [&str; 6] = ["aplausos", "gatos", "risa", "sí", "no", "baile"];

/// A picture the simulation names and nobody hosts.
pub(super) fn picture(path: &str) -> RemoteImage {
    shaped(path, ImageKind::Artwork)
}

/// One kept at its own shape: a wide nameplate, a GIF.
fn shaped(path: &str, kind: ImageKind) -> RemoteImage {
    RemoteImage {
        url: format!("https://simulacion.invalid/{path}.png"),
        kind,
    }
}

/// What the made-up person `index` of `PEOPLE` is doing at `now`.
pub(super) fn activities(person: usize, now: Timestamp) -> Vec<Activity> {
    let at = |minutes: i64| Some(now.plus_minutes(-minutes));
    let activity = |kind, name: &str, detail: Option<&str>, state: Option<&str>| Activity {
        kind,
        name: name.into(),
        detail: detail.map(str::to_owned),
        state: state.map(str::to_owned),
        image: None,
        small_image: None,
        started: None,
        ends: None,
        application: None,
    };
    match person {
        7 => vec![
            Activity {
                image: Some(picture("actividades/fabrica")),
                small_image: Some(picture("actividades/engranaje")),
                started: at(47),
                ..activity(
                    ActivityKind::Playing,
                    "Factorio",
                    Some("Construyendo una fábrica de circuitos"),
                    Some("Partida multijugador (2 de 4)"),
                )
            },
            radio(now),
        ],
        11 => vec![radio(now)],
        4 => vec![Activity {
            started: at(12),
            ..activity(ActivityKind::Watching, "Charlas de RustConf", None, None)
        }],
        16 => vec![Activity {
            image: Some(picture("actividades/estrella")),
            started: at(95),
            ..activity(
                ActivityKind::Competing,
                "Advent of Code",
                Some("Día 12"),
                Some("Puesto 34 de 120"),
            )
        }],
        _ => Vec::new(),
    }
}

/// A track with its length, a minute and a quarter in.
fn radio(now: Timestamp) -> Activity {
    let started = Timestamp(now.0 - 75_000);
    Activity {
        kind: ActivityKind::Listening,
        name: "Radio Nocturna".into(),
        detail: Some("Jazz para programar".into()),
        state: Some("Trío Ferroso".into()),
        image: Some(picture("actividades/disco")),
        small_image: None,
        started: Some(started),
        ends: Some(Timestamp(started.0 + 210_000)),
        application: None,
    }
}

/// Where the made-up person `index` is connected from.
pub(super) fn platforms(person: usize) -> Platforms {
    match person % 4 {
        0 => Platforms {
            desktop: true,
            ..Default::default()
        },
        1 => Platforms {
            mobile: true,
            ..Default::default()
        },
        2 => Platforms {
            desktop: true,
            mobile: true,
            web: false,
        },
        _ => Platforms {
            web: true,
            ..Default::default()
        },
    }
}

/// The account's simulated plan: emoji anywhere, stickers only in their
/// own server, a banner and a profile in each server, but an avatar of the
/// account that does not move: what the plan lacks is said, as in a
/// service.
pub(super) fn perks() -> Perks {
    Perks {
        emoji_anywhere: true,
        animated_emoji: true,
        stickers_anywhere: false,
        upgrade: Some(UPGRADE.into()),
        animated_avatar: false,
        profile_banner: true,
        server_profiles: true,
    }
}

/// Two packs anyone can send, the second of animated vector (Lottie)
/// stickers, as a service's own are.
fn packs() -> Vec<StickerPack> {
    let sticker = |id: u64, name: &str, raster: bool| Sticker {
        id: StickerId(id),
        name: name.into(),
        image: Some(if raster {
            picture(&format!("stickers/{id}"))
        } else {
            RemoteImage {
                url: format!("https://simulacion.invalid/stickers/{id}.json"),
                kind: ImageKind::Artwork,
            }
        }),
        available: true,
    };
    vec![
        StickerPack {
            id: 9_300,
            // Kept in Spanish, translated where shown, as the plan.
            name: es("Saludos (simulado)").into(),
            stickers: vec![
                sticker(9_301, "Hola", true),
                sticker(9_302, "Chau", true),
                sticker(9_303, "Gracias", true),
                sticker(9_304, "Buenas noches", true),
            ],
        },
        StickerPack {
            id: 9_310,
            name: es("Cangrejos animados (simulado)").into(),
            stickers: vec![
                sticker(9_311, "Cangrejo bailando", false),
                sticker(9_312, "Cangrejo saludando", false),
            ],
        },
    ]
}

/// A page of made-up GIFs for `query` from `offset`: the same each time.
fn gifs(query: &GifQuery, offset: u32) -> GifPage {
    let (slug, title) = match query {
        GifQuery::Trending => ("tendencias".to_owned(), "Tendencia".to_owned()),
        GifQuery::Search(text) => {
            let slug: String = text
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect();
            (slug, text.trim().to_owned())
        }
    };
    let items = (offset..(offset + GIF_PAGE).min(GIF_END))
        .map(|index| GifItem {
            id: format!("{slug}-{index}"),
            title: format!("{title} {} {}", index + 1, t("(simulado)")),
            creator: None,
            url: format!("https://simulacion.invalid/gifs/{slug}-{index}.gif"),
            preview: shaped(
                &format!("gifs/{slug}-{index}"),
                ImageKind::AttachmentPreview,
            ),
            thumbnail: None,
        })
        .collect();
    let next = offset + GIF_PAGE;
    GifPage {
        items,
        next_offset: (next < GIF_END).then_some(next),
        attribution: GifAttribution {
            label: t("GIF simulados").into(),
            url: String::new(),
            image: None,
        },
        categories: match query {
            GifQuery::Trending if offset == 0 => TRENDING
                .iter()
                .map(|name| GifCategory {
                    name: (*name).into(),
                    image: Some(shaped(
                        &format!("gifs/categoria-{name}"),
                        ImageKind::AttachmentPreview,
                    )),
                })
                .collect(),
            _ => Vec::new(),
        },
    }
}

/// What to search for: completions of `query`, or what is trending.
fn suggestions(query: &str) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        return TRENDING.iter().map(|term| (*term).to_owned()).collect();
    }
    ["", " gracioso", " animado", "s"]
        .iter()
        .map(|end| format!("{query}{end}"))
        .collect()
}

/// The badges of a made-up person's profile: a few have some.
pub(super) fn badges(user: UserId) -> Vec<rune_model::Badge> {
    let badge = |id: &str, description: &str| rune_model::Badge {
        id: id.into(),
        description: format!("{description} {}", t("(simulada)")),
        icon: Some(picture(&format!("insignias/{id}"))),
        link: Some(format!("https://simulacion.invalid/insignias/{id}")),
    };
    match user.0 {
        10 => vec![
            badge("pionera", "Pionera del taller"),
            badge("mejora", "Mejora del servidor desde 2024"),
        ],
        17 => vec![badge("fabrica", "Constructora de fábricas")],
        _ => Vec::new(),
    }
}

/// Two server emoji, simulated: one that moves and one that does not
/// (2026-10-17).
pub(super) fn server_emoji() -> [rune_model::TextPicture; 2] {
    let emoji = |name: &str, id: u64, animated: bool| rune_model::TextPicture {
        token: format!("<{}:{name}:{id}>", if animated { "a" } else { "" }),
        name: name.into(),
        image: picture(&format!("emoji/{name}")),
    };
    [
        emoji("chispa", 900_001, true),
        emoji("engranaje", 900_002, false),
    ]
}

/// The simulated server emoji `content` names, as its references.
pub(super) fn emoji_references(content: &str) -> Vec<rune_model::TextReference> {
    server_emoji()
        .into_iter()
        .filter(|emoji| content.contains(&emoji.token))
        .map(|emoji| rune_model::TextReference {
            target: rune_model::TextTarget::Emoji {
                // Its id, the number its token ends with.
                id: emoji
                    .token
                    .trim_end_matches('>')
                    .rsplit(':')
                    .next()
                    .and_then(|id| id.parse().ok())
                    .map(rune_model::EmojiId),
                name: emoji.name,
                // The same picture, asked larger for a message of emoji alone.
                jumbo: Some(RemoteImage {
                    url: format!("{}?size=96", emoji.image.url),
                    kind: emoji.image.kind,
                }),
                image: Some(emoji.image),
            },
            source: emoji.token,
        })
        .collect()
}

/// The simulated server emoji `text` names, as pictures.
pub(super) fn pictures_in(text: &str) -> Vec<rune_model::TextPicture> {
    server_emoji()
        .into_iter()
        .filter(|emoji| text.contains(&emoji.token))
        .collect()
}

/// A server's emoji before the custom status of a made-up person.
pub(super) fn status_emoji(person: usize) -> Option<rune_model::TextPicture> {
    let [chispa, _] = server_emoji();
    (person == 0).then_some(chispa)
}

/// What a made-up person writes about themselves with server emoji in it,
/// as a real bio does, and those emoji, simulated (2026-10-17).
pub(super) fn bio(user: UserId) -> Option<(String, Vec<rune_model::TextPicture>)> {
    (user.0 == 10).then(|| {
        let pictures = server_emoji().to_vec();
        let text = format!(
            "{} {}{} {}",
            t("Armo fábricas con limn"),
            pictures[0].token,
            pictures[1].token,
            t("(emoji simulados)")
        );
        (text, pictures)
    })
}

/// The effect over a made-up person's card: sparks over the whole canvas,
/// and their still.
pub(super) fn effect(user: UserId) -> Option<rune_model::ProfileEffect> {
    (user.0 == 10).then(|| rune_model::ProfileEffect {
        width: 450,
        height: 880,
        layers: vec![rune_model::EffectLayer {
            image: shaped("efectos/chispas", ImageKind::AttachmentPreview),
            x: 0,
            y: 0,
            width: 450,
            height: 880,
            start: 0,
            duration: 0,
            looping: true,
            loop_delay: 0,
        }],
        still: Some(shaped(
            "efectos/chispas-quietas",
            ImageKind::AttachmentPreview,
        )),
    })
}

/// The accounts elsewhere a made-up person shows, linked nowhere real.
pub(super) fn connections(user: UserId) -> Vec<rune_model::Connection> {
    let account = |service: &str, label: &str, name: &str, verified: bool| rune_model::Connection {
        service: service.into(),
        label: label.into(),
        name: name.into(),
        verified,
        link: Some(format!("https://simulacion.invalid/{service}/{name}")),
    };
    match user.0 {
        10 => vec![
            account("github", "GitHub", "ada-simulada", true),
            account("domain", t("Dominio"), "ada.simulacion.invalid", false),
        ],
        17 => vec![account("steam", "Steam", "fabrica-simulada", false)],
        _ => Vec::new(),
    }
}

impl World {
    /// Dress some people, roles and the account: call once everything exists.
    pub(super) fn add_identities(&mut self) {
        let tag = |server: u64, tag: &str| ServerTag {
            server: ServerId(server),
            tag: tag.into(),
            badge: Some(picture(&format!("etiquetas/{server}"))),
        };
        for user in &mut self.users {
            let person = user.id.0.checked_sub(10).map(|index| index as usize);
            user.identity = match person {
                Some(0) => Identity {
                    decoration: Some(picture("decoraciones/hojas")),
                    nameplate: Some(Nameplate {
                        image: shaped("placas/aurora", ImageKind::AttachmentPreview),
                        colors: Some((Rgb(0x2B_6C_B0), Rgb(0x9F_7A_EA))),
                        label: Some(format!("Aurora {}", t("(simulada)"))),
                        // It moves under the pointer (2026-10-18).
                        animated: Some(shaped(
                            "placas/aurora-animada",
                            ImageKind::AttachmentPreview,
                        )),
                    }),
                    tag: Some(tag(1, "RUST")),
                },
                Some(2) => Identity {
                    tag: Some(tag(2, "LIMN")),
                    ..Default::default()
                },
                Some(5) => Identity {
                    decoration: Some(picture("decoraciones/estrellas")),
                    ..Default::default()
                },
                Some(7) => Identity {
                    nameplate: Some(Nameplate {
                        image: shaped("placas/bosque", ImageKind::AttachmentPreview),
                        colors: Some((Rgb(0x27_6E_4B), Rgb(0x8B_C3_4A))),
                        label: Some(format!("Bosque {}", t("(simulado)"))),
                        animated: None,
                    }),
                    ..Default::default()
                },
                _ if user.id == ME => Identity {
                    tag: Some(tag(2, "LIMN")),
                    ..Default::default()
                },
                _ => continue,
            };
        }
        for server in &mut self.servers {
            for role in &mut server.roles {
                let kind = role.id.0.checked_sub(server.id.0 * 10);
                role.style = match kind {
                    Some(1) => RoleStyle {
                        emoji: Some("🛡️".into()),
                        gradient: vec![Rgb(0xE5_C0_7B)],
                        ..Default::default()
                    },
                    Some(2) => RoleStyle {
                        icon: Some(picture(&format!("roles/{}", role.id))),
                        gradient: vec![Rgb(0xC6_78_DD), Rgb(0x56_B6_C2)],
                        ..Default::default()
                    },
                    _ => continue,
                };
            }
        }
        // Someone dresses differently in the account's own server.
        let owned = ServerId(2);
        if let Some(member) = self
            .members
            .iter_mut()
            .find(|member| member.server == owned && member.user == UserId(15))
        {
            member.identity.decoration = Some(picture("decoraciones/laboratorio"));
        }
    }

    /// A message of the account carrying `sticker`, sent once per `nonce`.
    fn post_sticker(
        &mut self,
        nonce: u64,
        channel: ChannelId,
        sticker: Sticker,
        now: Timestamp,
    ) -> Option<Message> {
        let mut message = self.post_once(nonce, channel, String::new(), None, now)?;
        if message.stickers.is_empty() {
            message.stickers = vec![sticker];
            let stored = self.sims.get_mut(&channel)?.extra.iter_mut();
            if let Some(stored) = stored.rev().find(|kept| kept.id == message.id) {
                stored.stickers = message.stickers.clone();
            }
        }
        Some(message)
    }

    /// The sticker `id` and why the account may not send it in `channel`.
    fn sticker_for(&self, channel: ChannelId, id: StickerId) -> Result<Sticker, String> {
        let here = self.channel(channel).ok_or(t("el canal no existe"))?.server;
        if let Some(sticker) = packs()
            .into_iter()
            .flat_map(|pack| pack.stickers)
            .find(|sticker| sticker.id == id)
        {
            return Ok(sticker);
        }
        let (server, sticker) = self
            .servers
            .iter()
            .find_map(|server| {
                server
                    .stickers
                    .iter()
                    .find(|sticker| sticker.id == id)
                    .map(|sticker| (server.id, sticker.clone()))
            })
            .ok_or(t("ese sticker no existe (simulado)"))?;
        // Emoji and states (2026-10-18): the channel's permission first.
        if Some(server) != here
            && here.is_some()
            && !self.channel(channel).is_some_and(|channel| {
                channel
                    .permissions
                    .contains(rune_model::Permissions::EXTERNAL_STICKERS)
            })
        {
            return Err(t("no tenés permiso para usar stickers de otros servidores acá").into());
        }
        if Some(server) != here && !perks().stickers_anywhere {
            return Err(tr!(
                "hace falta {upgrade} para usar stickers de otros servidores",
                upgrade = t(UPGRADE)
            ));
        }
        Ok(sticker)
    }
}

impl Session {
    /// Carry out a command of P5.
    pub(super) fn identity(&mut self, command: Command) {
        let latency = self.config.latency;
        let now = (self.config.clock)();
        match command {
            Command::LoadStickerPacks => {
                self.later(latency, |_| vec![Event::StickerPacks(packs())])
            }
            Command::SearchGifs {
                request,
                query,
                offset,
            } => self.later(latency, move |_| {
                vec![Event::GifResults(GifResult {
                    request,
                    result: Ok(gifs(&query, offset)),
                    query,
                    offset,
                })]
            }),
            Command::SuggestGifs { request, query } => self.later(latency / 2, move |_| {
                vec![Event::GifSuggestions {
                    request,
                    terms: suggestions(&query),
                    query,
                }]
            }),
            Command::SendSticker {
                channel,
                sticker,
                nonce,
            } => self.later(latency, move |world| {
                let failure = Failure::Send { channel, nonce };
                let posted = world.sticker_for(channel, sticker).and_then(|sticker| {
                    world
                        .post_sticker(nonce, channel, sticker, now)
                        .ok_or_else(|| t("el canal no existe").to_owned())
                });
                match posted {
                    Ok(message) => vec![Event::MessageCreated {
                        message,
                        nonce: Some(nonce),
                    }],
                    Err(reason) => vec![Event::Failed {
                        failure,
                        reason,
                        ambiguous: false,
                    }],
                }
            }),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gifs_and_suggestions_answer_the_same_each_time() {
        let page = gifs(&GifQuery::Trending, 0);
        assert_eq!(page.items.len(), GIF_PAGE as usize);
        assert_eq!(page.categories.len(), TRENDING.len());
        assert_eq!(page, gifs(&GifQuery::Trending, 0));
        let last = gifs(&GifQuery::Search("Gato feliz".into()), 36);
        assert_eq!(last.next_offset, None, "the end");
        assert!(last.items[0].url.contains("gato-feliz-36"));
        assert!(last.categories.is_empty());
        assert_eq!(suggestions("  ")[0], "aplausos");
        assert_eq!(suggestions("gato")[1], "gato gracioso");
        let radio = activities(11, Timestamp(1_000_000));
        assert!(radio[0].is_track());
        assert_eq!(activities(7, Timestamp(0)).len(), 2);
        assert!(platforms(1).mobile_only() && !platforms(2).mobile_only());
    }

    #[test]
    fn another_servers_sticker_asks_the_channels_permission_before_the_plan() {
        let world = World::build(&crate::mock::MockConfig::default());
        let named = |name: &str| {
            world
                .channels
                .iter()
                .find(|channel| channel.name == name)
                .unwrap()
                .id
        };
        let sticker = world
            .servers
            .iter()
            .find(|server| server.id == ServerId(2))
            .and_then(|server| server.stickers.first())
            .unwrap()
            .id;
        // #off-topic keeps another server's expressions out (2026-10-18).
        assert_eq!(
            world.sticker_for(named("off-topic"), sticker).unwrap_err(),
            t("no tenés permiso para usar stickers de otros servidores acá")
        );
        assert!(world
            .sticker_for(named("charla"), sticker)
            .unwrap_err()
            .starts_with("hace falta"));
        assert!(world.sticker_for(named("limn-core"), sticker).is_ok());
    }
}
