//! DAVE, Discord's end-to-end encryption for calls (protocol version 1),
//! over `davey`: the MLS group of the call's members, the media keys it
//! derives and the voice gateway messages that move it between epochs.
//!
//! Only version 1 is accepted: a downgrade to unencrypted media ends the
//! call. Nothing is sent before a group exists and its transition executed.
//! The identity key is ephemeral, generated per call and never stored.

use rune_i18n::es;
use std::num::NonZeroU16;

use davey::{Codec, DaveSession, MediaType, ProposalsOperationType};

/// Opcodes of the voice gateway that DAVE uses.
pub(crate) mod op {
    pub const PREPARE_TRANSITION: u64 = 21;
    pub const EXECUTE_TRANSITION: u64 = 22;
    pub const TRANSITION_READY: u64 = 23;
    pub const PREPARE_EPOCH: u64 = 24;
    pub const EXTERNAL_SENDER: u8 = 25;
    pub const KEY_PACKAGE: u8 = 26;
    pub const PROPOSALS: u8 = 27;
    pub const COMMIT_WELCOME: u8 = 28;
    pub const ANNOUNCE_COMMIT: u8 = 29;
    pub const WELCOME: u8 = 30;
    pub const INVALID_COMMIT_WELCOME: u64 = 31;
}

/// People in one call at most: davey's and Discord's own bound.
pub(crate) const MAX_MEMBERS: usize = 99;
/// Recoveries from an unusable group before giving up.
const MAX_RESETS: u8 = 3;
/// Group changes before the call must be joined again: bounds what
/// OpenMLS keeps for a long call.
const MAX_TRANSITIONS: u16 = 1024;
const VERSION: NonZeroU16 = NonZeroU16::MIN;

/// What the connection has to send because of a DAVE message.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    /// A JSON message: `{"op": op, "d": {"transition_id": id}}`.
    Transition { op: u64, id: u16 },
    /// A binary message, opcode first.
    Binary(Vec<u8>),
}

pub(crate) struct Dave {
    pub(super) session: DaveSession,
    own: u64,
    channel: u64,
    /// Who the voice server says is connected, the account included.
    members: Vec<u64>,
    /// A transition prepared and not yet executed.
    pending: Option<u16>,
    /// Media may be encrypted and decrypted.
    ready: bool,
    resets: u8,
    transitions: u16,
}

impl Dave {
    pub(crate) fn new(own: u64, channel: u64) -> Result<Self, &'static str> {
        Ok(Self {
            session: DaveSession::new(VERSION, own, channel, None)
                .map_err(|_| es("No se pudo preparar el cifrado de la llamada."))?,
            own,
            channel,
            members: vec![own],
            pending: None,
            ready: false,
            resets: 0,
            transitions: 0,
        })
    }

    /// Media flows both ways.
    pub(crate) fn ready(&self) -> bool {
        self.ready && self.session.is_ready()
    }

    /// Nobody else is in the call: there is no one to encrypt for.
    pub(crate) fn alone(&self) -> bool {
        self.members.len() == 1
    }

    /// The code people compare to check that nobody else holds the keys,
    /// once media flows encrypted.
    pub(crate) fn privacy_code(&self) -> Option<&str> {
        self.ready()
            .then(|| self.session.voice_privacy_code())
            .flatten()
    }

    /// The account's and `user`'s key fingerprints, from which their
    /// verification code is derived (slowly: see [`verification_code`]).
    pub(crate) fn fingerprints(&self, user: u64) -> Option<[Vec<u8>; 2]> {
        (self.ready() && user != self.own && self.is_member(user))
            .then(|| self.session.get_key_fingerprint_pair(0, user).ok())
            .flatten()
    }

    pub(crate) fn key_package(&mut self) -> Result<Vec<u8>, &'static str> {
        // libdave and Serein send the bare TLS KeyPackage after the opcode.
        let mut message = vec![op::KEY_PACKAGE];
        message.extend(
            self.session
                .create_key_package()
                .map_err(|_| es("No se pudo preparar el cifrado de la llamada."))?,
        );
        Ok(message)
    }

    /// People joined (op 11).
    pub(crate) fn connected(&mut self, users: &[u64]) -> Result<(), &'static str> {
        for &user in users {
            if user == 0 {
                return Err(es("El servidor de voz anunció un participante inválido."));
            }
            if !self.members.contains(&user) {
                if self.members.len() >= MAX_MEMBERS {
                    return Err(es("La llamada superó el límite de participantes."));
                }
                self.members.push(user);
            }
        }
        Ok(())
    }

    /// Someone left (op 13). The group keeps them until the removal commit.
    pub(crate) fn disconnected(&mut self, user: u64) -> Result<(), &'static str> {
        if user == self.own {
            return Err(es("Discord sacó a esta cuenta de la llamada."));
        }
        self.members.retain(|member| *member != user);
        Ok(())
    }

    pub(crate) fn is_member(&self, user: u64) -> bool {
        self.members.contains(&user)
    }

    /// The external sender (op 25): the voice server's key for proposals.
    pub(crate) fn external_sender(&mut self, data: &[u8]) -> Result<(), &'static str> {
        self.session
            .set_external_sender(data)
            .map_err(|_| es("El servidor de voz envió una clave de grupo inválida."))
    }

    /// Proposals to add or remove members (op 27): answered with a commit,
    /// and a welcome for those added.
    pub(crate) fn proposals(&mut self, data: &[u8]) -> Result<Option<Reply>, &'static str> {
        // Until the external sender creates our group, proposals are for others.
        if self.session.group().is_none() {
            return Ok(None);
        }
        let (&operation, proposals) = data.split_first().ok_or(MALFORMED)?;
        let operation = match operation {
            0 => ProposalsOperationType::APPEND,
            1 => ProposalsOperationType::REVOKE,
            _ => return Err(MALFORMED),
        };
        let result = self
            .session
            .process_proposals(operation, proposals, Some(&self.members))
            .map_err(|_| es("El servidor de voz propuso un cambio de grupo inválido."))?;
        Ok(result.map(|result| {
            let mut message = vec![op::COMMIT_WELCOME];
            message.extend(result.commit);
            message.extend(result.welcome.unwrap_or_default());
            Reply::Binary(message)
        }))
    }

    /// A commit (op 29) or welcome (op 30) moving the group to a new epoch.
    /// A group that cannot be processed is reported (op 31) and started
    /// over with a new key package, a bounded number of times.
    pub(crate) fn group_changed(
        &mut self,
        opcode: u8,
        data: &[u8],
    ) -> Result<Vec<Reply>, &'static str> {
        if data.len() < 3 {
            return Err(MALFORMED);
        }
        self.count_transition()?;
        let id = u16::from_be_bytes([data[0], data[1]]);
        // Someone who left stays in the group until the commit removing them.
        let before = self.session.get_user_ids().unwrap_or_default();
        let processed = if opcode == op::ANNOUNCE_COMMIT {
            self.session.process_commit(&data[2..]).is_ok()
        } else {
            self.session.process_welcome(&data[2..]).is_ok()
        };
        if !processed || !self.group_matches(&before) {
            self.resets += 1;
            if self.resets > MAX_RESETS {
                return Err(es(
                    "El cifrado de la llamada falló varias veces. Vuelve a entrar.",
                ));
            }
            self.reinit()?;
            return Ok(vec![
                Reply::Transition {
                    op: op::INVALID_COMMIT_WELCOME,
                    id,
                },
                Reply::Binary(self.key_package()?),
            ]);
        }
        Ok(self.prepared(id))
    }

    /// The voice server prepares a protocol transition (op 21).
    pub(crate) fn prepare_transition(
        &mut self,
        version: u64,
        id: u16,
    ) -> Result<Vec<Reply>, &'static str> {
        if version != 1 {
            return Err(es(
                "Discord pidió quitar el cifrado de la llamada; se cortó.",
            ));
        }
        Ok(self.prepared(id))
    }

    fn prepared(&mut self, id: u16) -> Vec<Reply> {
        if id == 0 {
            // Transition zero is the initial group: it executes at once.
            self.pending = None;
            self.ready = self.session.is_ready();
            return Vec::new();
        }
        self.pending = Some(id);
        // Davey keeps the previous epoch's keys a while: the media of an
        // established call keeps flowing through a member change.
        vec![Reply::Transition {
            op: op::TRANSITION_READY,
            id,
        }]
    }

    /// The voice server executes a prepared transition (op 22).
    pub(crate) fn execute_transition(&mut self, id: u16) {
        if self.pending == Some(id) {
            self.pending = None;
            self.ready = self.session.is_ready();
        }
    }

    /// The voice server starts a new group (op 24, epoch 1).
    pub(crate) fn prepare_epoch(
        &mut self,
        version: u64,
        epoch: u64,
    ) -> Result<Option<Reply>, &'static str> {
        if version != 1 {
            return Err(es(
                "Discord pidió quitar el cifrado de la llamada; se cortó.",
            ));
        }
        if epoch != 1 {
            return Ok(None);
        }
        self.count_transition()?;
        self.reinit()?;
        Ok(Some(Reply::Binary(self.key_package()?)))
    }

    pub(crate) fn encrypt(&mut self, opus: &[u8]) -> Option<Vec<u8>> {
        if !self.ready() {
            return None;
        }
        self.session
            .encrypt_opus(opus)
            .ok()
            .map(|frame| frame.into_owned())
    }

    /// An H264 access unit of the account's own (Annex B), for the group.
    pub(crate) fn encrypt_video(&mut self, unit: &[u8]) -> Option<Vec<u8>> {
        if !self.ready() {
            return None;
        }
        self.session
            .encrypt(MediaType::VIDEO, Codec::H264, unit)
            .ok()
            .map(|frame| frame.into_owned())
    }

    /// Only someone the voice server announced, with a key in the group.
    pub(crate) fn decrypt(&mut self, user: u64, frame: &[u8]) -> Option<Vec<u8>> {
        self.decrypt_media(user, MediaType::AUDIO, frame)
    }

    /// An H264 access unit (Annex B), likewise.
    pub(crate) fn decrypt_video(&mut self, user: u64, frame: &[u8]) -> Option<Vec<u8>> {
        self.decrypt_media(user, MediaType::VIDEO, frame)
    }

    fn decrypt_media(&mut self, user: u64, media: MediaType, frame: &[u8]) -> Option<Vec<u8>> {
        if !self.ready() || !self.is_member(user) || user == self.own {
            return None;
        }
        self.session.decrypt(user, media, frame).ok()
    }

    fn reinit(&mut self) -> Result<(), &'static str> {
        self.ready = false;
        self.pending = None;
        self.session
            .reinit(VERSION, self.own, self.channel, None)
            .map_err(|_| es("No se pudo reiniciar el cifrado de la llamada."))
    }

    fn count_transition(&mut self) -> Result<(), &'static str> {
        self.transitions += 1;
        if self.transitions > MAX_TRANSITIONS {
            return Err(es(
                "La llamada cambió de clave demasiadas veces. Vuelve a entrar.",
            ));
        }
        Ok(())
    }

    /// The group is this call's and holds only us and people announced, or
    /// already in the group before this change.
    fn group_matches(&self, before: &[u64]) -> bool {
        let Some(group) = self.session.group() else {
            return false;
        };
        let Some(ids) = self.session.get_user_ids() else {
            return false;
        };
        group.group_id().as_slice() == self.channel.to_be_bytes()
            && ids.contains(&self.own)
            && ids.len() <= MAX_MEMBERS
            && ids
                .iter()
                .all(|id| self.members.contains(id) || before.contains(id))
            && ids
                .iter()
                .enumerate()
                .all(|(at, id)| !ids[..at].contains(id))
    }
}

impl Drop for Dave {
    fn drop(&mut self) {
        // Deletes the group and its secrets from davey's in-memory storage.
        let _ = self.session.reset();
    }
}

const MALFORMED: &str = es("El servidor de voz envió un mensaje de cifrado inválido.");

/// The 45 digits two people compare, as libdave and the official client
/// show them: scrypt over both fingerprints, so it takes a while; call it
/// off the connection's task.
pub(crate) fn verification_code(fingerprints: [Vec<u8>; 2]) -> Option<String> {
    let pairwise = davey::pairwise_fingerprints_internal(fingerprints).ok()?;
    davey::generate_displayable_code_internal(&pairwise, 45, 5).ok()
}
