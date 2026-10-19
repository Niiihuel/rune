//! What the account may do with a group conversation. Listing groups grants
//! nothing: every change needs the provider's support and, to take someone
//! out, the group's ownership.

use rune_i18n::t;
use rune_model::{ChannelId, ChannelKind, RelationshipKind, UserId};

use crate::{Permit, Store};

/// A group has the account and up to this many others.
pub const GROUP_OTHERS: usize = 9;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupAbilities {
    pub rename: Permit,
    /// Change or take away its picture.
    pub icon: Permit,
    pub add: Permit,
    /// Take someone else out: only who owns the group.
    pub remove: Permit,
    pub leave: Permit,
    /// Make someone else the owner: only who owns the group.
    pub owner: Permit,
}

impl Store {
    /// What the account may do with `channel`; `None` unless it is a group.
    pub fn group_abilities(&self, channel: ChannelId) -> Option<GroupAbilities> {
        let group = self
            .channel(channel)
            .filter(|found| found.kind == ChannelKind::Group)?;
        let capabilities = self.capabilities();
        let base = self.social_write(capabilities.manage_groups, t("gestionar grupos"));
        let members = match &base {
            Permit::Allowed => self.social_write(
                capabilities.group_members,
                t("añadir ni quitar personas de un grupo"),
            ),
            denied => denied.clone(),
        };
        let add = match &members {
            Permit::Allowed if group.recipients.len() >= GROUP_OTHERS => {
                Permit::Denied(t("El grupo ya tiene 10 personas").into())
            }
            permit => permit.clone(),
        };
        let remove = match &members {
            Permit::Allowed if group.owner.is_none() || group.owner != self.me() => {
                Permit::Denied(t("Solo quien es dueño del grupo puede quitar personas").into())
            }
            permit => permit.clone(),
        };
        let owner = match &base {
            Permit::Allowed => match self.social_write(
                capabilities.group_owner,
                t("pasar la propiedad de un grupo"),
            ) {
                Permit::Allowed if group.owner.is_none() || group.owner != self.me() => {
                    Permit::Denied(t("Solo quien es dueño del grupo puede pasarlo").into())
                }
                permit => permit,
            },
            denied => denied.clone(),
        };
        Some(GroupAbilities {
            rename: base.clone(),
            icon: base.clone(),
            add,
            remove,
            leave: base,
            owner,
        })
    }

    /// Friends that could join `channel`: not in it yet, by name.
    pub fn group_candidates(&self, channel: ChannelId) -> Vec<UserId> {
        let Some(group) = self.channel(channel) else {
            return Vec::new();
        };
        self.friends()
            .into_iter()
            .filter(|user| !group.recipients.contains(user))
            .collect()
    }

    /// The account's friends, by name.
    pub fn friends(&self) -> Vec<UserId> {
        self.relationships()
            .unwrap_or_default()
            .into_iter()
            .filter(|known| known.kind == RelationshipKind::Friend)
            .map(|known| known.user)
            .collect()
    }

    /// Whether a group can be made with several friends at once.
    pub fn create_group(&self) -> Permit {
        self.social_write(self.capabilities().create_groups, t("crear grupos"))
    }

    /// Whether `channel`, a direct conversation, can be taken off the list.
    pub fn close_direct(&self, channel: ChannelId) -> Permit {
        if self
            .channel(channel)
            .is_none_or(|found| found.kind != ChannelKind::Direct)
        {
            Permit::Denied(t("Solo se cierran conversaciones directas").into())
        } else {
            self.social_write(self.capabilities().close_direct, t("cerrar conversaciones"))
        }
    }
}
