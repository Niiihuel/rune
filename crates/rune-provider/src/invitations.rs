//! Public invitation metadata, independent of a provider's protocol or account.

use rune_i18n::t;
use rune_model::{ChannelId, RemoteImage, ServerId, Timestamp, UserId};

/// An adapter-validated invitation. `key` is opaque to the host and UI.
#[derive(Clone, PartialEq, Eq)]
pub struct InvitationTarget {
    pub key: String,
    /// Its canonical public page, where a person can accept it in a browser
    /// with the service's own client.
    pub url: String,
}

impl std::fmt::Debug for InvitationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InvitationTarget([REDACTED])")
    }
}

/// Where accepting an invitation leads, as the provider read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvitationDestination {
    /// Membership of a server.
    Server(ServerId),
    /// A guest's way into one voice channel of a server, not membership:
    /// the service lets the guest go when they leave the call.
    Guest {
        server: ServerId,
        channel: ChannelId,
    },
    /// A place in a group conversation.
    Group(ChannelId),
    /// Friendship with whoever made the invitation.
    Friend(UserId),
}

impl InvitationDestination {
    /// The server it leads into, as a member or a guest.
    pub fn server(self) -> Option<ServerId> {
        match self {
            Self::Server(server) | Self::Guest { server, .. } => Some(server),
            Self::Group(_) | Self::Friend(_) => None,
        }
    }

    /// Whether it names something: no id of it is zero.
    pub fn valid(self) -> bool {
        match self {
            Self::Server(server) => server.0 != 0,
            Self::Guest { server, channel } => server.0 != 0 && channel.0 != 0,
            Self::Group(channel) => channel.0 != 0,
            Self::Friend(user) => user.0 != 0,
        }
    }
}

/// Preview data never grants membership, channels or permissions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationPreview {
    pub destination: InvitationDestination,
    /// The server's or the group's name, or the name of who offers
    /// friendship; a group without a name has it empty.
    pub name: String,
    pub description: Option<String>,
    pub image: Option<RemoteImage>,
    pub members: Option<u64>,
    pub online: Option<u64>,
    /// Provider-reported expiration, if known; absence does not mean permanent.
    pub expires_at: Option<Timestamp>,
    // Scheduled events (eventos-programados).
    /// The server's event the invitation is for, when it names one.
    pub event: Option<rune_model::ScheduledEvent>,
}

/// Static failure categories: remote response bodies never reach the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvitationError {
    Unsupported,
    Invalid,
    Unavailable,
    RateLimited { retry_after_secs: u64 },
}

impl InvitationError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unsupported => t("Esta conexión no permite consultar invitaciones."),
            Self::Invalid => t("La invitación no es válida o esta conexión no la entiende."),
            Self::Unavailable => {
                t("No se pudo consultar la invitación. Puedes volver a intentarlo.")
            }
            Self::RateLimited { .. } => t("Hay que esperar antes de consultar otra invitación."),
        }
    }
}

/// A request-scoped response, delivered independently of account events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationResult {
    pub request: crate::RequestId,
    pub target: InvitationTarget,
    pub result: Result<InvitationPreview, InvitationError>,
}

/// Acceptance failures are separate from public-preview failures. A transport
/// failure after dispatch is ambiguous, never an unavailable/safe-to-retry result.
/// Raw remote responses and challenge material do not cross this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvitationJoinError {
    Unsupported,
    Invalid,
    Denied,
    /// The service demands a verification this provider cannot offer here.
    VerificationRequired,
    /// The service asked for a challenge the person can solve in the
    /// provider's own window: [`crate::Command::VerifyInvitation`] opens it
    /// and sends this same acceptance once more. Nothing is sent otherwise.
    ChallengeRequired,
    RateLimited {
        retry_after_secs: u64,
    },
    /// The attempt was definitely not sent or was definitely rejected.
    Unavailable,
    /// The write may have succeeded; never automatically repeat it.
    Ambiguous,
    // Why the service refused, when it says so (2026-10-19): another
    // attempt would be refused alike, so none is offered.
    /// The account already has what the invitation gives.
    AlreadyJoined,
    /// The account is in as many servers as the service allows.
    ServerLimit,
    /// The account is banned from that server.
    Banned,
    /// The server has as many members as it can hold.
    ServerFull,
    /// The server paused its invitations.
    InvitesPaused,
    /// The server is not available where the account is.
    Region,
    /// The service keeps the account out of that server by its age.
    AgeRestricted,
    /// The server's features are limited for now.
    LimitedAccess,
    /// The service limited the account itself.
    AccountLimited,
}

impl InvitationJoinError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unsupported => t("Esta conexión no permite aceptar invitaciones."),
            Self::Invalid => t("La invitación ya no es válida."),
            Self::Denied => t("No puedes aceptar esta invitación con esta cuenta."),
            Self::VerificationRequired => t("El servicio requiere una verificación para entrar."),
            Self::ChallengeRequired => t("El servicio pide una verificación para entrar."),
            Self::RateLimited { .. } => t("Espera antes de volver a aceptar la invitación."),
            Self::Unavailable => t("No se pudo enviar la solicitud. Puedes volver a intentarlo."),
            Self::Ambiguous => {
                t("No se pudo confirmar el ingreso. Comprueba si el servidor aparece.")
            }
            Self::AlreadyJoined => t("Esta cuenta ya tiene lo que da la invitación."),
            Self::ServerLimit => t("Llegaste al máximo de servidores: sal de uno para entrar."),
            Self::Banned => t("Te banearon de este servidor."),
            Self::ServerFull => t("El servidor está lleno."),
            Self::InvitesPaused => t("El servidor pausó sus invitaciones."),
            Self::Region => t("Este servidor no está disponible en tu región."),
            Self::AgeRestricted => {
                t("El servicio no deja entrar a este servidor por la edad de la cuenta.")
            }
            Self::LimitedAccess => {
                t("Las funciones de este servidor están limitadas: no admite ingresos ahora.")
            }
            Self::AccountLimited => {
                t("El servicio limitó esta cuenta y no la deja entrar a servidores.")
            }
        }
    }

    /// Whether another explicit attempt can be offered: only after a failure
    /// that may pass (nothing sent, or a rate limit once its wait expires).
    /// A refusal is never offered again: repeating it can harm the account.
    /// This never authorizes automatic retries or bypassing verification.
    pub fn can_retry(self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capabilities, Command, Failure};
    use rune_model::UserId;

    #[test]
    fn accepting_is_opt_in_and_failures_keep_the_request_binding_without_the_link() {
        assert!(!Capabilities::default().invitation_accept);
        let command = Command::AcceptInvitation {
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
            request: 7,
            target: InvitationTarget {
                key: "private-invite-code".into(),
                url: "https://example.invalid/private-invite-code".into(),
            },
            destination: InvitationDestination::Server(ServerId(2)),
            account: UserId(3),
        };
        let failure = Failure::of(&command).unwrap();
        assert_eq!(
            failure,
            Failure::Invitation {
                request: 7,
                destination: InvitationDestination::Server(ServerId(2))
            }
        );
        assert_eq!(failure.action(), "aceptar la invitación");
        assert!(!format!("{command:?} {failure:?}").contains("private-invite-code"));
    }

    #[test]
    fn each_kind_of_invitation_is_accepted_only_with_its_own_capability() {
        let server = InvitationDestination::Server(ServerId(2));
        let guest = InvitationDestination::Guest {
            server: ServerId(2),
            channel: ChannelId(4),
        };
        let group = InvitationDestination::Group(ChannelId(5));
        let friend = InvitationDestination::Friend(UserId(6));
        let mut capabilities = Capabilities {
            invitation_accept: true,
            ..Capabilities::default()
        };
        assert!(capabilities.accepts(server));
        assert!(![guest, group, friend]
            .iter()
            .any(|kind| capabilities.accepts(*kind)));
        capabilities.guest_invitations = true;
        capabilities.group_invitations = true;
        capabilities.friend_invitations = true;
        assert!([server, guest, group, friend]
            .iter()
            .all(|kind| capabilities.accepts(*kind)));
        capabilities.invitation_accept = false;
        assert!(![server, guest, group, friend]
            .iter()
            .any(|kind| capabilities.accepts(*kind)));
        assert_eq!(guest.server(), Some(ServerId(2)));
        assert_eq!(friend.server(), None);
        assert!(!InvitationDestination::Group(ChannelId(0)).valid());
        assert!(!guest_without_channel().valid());
        assert!(friend.valid());
    }

    fn guest_without_channel() -> InvitationDestination {
        InvitationDestination::Guest {
            server: ServerId(2),
            channel: ChannelId(0),
        }
    }

    #[test]
    fn refusals_uncertain_writes_and_verification_never_offer_a_repeat() {
        for error in [
            InvitationJoinError::Unsupported,
            InvitationJoinError::Invalid,
            InvitationJoinError::Denied,
            InvitationJoinError::VerificationRequired,
            InvitationJoinError::ChallengeRequired,
            InvitationJoinError::Ambiguous,
            InvitationJoinError::AlreadyJoined,
            InvitationJoinError::ServerLimit,
            InvitationJoinError::Banned,
            InvitationJoinError::ServerFull,
            InvitationJoinError::InvitesPaused,
            InvitationJoinError::Region,
            InvitationJoinError::AgeRestricted,
            InvitationJoinError::LimitedAccess,
            InvitationJoinError::AccountLimited,
        ] {
            assert!(!error.can_retry());
            assert!(!error.label().is_empty());
        }
        for error in [
            InvitationJoinError::Unavailable,
            InvitationJoinError::RateLimited {
                retry_after_secs: 30,
            },
        ] {
            assert!(error.can_retry());
            assert!(!error.label().is_empty());
        }
    }
}
