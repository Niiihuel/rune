//! One explicit unofficial invite write, preceded by public scope inspection.
//! No remote messages or credentials leave this module; an hCaptcha
//! challenge leaves it only toward the provider's own verification window.

use super::challenges::{with_answer, Solved};
use super::*;
use rune_provider::InvitationDestination;
use rune_provider::{InvitationJoinError as JoinError, InvitationTarget};

const MAX_BODY: usize = 64 * 1024;

impl RestError {
    pub(crate) fn invitation(error: JoinError) -> Self {
        Self {
            reason: LoginError::new(error.label()),
            ambiguous: error == JoinError::Ambiguous,
            ends_session: false,
            invitation: Some(error),
            challenge: None,
            refused: false,
            blocked: None,
        }
    }
}

impl Http {
    /// The GET intentionally has no account authorization. Its result is never
    /// a grant of membership; the provider rechecks the session before POST.
    pub async fn inspect_acceptance(
        &self,
        invitation: &InvitationTarget,
        destination: InvitationDestination,
    ) -> Result<Vec<Event>, RestError> {
        canonical(invitation, destination)?;
        let data = self.invitation_request(None, invitation, None).await?;
        if !matches_scope(&data, invitation, destination) {
            return Err(RestError::invitation(JoinError::Invalid));
        }
        let expiration = crate::invitations::expiration(&data)
            .map_err(|_| RestError::invitation(JoinError::Invalid))?;
        if expiration.is_some_and(|at| at.0 <= chrono::Utc::now().timestamp_millis()) {
            return Err(RestError::invitation(JoinError::Invalid));
        }
        // Browser identity (2026-10-19): what the acceptance says it saw.
        self.remember_invitation(invitation, &data);
        Ok(Vec::new())
    }

    /// A success only acknowledges the request. Gateway membership is still
    /// authoritative; never construct a server/channel from the HTTP response.
    /// With `solved`, this is the one retry a solved challenge allows: another
    /// challenge then is a verification for the official application.
    pub async fn accept_invitation(
        &self,
        secret: &SessionSecret,
        invitation: &InvitationTarget,
        destination: InvitationDestination,
        request: u64,
        solved: Option<&Solved>,
    ) -> Result<Vec<Event>, RestError> {
        canonical(invitation, destination)?;
        let data = self
            .invitation_request(Some(secret), invitation, solved)
            .await
            .map_err(|mut error| {
                if solved.is_some() && error.challenge.take().is_some() {
                    error.invitation = Some(JoinError::VerificationRequired);
                }
                error
            })?;
        if !matches_scope(&data, invitation, destination) {
            return Err(RestError::invitation(JoinError::Ambiguous));
        }
        Ok(vec![Event::InvitationAcceptance {
            request,
            destination,
            result: Ok(()),
        }])
    }

    async fn invitation_request(
        &self,
        secret: Option<&SessionSecret>,
        invitation: &InvitationTarget,
        solved: Option<&Solved>,
    ) -> Result<Value, RestError> {
        let mutating = secret.is_some();
        let step = match (mutating, solved.is_some()) {
            (false, _) => "preflight GET",
            (true, false) => "POST",
            (true, true) => "POST after verification",
        };
        let now = Instant::now();
        if let Some(until) = *self
            .cooldown
            .lock()
            .map_err(|_| RestError::invitation(JoinError::Unavailable))?
        {
            if until > now {
                said(
                    step,
                    format_args!("not sent: still waiting after a rate limit"),
                );
                return Err(RestError::invitation(JoinError::RateLimited {
                    retry_after_secs: until.duration_since(now).as_secs_f64().ceil() as u64,
                }));
            }
        }
        let uncertain = || {
            RestError::invitation(if mutating {
                JoinError::Ambiguous
            } else {
                JoinError::Unavailable
            })
        };
        let path = format!("{}/invites/{}", self.base, invitation.key);
        let mut request = if let Some(secret) = secret {
            let mut authorization = HeaderValue::from_str(&secret.0)
                .map_err(|_| RestError::invitation(JoinError::Denied))?;
            authorization.set_sensitive(true);
            let mut post = self
                .api(Method::POST, path)
                .header(AUTHORIZATION, authorization)
                .json(&json!({}));
            // Browser identity (2026-10-19): the web client's invite context.
            if let Some(context) = self.invitation_context(invitation) {
                post = post.header("x-context-properties", context);
            }
            if let Some(solved) = solved {
                post = with_answer(post, solved)
                    .map_err(|()| RestError::invitation(JoinError::Unavailable))?;
            }
            post
        } else {
            self.api(Method::GET, path)
                .query(&[("with_counts", "true")])
        };
        // Preserve a fixed total deadline for each phase, including its body.
        request = request.timeout(Duration::from_secs(15));
        let response = request.send().await.map_err(|error| {
            // Its text may hold the address, and with it the invitation.
            let kind = if error.is_timeout() {
                "timed out"
            } else if error.is_connect() {
                "could not connect"
            } else {
                "transport error"
            };
            said(step, format_args!("no answer ({kind})"));
            uncertain()
        })?;
        let status = response.status();
        if !matches!(status.as_u16(), 400 | 403) {
            said(step, format_args!("HTTP {}", status.as_u16()));
        }
        // Only an authenticated POST can invalidate the account session.
        // Its rejection is decisive even if the response body stalls.
        if status == StatusCode::UNAUTHORIZED {
            let mut error = RestError::invitation(JoinError::Denied);
            error.ends_session = mutating;
            return Err(error);
        }
        let header_delay = if status == StatusCode::TOO_MANY_REQUESTS {
            response.headers().get(RETRY_AFTER)
        } else if response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v == "0")
        {
            response.headers().get("x-ratelimit-reset-after")
        } else {
            None
        }
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0);
        if let Some(delay) = header_delay {
            self.defer(delay, super::Bucket::All);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            // If body parsing is cancelled, keep a conservative hold. If it
            // completes, replace only our provisional value with the real delay,
            // preserving any older/concurrent longer cooldown.
            let provisional = Instant::now() + Duration::from_secs(60);
            let (previous, installed) = {
                let mut hold = self
                    .cooldown
                    .lock()
                    .map_err(|_| RestError::invitation(JoinError::Unavailable))?;
                let previous = *hold;
                let installed = previous.map_or(provisional, |old| old.max(provisional));
                *hold = Some(installed);
                (previous, installed)
            };
            let delay = if let Some(delay) = header_delay {
                delay
            } else {
                read_bounded(response, MAX_BODY)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .and_then(|body| body["retry_after"].as_f64())
                    .filter(|delay| delay.is_finite() && *delay >= 0.0)
                    .unwrap_or(60.0)
            }
            .max(1.0);
            let until = Duration::try_from_secs_f64(delay)
                .ok()
                .and_then(|delay| Instant::now().checked_add(delay))
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 60 * 60));
            if let Ok(mut hold) = self.cooldown.lock() {
                *hold = Some(if *hold == Some(installed) {
                    previous.map_or(until, |old| old.max(until))
                } else {
                    hold.map_or(until, |old| old.max(until))
                });
            }
            let retry_after_secs = self
                .cooldown
                .lock()
                .ok()
                .and_then(|hold| *hold)
                .map(|until| {
                    until
                        .saturating_duration_since(Instant::now())
                        .as_secs_f64()
                        .ceil() as u64
                })
                .unwrap_or(60)
                .max(1);
            return Err(RestError::invitation(JoinError::RateLimited {
                retry_after_secs,
            }));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(RestError::invitation(JoinError::Invalid));
        }
        // Inspect only bounded rejection bodies to recognize a verification
        // requirement. Only the account's own POST may offer a challenge; the
        // provider resumes it solely with a person's answer.
        if matches!(status.as_u16(), 400 | 403) {
            let (rejection, code) = rejections::inspect_coded(response).await;
            match code {
                Some(code) => said(
                    step,
                    format_args!("HTTP {}, error code {code}", status.as_u16()),
                ),
                None => said(
                    step,
                    format_args!("HTTP {}, no error code", status.as_u16()),
                ),
            }
            if let Some(rejection) = rejection {
                let invitation = match &rejection {
                    rejections::Rejection::InvalidSession | rejections::Rejection::Refused(_) => {
                        JoinError::Denied
                    }
                    rejections::Rejection::Challenge(_) if mutating => JoinError::ChallengeRequired,
                    _ => JoinError::VerificationRequired,
                };
                let mut error = rejection.error(mutating);
                error.invitation = Some(invitation);
                if !mutating {
                    error.challenge = None;
                }
                return Err(error);
            }
            return Err(RestError::invitation(match code.and_then(refused) {
                Some(reason) => reason,
                None if status == StatusCode::FORBIDDEN => JoinError::Denied,
                None => JoinError::Invalid,
            }));
        }
        if status != StatusCode::OK {
            return Err(
                if status.is_server_error() || status.is_redirection() || status.is_success() {
                    uncertain()
                } else {
                    RestError::invitation(JoinError::Unavailable)
                },
            );
        }
        let bytes = read_bounded(response, MAX_BODY)
            .await
            .map_err(|_| uncertain())?;
        serde_json::from_slice(&bytes).map_err(|_| uncertain())
    }
}

/// One line in RUNE's log per answer to an invitation step, so a refused
/// acceptance says why: the HTTP status and the service's integer error
/// code. Never the body, the invitation, the token or the account.
fn said(step: &str, what: std::fmt::Arguments<'_>) {
    crate::voice::diag::write("invitation", format_args!("{step}: {what}"));
}

/// What the service's JSON error code says of a refused acceptance: the
/// codes the official web client names for joining (`AbortCodes`, and the
/// texts it shows for them; docs/REA.md). Any other stays a plain refusal.
fn refused(code: u64) -> Option<JoinError> {
    Some(match code {
        10006 => JoinError::Invalid,
        30001 => JoinError::ServerLimit,
        30019 => JoinError::ServerFull,
        40007 => JoinError::Banned,
        40068 => JoinError::AccountLimited,
        40069 => JoinError::InvitesPaused,
        50095 => JoinError::Region,
        20024 | 20086 | 700_000 | 700_002 => JoinError::AgeRestricted,
        // GUILD_LIMITED_ACCESS_*, answered with 403.
        400_000..=409_999 => JoinError::LimitedAccess,
        _ => return None,
    })
}

fn canonical(
    invitation: &InvitationTarget,
    destination: InvitationDestination,
) -> Result<(), RestError> {
    if !destination.valid()
        || crate::invitations::target(&invitation.url).as_ref() != Some(invitation)
    {
        return Err(RestError::invitation(JoinError::Invalid));
    }
    Ok(())
}

fn matches_scope(
    data: &Value,
    invitation: &InvitationTarget,
    destination: InvitationDestination,
) -> bool {
    // Public Invite: type 0 is guild, 1 group DM, 2 friend; flag bit 0 is
    // voice-only guest access. Reject other flags and targets until their
    // semantics have an explicit flow.
    // https://docs.discord.com/developers/resources/invite#invite-object
    let guest = matches!(destination, InvitationDestination::Guest { .. });
    data["code"].as_str() == Some(invitation.key.as_str())
        && crate::invitations::destination(data) == Some(destination)
        && data
            .get("flags")
            .is_none_or(|value| value.as_u64() == Some(u64::from(guest)))
        && ["target_type", "target_user", "target_application"]
            .iter()
            .all(|key| data.get(*key).is_none_or(Value::is_null))
}
