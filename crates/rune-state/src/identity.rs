//! Presence, identity and expressions (P5): what the account may use beyond
//! each conversation's own server, the standard sticker packs, and the
//! marks someone shows where they are seen.

use rune_i18n::{t, tr};
use rune_model::{
    ChannelId, CustomEmoji, Identity, Perks, Role, ServerId, Sticker, StickerId, StickerPack,
    UserId,
};
use rune_provider::Event;

use crate::{Applied, Permit, Store};

/// What this session knows of the account's perks and of the packs.
#[derive(Debug, Default)]
pub(crate) struct IdentityState {
    /// `None` until the provider says: only the conversation's own server's
    /// expressions are offered then.
    perks: Option<Perks>,
    /// `None` until read.
    packs: Option<Vec<StickerPack>>,
}

impl Store {
    pub(crate) fn apply_identity(&mut self, event: Event) -> Applied {
        match event {
            Event::Perks(perks) => {
                self.identity.perks = Some(perks);
                Applied::Perks
            }
            Event::StickerPacks(mut packs) => {
                packs.truncate(rune_model::identity::MAX_PACKS);
                for pack in &mut packs {
                    pack.stickers
                        .truncate(rune_model::identity::MAX_PACK_STICKERS);
                }
                self.identity.packs = Some(packs);
                Applied::StickerPacks
            }
            Event::GifResults(result) => Applied::GifResults(result),
            Event::GifSuggestions {
                request,
                query,
                terms,
            } => Applied::GifSuggestions {
                request,
                query,
                terms,
            },
            _ => Applied::Nothing,
        }
    }

    pub(crate) fn identity_reset(&mut self) {
        self.identity = IdentityState::default();
    }

    /// What the account may use beyond each conversation's own server, when
    /// its provider said.
    pub fn perks(&self) -> Option<&Perks> {
        self.identity.perks.as_ref()
    }

    /// The standard sticker packs, once read.
    pub fn sticker_packs(&self) -> Option<&[StickerPack]> {
        self.identity.packs.as_deref()
    }

    /// The marks `user` shows in `server`, or everywhere: their membership's
    /// over their account's.
    pub fn identity_in(&self, server: Option<ServerId>, user: UserId) -> Identity {
        let account = self
            .user(user)
            .map(|user| user.identity.clone())
            .unwrap_or_default();
        match server.and_then(|server| self.member_of(server, user)) {
            Some(member) => account.with(&member.identity),
            None => account,
        }
    }

    /// The highest of `user`'s roles in `server` that has a picture or an
    /// emoji, shown beside their name.
    pub fn role_mark(&self, user: UserId, server: ServerId) -> Option<&Role> {
        let member = self.member_of(server, user)?;
        self.server(server)?
            .roles
            .iter()
            .filter(|role| member.roles.contains(&role.id) && role.style.has_mark())
            .max_by_key(|role| role.position)
    }

    /// Why the account cannot use `emoji` in `channel`: another server's
    /// without the channel's permission (asked first, as the official
    /// client does), or for lack of its plan. Unknown perks keep emoji to
    /// their own server, as before perks were reported.
    pub fn emoji_perk(&self, channel: ChannelId, emoji: &CustomEmoji) -> Permit {
        let here = self.channel(channel).and_then(|channel| channel.server);
        if here != Some(emoji.server) {
            if let Some(denied) = self.external(channel, |a| a.external_emoji) {
                return denied;
            }
        }
        let Some(perks) = self.perks() else {
            return if here == Some(emoji.server) {
                Permit::Allowed
            } else {
                Permit::Denied(t("Usalo dentro de su servidor").into())
            };
        };
        // A plan's name may be a provider's text kept in Spanish.
        let unlock = |what: &str| match &perks.upgrade {
            Some(upgrade) => Permit::Denied(tr!(
                "Necesitas {upgrade} para usar {what}",
                upgrade = t(upgrade),
                what
            )),
            None => Permit::Denied(tr!("Tu cuenta no puede usar {what}", what)),
        };
        if emoji.animated && !perks.animated_emoji {
            unlock(t("emojis animados"))
        } else if here != Some(emoji.server) && !perks.emoji_anywhere {
            unlock(t("emojis de otros servidores aquí"))
        } else {
            Permit::Allowed
        }
    }

    /// Why the account cannot send a sticker of `server` (`None` for a
    /// standard pack's) in `channel`: without the channel's permission for
    /// another server's, or for lack of its plan.
    pub fn sticker_perk(&self, channel: ChannelId, server: Option<ServerId>) -> Permit {
        let Some(server) = server else {
            return Permit::Allowed;
        };
        let here = self.channel(channel).and_then(|channel| channel.server);
        if here == Some(server) {
            return Permit::Allowed;
        }
        if let Some(denied) = self.external(channel, |a| a.external_stickers) {
            return denied;
        }
        if self.perks().is_some_and(|perks| perks.stickers_anywhere) {
            return Permit::Allowed;
        }
        match self.perks().and_then(|perks| perks.upgrade.as_ref()) {
            Some(upgrade) => Permit::Denied(tr!(
                "Necesitas {upgrade} para usar stickers de otros servidores aquí",
                upgrade = t(upgrade)
            )),
            None => Permit::Denied(t("Usalo dentro de su servidor").into()),
        }
    }

    /// The denial of another server's expression in `channel`, as its
    /// ability (`which`) words it; `None` when allowed or unknown. Unknown
    /// from the local store: it may be older than the bit
    /// (`EXTERNAL_STICKERS` is read since 2026-10-18).
    fn external(
        &self,
        channel: ChannelId,
        which: impl Fn(crate::Abilities) -> Permit,
    ) -> Option<Permit> {
        self.abilities(channel)
            .map(which)
            .filter(|permit| !permit.allowed() && !self.from_cache)
    }

    /// A sticker the account may know of: of one of its servers, or of a
    /// standard pack, with the server it belongs to.
    pub fn known_sticker(&self, id: StickerId) -> Option<(Option<ServerId>, &Sticker)> {
        let in_server = self.servers().iter().find_map(|server| {
            server
                .stickers
                .iter()
                .find(|sticker| sticker.id == id)
                .map(|sticker| (Some(server.id), sticker))
        });
        in_server.or_else(|| {
            self.sticker_packs()?
                .iter()
                .flat_map(|pack| &pack.stickers)
                .find(|sticker| sticker.id == id)
                .map(|sticker| (None, sticker))
        })
    }
}
