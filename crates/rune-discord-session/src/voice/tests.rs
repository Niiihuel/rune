//! A synthetic voice server on this machine (WebSocket and UDP on
//! 127.0.0.1) with its own MLS external sender, and a second synthetic
//! participant. Nothing here reaches Discord.

use std::{net::SocketAddr, time::Duration};

use futures_util::{SinkExt, StreamExt};
use openmls::prelude::{tls_codec::Serialize as _, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use rune_model::{CallPhase, ChannelId, UserId};
use rune_provider::{video::MAX_STREAMS, voice::voice_link, Event, Snapshot};
use serde_json::{json, Value};
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    sync::watch,
    time::timeout,
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message,
    },
    WebSocketStream,
};
use zeroize::Zeroizing;

use super::{
    connection::{self, Controls, Credentials, Video},
    dave::{op, Dave, Reply},
    rtp::{self, Transport},
    Calls, VoiceWant,
};

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHANNEL: u64 = 30;
const STAGE: u64 = 35;
const GUILD: u64 = 40;
const KEY: [u8; 32] = [5; 32];
const WAIT: Duration = Duration::from_secs(5);
/// The heartbeat of a test about media, not heartbeats: only the first one
/// is sent. Such a test reads the socket between steps only, and under load
/// a heartbeat left unanswered past the next one rightly ends the session
/// mid-test (the connection then waits on a resumption nobody answers).
const QUIET: u64 = 60_000;

/// The voice server's side of MLS: the key it signs proposals with.
struct Proposer(SignatureKeyPair);

impl Proposer {
    fn new() -> Self {
        Self(SignatureKeyPair::new(SignatureScheme::ECDSA_SECP256R1_SHA256).unwrap())
    }

    /// The external sender it announces (op 25, without the opcode).
    fn announcement(&self) -> Vec<u8> {
        let identity = BasicCredential::new(b"synthetic voice server".to_vec());
        ExternalSender::new(self.0.public().to_vec().into(), identity.into())
            .tls_serialize_detached()
            .unwrap()
    }

    /// Proposals (op 27, without the opcode) adding the owner of `package`
    /// to `member`'s group.
    fn adding(&self, member: &Dave, package: &[u8]) -> Vec<u8> {
        assert_eq!(package[0], op::KEY_PACKAGE);
        let crypto = OpenMlsRustCrypto::default();
        let joining = KeyPackageIn::tls_deserialize_exact_bytes(&package[1..]).unwrap();
        let joining = joining
            .validate(crypto.crypto(), ProtocolVersion::Mls10)
            .unwrap();
        let group = member.session.group().unwrap();
        let (id, epoch) = (group.group_id().clone(), group.epoch());
        let signed = ExternalProposal::new_add::<OpenMlsRustCrypto>(
            joining,
            id,
            epoch,
            &self.0,
            SenderExtensionIndex::new(0),
        )
        .unwrap();
        let signed = signed.tls_serialize_detached().unwrap();
        // One proposal kind byte (0: append), then the proposals as one
        // length-prefixed vector.
        let proposals = openmls::prelude::tls_codec::VLBytes::new(signed);
        [vec![0], proposals.tls_serialize_detached().unwrap()].concat()
    }
}

/// The commit and welcome of an op 28.
fn split(message: &[u8]) -> (Vec<u8>, Vec<u8>) {
    assert_eq!(message[0], op::COMMIT_WELCOME);
    let (_, welcome) = MlsMessageIn::tls_deserialize_bytes(&message[1..]).unwrap();
    let commit = message[1..message.len() - welcome.len()].to_vec();
    (commit, welcome.to_vec())
}

fn with_transition(id: u16, bytes: &[u8]) -> Vec<u8> {
    [id.to_be_bytes().as_slice(), bytes].concat()
}

/// Alice commits Bob in; both end up in one group with the same keys.
fn pair(server: &Proposer) -> (Dave, Dave) {
    let mut alice = Dave::new(ALICE, CHANNEL).unwrap();
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    alice.connected(&[BOB]).unwrap();
    bob.connected(&[ALICE]).unwrap();
    alice.external_sender(&server.announcement()).unwrap();
    bob.external_sender(&server.announcement()).unwrap();
    let package = bob.key_package().unwrap();
    let Some(Reply::Binary(answer)) = alice.proposals(&server.adding(&alice, &package)).unwrap()
    else {
        panic!("a commit answers the proposal");
    };
    let (commit, welcome) = split(&answer);
    assert!(alice
        .group_changed(op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .unwrap()
        .is_empty());
    assert!(bob
        .group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap()
        .is_empty());
    (alice, bob)
}

#[test]
fn dave_two_members_share_keys_and_reject_forgeries_and_downgrades() {
    let server = Proposer::new();
    let mut lonely = Dave::new(ALICE, CHANNEL).unwrap();
    assert!(lonely.alone() && !lonely.ready());
    assert!(
        lonely.encrypt(b"never in clear").is_none(),
        "no group, no media"
    );
    let (mut alice, mut bob) = pair(&server);
    assert!(alice.ready() && bob.ready());
    assert_eq!(alice.privacy_code(), bob.privacy_code());
    // Desktop (P6): 30 digits for the call, and the same 45 for each pair.
    assert!(alice
        .privacy_code()
        .is_some_and(|code| code.len() == 30 && code.bytes().all(|byte| byte.is_ascii_digit())));
    assert!(lonely.privacy_code().is_none());
    let code = super::dave::verification_code(alice.fingerprints(BOB).unwrap()).unwrap();
    assert_eq!(code.len(), 45);
    assert_eq!(
        Some(code),
        super::dave::verification_code(bob.fingerprints(ALICE).unwrap())
    );
    assert!(alice.fingerprints(ALICE).is_none() && alice.fingerprints(99).is_none());
    let sealed = alice.encrypt(b"synthetic opus").unwrap();
    assert_ne!(sealed, b"synthetic opus");
    let mut forged = sealed.clone();
    forged[0] ^= 1;
    assert!(bob.decrypt(ALICE, &forged).is_none());
    assert_eq!(bob.decrypt(ALICE, &sealed).unwrap(), b"synthetic opus");
    assert!(bob.decrypt(ALICE, &sealed).is_none(), "a replay is refused");
    assert!(bob.decrypt(99, &sealed).is_none(), "nobody announced");
    // A prepared transition is acknowledged and gates nothing it should not.
    assert_eq!(
        alice.prepare_transition(1, 7).unwrap(),
        vec![Reply::Transition {
            op: op::TRANSITION_READY,
            id: 7
        }]
    );
    alice.execute_transition(7);
    assert!(alice.ready());
    assert!(
        alice.prepare_transition(0, 8).is_err(),
        "no downgrade to clear media"
    );
    assert!(alice.prepare_epoch(0, 1).is_err());
    // A new epoch starts over: a key package goes out, media waits.
    assert!(
        matches!(alice.prepare_epoch(1, 1).unwrap(), Some(Reply::Binary(package)) if package[0] == op::KEY_PACKAGE)
    );
    assert!(!alice.ready());
    assert!(
        alice.disconnected(ALICE).is_err(),
        "removing the account ends the call"
    );
}

#[test]
fn dave_an_unusable_group_is_reported_and_retried_a_bounded_number_of_times() {
    let server = Proposer::new();
    let mut alice = Dave::new(ALICE, CHANNEL).unwrap();
    alice.connected(&[BOB]).unwrap();
    alice.external_sender(&server.announcement()).unwrap();
    for attempt in 0..3 {
        let replies = alice
            .group_changed(op::WELCOME, &with_transition(9, b"not a welcome"))
            .unwrap();
        assert_eq!(
            replies[0],
            Reply::Transition {
                op: op::INVALID_COMMIT_WELCOME,
                id: 9
            },
            "attempt {attempt}"
        );
        assert!(matches!(&replies[1], Reply::Binary(package) if package[0] == op::KEY_PACKAGE));
    }
    assert!(alice
        .group_changed(op::WELCOME, &with_transition(9, b"not a welcome"))
        .is_err());
    // A welcome into a group with someone never announced is refused.
    let mut carol = Dave::new(3, CHANNEL).unwrap();
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    carol.connected(&[BOB]).unwrap();
    carol.external_sender(&server.announcement()).unwrap();
    bob.external_sender(&server.announcement()).unwrap();
    let Some(Reply::Binary(answer)) = carol
        .proposals(&server.adding(&carol, &bob.key_package().unwrap()))
        .unwrap()
    else {
        panic!()
    };
    let (_, welcome) = split(&answer);
    let replies = bob
        .group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap();
    assert!(matches!(
        replies[0],
        Reply::Transition {
            op: op::INVALID_COMMIT_WELCOME,
            ..
        }
    ));
    assert!(!bob.ready());
}

type Peer = WebSocketStream<TcpStream>;

struct Server {
    listener: TcpListener,
    udp: UdpSocket,
    seq: u16,
}

impl Server {
    async fn new() -> Self {
        Self {
            listener: TcpListener::bind("127.0.0.1:0").await.unwrap(),
            udp: UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            seq: 0,
        }
    }

    fn url(&self) -> String {
        format!("ws://{}/?v=8", self.listener.local_addr().unwrap())
    }

    async fn accept(&self) -> Peer {
        let (stream, _) = timeout(WAIT, self.listener.accept())
            .await
            .unwrap()
            .unwrap();
        accept_async(stream).await.unwrap()
    }

    async fn text(&mut self, peer: &mut Peer, op: u64, data: Value) {
        self.seq += 1;
        peer.send(Message::Text(
            json!({"op": op, "seq": self.seq, "d": data})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    }

    async fn binary(&mut self, peer: &mut Peer, opcode: u8, payload: &[u8]) {
        self.seq += 1;
        let mut message = self.seq.to_be_bytes().to_vec();
        message.push(opcode);
        message.extend_from_slice(payload);
        peer.send(Message::Binary(message.into())).await.unwrap();
    }
}

/// The next message from the client that is not a heartbeat; heartbeats are
/// answered, and their `seq_ack` recorded.
async fn next(peer: &mut Peer, acks: &mut Vec<i64>) -> Result<Value, Vec<u8>> {
    loop {
        let message = timeout(WAIT, peer.next()).await.unwrap().unwrap().unwrap();
        match message {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["op"] == 3 {
                    acks.push(value["d"]["seq_ack"].as_i64().unwrap());
                    let nonce = value["d"]["t"].clone();
                    peer.send(Message::Text(
                        json!({"op": 6, "d": {"t": nonce}}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                    continue;
                }
                return Ok(value);
            }
            Message::Binary(bytes) => return Err(bytes.to_vec()),
            _ => {}
        }
    }
}

/// Answer heartbeats for `period`, ignoring anything else.
async fn drain(peer: &mut Peer, acks: &mut Vec<i64>, period: Duration) {
    let _ = timeout(period, async {
        loop {
            let _ = next(peer, acks).await;
        }
    })
    .await;
}

async fn expect(peer: &mut Peer, acks: &mut Vec<i64>, op: u64) -> Value {
    let value = next(peer, acks).await.expect("a JSON message");
    assert_eq!(value["op"], op, "{value}");
    value["d"].clone()
}

async fn phase(phases: &mut watch::Receiver<CallPhase>, wanted: CallPhase) {
    let reached = timeout(WAIT, phases.wait_for(|phase| *phase == wanted))
        .await
        .is_ok();
    assert!(reached, "never reached {wanted:?}: {:?}", *phases.borrow());
}

fn credentials(url: String) -> Credentials {
    Credentials {
        server: GUILD,
        channel: CHANNEL,
        user: ALICE,
        session: Zeroizing::new("SYNTHETIC_VOICE_SESSION".into()),
        token: Zeroizing::new("SYNTHETIC_VOICE_TOKEN".into()),
        url,
        codes: None,
        local: true,
    }
}

#[tokio::test]
async fn a_call_handshakes_secures_carries_audio_both_ways_resumes_and_leaves() {
    let mut server = Server::new().await;
    let delivery = Proposer::new();
    let (port, mut link) = voice_link();
    let (controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    // Desktop (P6): the call's codes.
    let (privacy, mut told) = watch::channel(None);
    let (verify, asked) = tokio::sync::mpsc::channel(4);
    let call = tokio::spawn(connection::run(
        Credentials {
            codes: Some(connection::Codes {
                privacy,
                verify: asked,
                quality: watch::channel(None).0,
            }),
            ..credentials(server.url())
        },
        port.clone(),
        None,
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;

    // Hello → Identify. The client beats at once, then every 5 s (its cap):
    // a dead server takes two silent beats, longer than any step's WAIT, so
    // the steps that wait on UDP without reading the socket are never taken
    // for one, whatever the load (with 200 ms beats a starved CPU did).
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(identify["server_id"], GUILD.to_string());
    assert_eq!(identify["user_id"], ALICE.to_string());
    assert_eq!(identify["session_id"], "SYNTHETIC_VOICE_SESSION");
    assert_eq!(identify["token"], "SYNTHETIC_VOICE_TOKEN");
    assert_eq!(identify["max_dave_protocol_version"], 1);

    // Ready → IP discovery → Select Protocol.
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port,
            "modes": ["aead_aes256_gcm_rtpsize", rtp::MODE]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (length, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&datagram[..length], &rtp::discovery_request(11));
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(select["protocol"], "udp");
    assert_eq!(select["data"]["address"], "127.0.0.1");
    assert_eq!(select["data"]["port"], client.port());
    assert_eq!(select["data"]["mode"], rtp::MODE);

    // Session Description: the SSRC is announced and a key package goes out.
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1}),
        )
        .await;
    let speaking = expect(&mut peer, &mut acks, 5).await;
    assert_eq!(speaking, json!({"speaking": 0, "delay": 0, "ssrc": 11}));
    let package = next(&mut peer, &mut acks)
        .await
        .expect_err("binary key package");
    // Alone, the transport alone makes the call connected, waiting for
    // the others (voz-protocolo).
    phase(&mut phases, CallPhase::Waiting).await;

    // Bob arrives: external sender, proposal, the client's commit, then its announcement.
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    bob.connected(&[ALICE]).unwrap();
    bob.external_sender(&delivery.announcement()).unwrap();
    server
        .text(&mut peer, 11, json!({"user_ids": [BOB.to_string()]}))
        .await;
    server
        .binary(&mut peer, op::EXTERNAL_SENDER, &delivery.announcement())
        .await;
    // The fixture needs the client's group to address a proposal to it:
    // rebuild the same pending group from its key package's owner view.
    let mut mirror = Dave::new(ALICE, CHANNEL).unwrap();
    mirror.external_sender(&delivery.announcement()).unwrap();
    assert_eq!(package[0], op::KEY_PACKAGE);
    let proposals = delivery.adding(&mirror, &bob.key_package().unwrap());
    server.binary(&mut peer, op::PROPOSALS, &proposals).await;
    let answer = next(&mut peer, &mut acks).await.expect_err("binary commit");
    let (commit, welcome) = split(&answer);
    server
        .binary(&mut peer, op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .await;
    assert!(bob
        .group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap()
        .is_empty());
    assert!(bob.ready());
    // With Bob in the group, it no longer waits.
    phase(&mut phases, CallPhase::Connected).await;
    // Desktop (P6): the call's privacy code is Bob's too, and the code
    // Alice compares with Bob is the one Bob computes.
    let shown = timeout(WAIT, told.wait_for(|code| code.is_some()))
        .await
        .expect("a privacy code")
        .unwrap()
        .clone();
    assert_eq!(shown.as_deref(), bob.privacy_code());
    let (answer, answered) = tokio::sync::oneshot::channel();
    verify.send((BOB, answer)).await.unwrap();
    assert_eq!(
        timeout(WAIT, answered).await.unwrap().unwrap(),
        super::dave::verification_code(bob.fingerprints(ALICE).unwrap())
    );

    // Out: the host's frame is end-to-end and transport encrypted.
    let transport = Transport::new(&Zeroizing::new(KEY));
    let mut sent = None;
    for _ in 0..50 {
        link.outgoing.try_send(b"synthetic opus out".to_vec()).ok();
        if let Ok(Ok((length, _))) = timeout(
            Duration::from_millis(100),
            server.udp.recv_from(&mut datagram),
        )
        .await
        {
            if let Some(packet) = transport.open(&datagram[..length]) {
                sent = Some(packet);
                break;
            }
        }
    }
    let sent = sent.expect("a sealed RTP packet");
    assert_eq!(sent.ssrc, 11);
    assert_eq!(
        bob.decrypt(ALICE, &sent.payload).unwrap(),
        b"synthetic opus out"
    );
    let speaking = expect(&mut peer, &mut acks, 5).await;
    assert_eq!(speaking["speaking"], 1);
    // The spurt ends with silence frames and speaking off.
    let mut silences = 0;
    while silences < 5 {
        let (length, _) = timeout(WAIT, server.udp.recv_from(&mut datagram))
            .await
            .unwrap()
            .unwrap();
        match transport.open(&datagram[..length]) {
            Some(packet) if packet.payload == [0xf8, 0xff, 0xfe] => silences += 1,
            _ => {}
        }
    }
    assert_eq!(expect(&mut peer, &mut acks, 5).await["speaking"], 0);

    // In: Bob's frame reaches the host as his, in order, decrypted.
    server
        .text(
            &mut peer,
            5,
            json!({"user_id": BOB.to_string(), "ssrc": 22, "speaking": 1}),
        )
        .await;
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    let mut received = None;
    for sequence in 0..50u16 {
        let frame = bob.encrypt(b"synthetic opus in").unwrap();
        let packet = sealer
            .seal(
                &rtp::header(sequence, u32::from(sequence) * 960, 22),
                &frame,
            )
            .unwrap();
        server.udp.send_to(&packet, client).await.unwrap();
        if let Ok(Some(packet)) = timeout(Duration::from_millis(100), link.incoming.recv()).await {
            received = Some(packet);
            break;
        }
    }
    let received = received.expect("a decrypted packet for the host");
    assert_eq!(received.user, UserId(BOB));
    assert_eq!(received.opus, b"synthetic opus in");
    // A forged datagram never reaches the host.
    let mut forged = sealer
        .seal(&rtp::header(60, 0, 22), &bob.encrypt(b"x").unwrap())
        .unwrap();
    forged[14] ^= 1;
    server.udp.send_to(&forged, client).await.unwrap();
    assert!(timeout(Duration::from_millis(200), link.incoming.recv())
        .await
        .is_err());
    // Everything sent so far was read: Bob's speaking last, before his audio
    // reached the host as his.
    let read = server.seq;

    // The voice server restarts (4015): the call resumes on a new socket.
    peer.close(Some(CloseFrame {
        code: CloseCode::from(4015),
        reason: "".into(),
    }))
    .await
    .unwrap();
    phase(&mut phases, CallPhase::Reconnecting { attempt: 1 }).await;
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let resume = expect(&mut peer, &mut acks, 7).await;
    assert_eq!(resume["server_id"], GUILD.to_string());
    assert_eq!(resume["session_id"], "SYNTHETIC_VOICE_SESSION");
    assert_eq!(resume["token"], "SYNTHETIC_VOICE_TOKEN");
    // It acknowledges the last message read, as its heartbeats do (the
    // same `seq_ack`): not merely the Hello's.
    assert_eq!(resume["seq_ack"], json!(read));
    server.text(&mut peer, 9, json!(null)).await;
    phase(&mut phases, CallPhase::Connected).await;

    // Muted: nothing the host still sends goes out.
    controls.send_replace(Controls {
        muted: true,
        ..Controls::default()
    });
    drain(&mut peer, &mut acks, Duration::from_millis(100)).await;
    while server.udp.try_recv_from(&mut datagram).is_ok() {}
    link.outgoing.try_send(b"muted".to_vec()).unwrap();
    drain(&mut peer, &mut acks, Duration::from_millis(150)).await;
    while let Ok((length, _)) = server.udp.try_recv_from(&mut datagram) {
        if let Some(packet) = transport.open(&datagram[..length]) {
            assert_ne!(
                bob.decrypt(ALICE, &packet.payload).as_deref(),
                Some(&b"muted"[..])
            );
        }
    }

    // Leaving drops the controls: the call ends and lets go of the link.
    drop(controls);
    assert_eq!(timeout(WAIT, call).await.unwrap().unwrap(), Ok(()));
    assert!(port.outgoing.try_lock().is_ok(), "the next call can send");
    drop(peer);
}

#[tokio::test]
async fn fatal_closes_and_exhausted_resumes_end_the_call_with_a_reason() {
    for (code, reason) in [
        (4014, "Discord te desconectó de la llamada."),
        (
            4017,
            "Discord exige cifrado de extremo a extremo que no se pudo negociar.",
        ),
    ] {
        let mut server = Server::new().await;
        let (port, _link) = voice_link();
        let (_controls, watched) = watch::channel(Controls::default());
        let (phase_tx, _phases) = watch::channel(CallPhase::Joining);
        let call = tokio::spawn(connection::run(
            credentials(server.url()),
            port,
            None,
            watched,
            phase_tx,
        ));
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        peer.close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        }))
        .await
        .unwrap();
        assert_eq!(timeout(WAIT, call).await.unwrap().unwrap(), Err(reason));
    }
    // Before a session exists there is nothing to resume.
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let (_controls, watched) = watch::channel(Controls::default());
    let (phase_tx, _phases) = watch::channel(CallPhase::Joining);
    let call = tokio::spawn(connection::run(
        credentials(server.url()),
        port,
        None,
        watched,
        phase_tx,
    ));
    drop(server.accept().await);
    assert!(timeout(WAIT, call).await.unwrap().unwrap().is_err());
    // A server without the required encryption is refused.
    let mut server = Server::new().await;
    let (port, _link) = voice_link();
    let (_controls, watched) = watch::channel(Controls::default());
    let (phase_tx, _phases) = watch::channel(CallPhase::Joining);
    let call = tokio::spawn(connection::run(
        credentials(server.url()),
        port,
        None,
        watched,
        phase_tx,
    ));
    let mut peer = server.accept().await;
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 1, "ip": "127.0.0.1", "port": 9, "modes": ["xsalsa20_poly1305"]}),
        )
        .await;
    assert_eq!(
        timeout(WAIT, call).await.unwrap().unwrap(),
        Err("El servidor de voz no ofrece el cifrado requerido.")
    );
}

fn snapshot() -> Snapshot {
    let me = crate::wire::user(&json!({"id": ALICE.to_string(), "username": "Local"})).unwrap();
    crate::bootstrap::ready(
        &json!({
            "user": {"id": ALICE.to_string(), "username": "Local"},
            "guilds": [{
                "id": GUILD.to_string(), "name": "Synthetic",
                "members": [{"user": {"id": ALICE.to_string(), "username": "Local"}, "roles": []}],
                // @everyone may see, connect and speak.
                "roles": [{"id": GUILD.to_string(), "name": "@everyone", "permissions": "3146752"}],
                "channels": [
                    {"id": CHANNEL.to_string(), "type": 2, "name": "General", "permission_overwrites": []},
                    {"id": STAGE.to_string(), "type": 13, "name": "Escenario", "permission_overwrites": []},
                ],
                "voice_states": [{"user_id": "77", "channel_id": CHANNEL.to_string(), "self_mute": true}],
            }],
            "private_channels": [{"id": "50", "type": 1, "recipients": [{"id": "2", "username": "Bob"}]}],
        }),
        &me,
    )
    .unwrap()
}

#[tokio::test]
async fn joining_asks_the_gateway_then_connects_with_what_it_answers_and_leaving_releases_it() {
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    assert_eq!(
        snapshot.voice.len(),
        1,
        "a guild arrives with who is in voice"
    );
    assert!(
        calls.join(&snapshot, ChannelId(999), false, false).is_err(),
        "a channel not in the list is refused"
    );
    let events = calls
        .join(&snapshot, ChannelId(CHANNEL), true, false)
        .unwrap();
    assert!(matches!(
        events[..],
        [Event::CallPhase {
            phase: CallPhase::Joining,
            ..
        }]
    ));
    assert_eq!(
        *wanted.borrow_and_update(),
        Some(VoiceWant {
            guild: Some(GUILD),
            channel: Some(CHANNEL),
            muted: true,
            deafened: false,
            request: 1,
            streams: [None; MAX_STREAMS],
            share: None,
            share_request: 0,
            camera: false,
            share_paused: false,
        })
    );
    // The service answers with the account's state and the voice server.
    let events = calls
        .dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &json!({
                "guild_id": GUILD.to_string(), "channel_id": CHANNEL.to_string(),
                "user_id": ALICE.to_string(), "session_id": "SYNTHETIC_VOICE_SESSION",
                "self_mute": true, "self_deaf": false,
            }),
        )
        .unwrap();
    assert!(
        matches!(&events[..], [Event::VoiceChanged(state)] if state.user == UserId(ALICE) && state.muted)
    );
    calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &json!({
        "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
    }));
    let handshake = tokio::spawn(async move {
        let mut server = server;
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (peer, identify)
    });
    // The connection runs while the provider polls the call.
    while !handshake.is_finished() {
        let _ = timeout(Duration::from_millis(20), calls.next()).await;
    }
    let (mut peer, identify) = handshake.await.unwrap();
    assert_eq!(identify["session_id"], "SYNTHETIC_VOICE_SESSION");
    // Someone else's state is just theirs.
    let events = calls
        .dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &json!({
            "guild_id": GUILD.to_string(), "channel_id": CHANNEL.to_string(), "user_id": BOB.to_string(),
            "self_deaf": true,
        }))
        .unwrap();
    assert!(
        matches!(&events[..], [Event::VoiceChanged(state)] if state.user == UserId(BOB) && state.muted && state.deafened)
    );
    assert!(calls
        .dispatch(&mut snapshot, "MESSAGE_CREATE", &json!({}))
        .is_none());
    // Mute and deafen go to the service too.
    calls.set_self(false, true);
    assert!(wanted.borrow_and_update().unwrap().deafened);
    // Leaving: the service is told, the connection ends, the account leaves.
    let events = calls.leave();
    assert!(matches!(
        &events[..],
        [
            Event::VoiceLeft {
                user: UserId(ALICE),
                ..
            },
            Event::CallEnded { reason: None, .. }
        ]
    ));
    assert_eq!(wanted.borrow_and_update().unwrap().channel, None);
    let closed = timeout(WAIT, async {
        loop {
            match peer.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                _ => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the voice socket closes with the call");
    assert!(
        timeout(Duration::from_millis(50), calls.next())
            .await
            .is_err(),
        "nothing more to say"
    );
}

#[tokio::test]
async fn a_voice_server_that_never_comes_ends_the_join_and_a_bad_endpoint_is_refused() {
    tokio::time::pause();
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let _wanted = calls.gateway();
    let mut snapshot = snapshot();
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    // A null endpoint (being allocated) waits; nothing more in time ends it.
    let events = calls.dispatch(
        &mut snapshot,
        "VOICE_SERVER_UPDATE",
        &json!({
            "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": null,
        }),
    );
    assert!(events.is_some_and(|events| events.is_empty()));
    let events = calls.next().await;
    assert!(matches!(
        &events[..],
        [Event::VoiceLeft { .. }, Event::CallEnded { reason: Some(reason), .. }] if reason.contains("a tiempo")
    ));
    // A server outside Discord's media hosts ends the join at once, with why.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let events = calls
        .dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &json!({
            "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "voice.example.com",
        }))
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::VoiceLeft { .. }, Event::CallEnded { reason: Some(reason), .. }] if reason.contains("servidor de voz inválido")
    ));
    // One on another port is Discord's too: no end.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let events = calls
        .dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &json!({
            "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-eze02-1a2b.discord.media:2087",
        }))
        .unwrap();
    assert!(events.is_empty());
    calls.leave();
    // Removed by a moderator once the call's voice server accepted it: the
    // call ends and says so.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    calls.call.as_mut().unwrap().confirmed = true;
    let events = calls
        .dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &json!({
                "guild_id": GUILD.to_string(), "channel_id": null, "user_id": ALICE.to_string(),
            }),
        )
        .unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::CallEnded {
            reason: Some(_),
            ..
        }
    )));
}

#[test]
fn guild_voice_states_are_bounded_and_malformed_ones_skipped() {
    let mut snapshot = Snapshot::default();
    let rows: Vec<Value> = (1..=super::MAX_STATES as u64 + 10)
        .map(|user| json!({"user_id": user.to_string(), "channel_id": "7", "self_mute": true}))
        .chain([json!({"user_id": "x"}), json!({"channel_id": "7"})])
        .collect();
    super::guild_states(&mut snapshot, &json!({"voice_states": rows}));
    assert_eq!(snapshot.voice.len(), super::MAX_STATES);
    assert!(snapshot
        .voice
        .iter()
        .all(|state| state.muted && state.channel == ChannelId(7)));
}

#[test]
fn other_guilds_voice_reaches_the_store_with_streams_moves_and_leaves() {
    let mut snapshot = snapshot();
    let mut calls = Calls::new(None);
    // A guild that arrives after READY brings who is in voice there.
    let guild = |voice: Value| {
        json!({
            "id": "41", "name": "Other",
            "members": [{"user": {"id": ALICE.to_string(), "username": "Local"}, "roles": []}],
            "roles": [{"id": "41", "name": "@everyone", "permissions": "3146752"}],
            "channels": [
                {"id": "31", "type": 2, "name": "Sala", "permission_overwrites": []},
                {"id": "32", "type": 2, "name": "Otra", "permission_overwrites": []},
            ],
            "voice_states": voice,
        })
    };
    let events = crate::bootstrap::update(
        &mut snapshot,
        "GUILD_CREATE",
        &guild(json!([
            {"user_id": "88", "channel_id": "31", "self_stream": true},
            {"user_id": "89", "channel_id": "31", "self_video": true},
        ])),
    )
    .unwrap();
    let joined: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::VoiceChanged(state) => Some((state.user.0, state.streaming, state.camera)),
            _ => None,
        })
        .collect();
    assert_eq!(joined, [(88, true, false), (89, false, true)]);
    // Someone moves within that guild and someone leaves it.
    let moved = calls
        .dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &json!({
                "guild_id": "41", "channel_id": "32", "user_id": "89",
            }),
        )
        .unwrap();
    assert!(matches!(&moved[..], [Event::VoiceChanged(state)] if state.channel == ChannelId(32)));
    let left = calls
        .dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &json!({
                "guild_id": "41", "channel_id": null, "user_id": "88",
            }),
        )
        .unwrap();
    assert!(matches!(&left[..], [Event::VoiceLeft { user, .. }] if user.0 == 88));
    // The guild comes back (after an outage) without 89: they left meanwhile.
    let events =
        crate::bootstrap::update(&mut snapshot, "GUILD_CREATE", &guild(json!([]))).unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::VoiceLeft { user, .. } if user.0 == 89)));
    assert_eq!(
        snapshot.voice.len(),
        1,
        "only the first guild's voice stays"
    );
}

/// The account's own voice state, from `session`, in `channel`.
fn own(session: &str, channel: u64) -> Value {
    json!({
        "guild_id": GUILD.to_string(), "channel_id": channel.to_string(),
        "user_id": ALICE.to_string(), "session_id": session,
    })
}

#[test]
fn a_server_mute_or_deafen_is_kept_apart_from_the_account_s_own() {
    let state = |data: Value| super::state(&data).unwrap().1.unwrap();
    let held = state(json!({"user_id": "5", "channel_id": "7", "mute": true}));
    assert!(held.server_muted && !held.muted && !held.server_deafened);
    let quiet = state(json!({"user_id": "5", "channel_id": "7", "deaf": true, "self_mute": true}));
    assert!(quiet.server_deafened && quiet.muted && !quiet.deafened && !quiet.server_muted);
    let audience = state(json!({"user_id": "5", "channel_id": "7", "suppress": true}));
    assert!(audience.server_muted && !audience.muted);
    // A stage's seat, read as the official client reads it.
    use rune_model::StageSeat;
    assert_eq!(audience.stage_seat(), StageSeat::Audience);
    let hand = state(json!({"user_id": "5", "channel_id": "7", "suppress": true,
        "request_to_speak_timestamp": "2026-10-17T12:00:00.000Z"}));
    assert_eq!(hand.stage_seat(), StageSeat::HandRaised);
    let invited = state(json!({"user_id": "5", "channel_id": "7", "suppress": false,
        "request_to_speak_timestamp": "2026-10-17T12:00:00.000Z"}));
    assert_eq!(invited.stage_seat(), StageSeat::Invited);
    assert!(
        invited.server_muted,
        "held until the invitation is accepted"
    );
    let speaker = state(json!({"user_id": "5", "channel_id": "7", "suppress": false,
        "request_to_speak_timestamp": null}));
    assert!(speaker.stage_seat() == StageSeat::Speaker && !speaker.server_muted);
}

#[tokio::test]
async fn a_stage_is_joined_like_a_voice_room() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let snapshot = snapshot();
    let stage = snapshot.channels.iter().find(|c| c.id == ChannelId(STAGE));
    assert_eq!(stage.map(|c| c.kind), Some(rune_model::ChannelKind::Stage));
    assert!(calls
        .join(&snapshot, ChannelId(STAGE), false, false)
        .is_ok());
}

#[tokio::test]
async fn another_device_taking_the_call_ends_it_here_without_hanging_it_up() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    for elsewhere in [CHANNEL, 99] {
        calls
            .join(&snapshot, ChannelId(CHANNEL), false, false)
            .unwrap();
        // This device's session, once its voice server accepted it.
        let events = calls
            .dispatch(
                &mut snapshot,
                "VOICE_STATE_UPDATE",
                &own("THIS_DEVICE", CHANNEL),
            )
            .unwrap();
        assert!(!events
            .iter()
            .any(|event| matches!(event, Event::CallEnded { .. })));
        calls.call.as_mut().unwrap().confirmed = true;
        wanted.borrow_and_update();
        // Then the account's voice goes on with another session: a phone.
        let events = calls
            .dispatch(
                &mut snapshot,
                "VOICE_STATE_UPDATE",
                &own("PHONE", elsewhere),
            )
            .unwrap();
        assert!(
            matches!(
                &events[..],
                [Event::VoiceChanged(state), Event::CallEnded { reason: Some(reason), .. }]
                    if state.user == UserId(ALICE) && reason == super::TAKEN_OVER
            ),
            "{events:?}"
        );
        // A leave would hang up the phone: nothing is told.
        assert_eq!(*wanted.borrow_and_update(), None);
        assert!(calls.call.is_none());
    }
}

#[tokio::test]
async fn a_fresh_gateway_session_asks_for_the_call_again_unless_another_device_has_it() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    calls
        .join(&snapshot, ChannelId(CHANNEL), true, false)
        .unwrap();
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("OLD_SESSION", CHANNEL),
    );
    let asked = wanted.borrow_and_update().unwrap();
    let ready = |session: &str| {
        json!({"guilds": [{"id": GUILD.to_string(), "voice_states": [
            {"user_id": ALICE.to_string(), "channel_id": CHANNEL.to_string(), "session_id": session},
        ]}]})
    };
    calls.call.as_mut().unwrap().confirmed = true;
    // Still listed with this device's session: asked again, as it was.
    assert!(matches!(
        &calls.ready(&ready("OLD_SESSION"))[..],
        [Event::CallPhase {
            phase: CallPhase::Joining,
            ..
        }]
    ));
    let again = wanted.borrow_and_update().unwrap();
    assert_eq!((again.channel, again.muted), (Some(CHANNEL), true));
    assert_ne!(again.request, asked.request);
    // The new Gateway session brings a new voice session: this device's.
    let events = calls
        .dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &own("NEW_SESSION", CHANNEL),
        )
        .unwrap();
    assert!(!events
        .iter()
        .any(|event| matches!(event, Event::CallEnded { .. })));
    // Not yet accepted by its voice server: another outage only asks again.
    assert!(!calls
        .ready(&ready("PHONE"))
        .iter()
        .any(|event| matches!(event, Event::CallEnded { .. })));
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("NEW_SESSION", CHANNEL),
    );
    calls.call.as_mut().unwrap().confirmed = true;
    // Another outage, and meanwhile a phone took the call: not taken back.
    let events = calls.ready(&ready("PHONE"));
    assert!(matches!(
        &events[..],
        [Event::CallEnded { reason: Some(reason), .. }] if reason == super::TAKEN_OVER
    ));
    assert_eq!(*wanted.borrow_and_update(), None);
}

/// Stand in for the connection `start` would run: its phases, and a way to
/// lose it after every resumption failed.
fn connect(calls: &mut Calls) -> (watch::Sender<CallPhase>, tokio::sync::oneshot::Sender<()>) {
    connect_ending(calls, connection::LOST)
}

/// [`connect`], lost with `reason`.
fn connect_ending(
    calls: &mut Calls,
    reason: &'static str,
) -> (watch::Sender<CallPhase>, tokio::sync::oneshot::Sender<()>) {
    let call = calls.call.as_mut().unwrap();
    let (phase, phases) = watch::channel(CallPhase::Joining);
    let (lose, lost) = tokio::sync::oneshot::channel::<()>();
    call.phase = Some(phases);
    call.deadline = None;
    call.connection = Some(Box::pin(async move {
        let _ = lost.await;
        Err(reason)
    }));
    (phase, lose)
}

async fn reconnecting(calls: &mut Calls) -> u32 {
    match &calls.next().await[..] {
        [Event::CallPhase {
            phase: CallPhase::Reconnecting { attempt },
            ..
        }] => *attempt,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_lost_connection_rejoins_three_times_then_ends_and_a_stable_call_earns_its_budget_back() {
    use tokio::time::Instant;
    tokio::time::pause();
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut wanted = calls.gateway();
    let snapshot = snapshot();
    // Lost before it ever connected: no rejoin.
    calls
        .join(&snapshot, ChannelId(CHANNEL), true, false)
        .unwrap();
    let (_phase, lose) = connect(&mut calls);
    let _ = lose.send(());
    assert!(matches!(
        &calls.next().await[..],
        [
            Event::VoiceLeft { .. },
            Event::CallEnded {
                reason: Some(_),
                ..
            }
        ]
    ));

    calls
        .join(&snapshot, ChannelId(CHANNEL), true, false)
        .unwrap();
    let (phase, lose) = connect(&mut calls);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let before = wanted.borrow_and_update().unwrap();
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 1);
    assert!(
        !wanted.has_changed().unwrap(),
        "asked again only after a wait"
    );
    let lost_at = Instant::now();
    assert!(calls.next().await.is_empty());
    assert!((Duration::from_secs(1)..Duration::from_millis(1_010)).contains(&lost_at.elapsed()));
    let again = wanted.borrow_and_update().unwrap();
    assert_eq!((again.channel, again.muted), (Some(CHANNEL), true));
    assert_ne!(again.request, before.request);
    // No voice server answers in time: the next attempts wait longer.
    for (attempt, wait) in [(2, 3), (3, 10)] {
        assert_eq!(reconnecting(&mut calls).await, attempt);
        let lost_at = Instant::now();
        assert!(calls.next().await.is_empty());
        let waited = lost_at.elapsed().as_millis();
        assert!(
            (wait * 1_000..wait * 1_000 + 10).contains(&waited),
            "{waited}"
        );
        assert!(wanted.has_changed().unwrap());
    }
    assert!(matches!(
        &calls.next().await[..],
        [
            Event::VoiceLeft { .. },
            Event::CallEnded {
                reason: Some(_),
                ..
            }
        ]
    ));

    // A rejoined call lost again at once spends the next attempt; one that
    // stayed connected a while starts over.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let (phase, lose) = connect(&mut calls);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 1);
    calls.next().await;
    let (phase, lose) = connect(&mut calls);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 2);
    calls.next().await;
    let (phase, lose) = connect(&mut calls);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    tokio::time::advance(super::REJOIN_STABLE).await;
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 1);
}

/// RTP payloads of an Annex B access unit: each NAL whole, or in FU-A
/// pieces of at most `size` bytes.
fn packetize(unit: &[u8], size: usize) -> Vec<Vec<u8>> {
    let mut nals = Vec::new();
    let (mut start, mut at) = (None, 0);
    while at + 3 <= unit.len() {
        if unit[at..at + 3] == [0, 0, 1] {
            if let Some(from) = start {
                let end = if unit[at - 1] == 0 { at - 1 } else { at };
                nals.push(&unit[from..end]);
            }
            start = Some(at + 3);
            at += 3;
        } else {
            at += 1;
        }
    }
    nals.extend(start.map(|from| &unit[from..]));
    let mut payloads = Vec::new();
    for nal in nals {
        if nal.len() <= size {
            payloads.push(nal.to_vec());
            continue;
        }
        let pieces: Vec<&[u8]> = nal[1..].chunks(size).collect();
        for (index, piece) in pieces.iter().enumerate() {
            let mut header = nal[0] & 0x1f;
            if index == 0 {
                header |= 0x80;
            }
            if index == pieces.len() - 1 {
                header |= 0x40;
            }
            payloads.push([&[(nal[0] & 0xe0) | 28, header], *piece].concat());
        }
    }
    payloads
}

/// `count` pictures of a flat color, H264 as OpenH264 encodes them.
fn encoded(count: usize) -> Vec<Vec<u8>> {
    use openh264::formats::{RgbSliceU8, YUVBuffer};
    let mut encoder = openh264::encoder::Encoder::new().unwrap();
    let pixels = [30u8, 160, 60].repeat(320 * 180);
    let yuv = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&pixels, (320, 180)));
    (0..count)
        .map(|_| encoder.encode(&yuv).unwrap().to_vec())
        .collect()
}

/// The PLI an RTCP datagram carries, opened: sender and media SSRCs.
fn pli(packet: &[u8]) -> Option<(u32, u32)> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        XChaCha20Poly1305,
    };
    if packet.len() < 8 + 16 + 4 || packet[..4] != [0x81, 206, 0, 2] {
        return None;
    }
    let mut nonce = [0; 24];
    nonce[..4].copy_from_slice(&packet[packet.len() - 4..]);
    let body = XChaCha20Poly1305::new(&KEY.into())
        .decrypt(
            &nonce.into(),
            Payload {
                msg: &packet[8..packet.len() - 4],
                aad: &packet[..8],
            },
        )
        .ok()?;
    Some((
        u32::from_be_bytes(packet[4..8].try_into().ok()?),
        u32::from_be_bytes(body.try_into().ok()?),
    ))
}

/// The NACK an RTCP datagram carries, opened: sender and media SSRCs and
/// the sequence numbers asked for.
fn nacked(packet: &[u8]) -> Option<(u32, u32, Vec<u16>)> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        XChaCha20Poly1305,
    };
    if packet.len() < 8 + 16 + 4 || packet[..2] != [0x81, 205] {
        return None;
    }
    let mut nonce = [0; 24];
    nonce[..4].copy_from_slice(&packet[packet.len() - 4..]);
    let body = XChaCha20Poly1305::new(&KEY.into())
        .decrypt(
            &nonce.into(),
            Payload {
                msg: &packet[8..packet.len() - 4],
                aad: &packet[..8],
            },
        )
        .ok()?;
    let lost = body[4..]
        .chunks_exact(4)
        .flat_map(|entry| {
            super::nack::lost(
                u16::from_be_bytes([entry[0], entry[1]]),
                u16::from_be_bytes([entry[2], entry[3]]),
            )
        })
        .collect();
    Some((
        u32::from_be_bytes(packet[4..8].try_into().ok()?),
        u32::from_be_bytes(body[..4].try_into().ok()?),
        lost,
    ))
}

/// The next sealed NACK the synthetic server receives.
async fn next_nack(server: &Server) -> (u32, u32, Vec<u16>) {
    let mut datagram = [0u8; 4096];
    timeout(WAIT, async {
        loop {
            let (length, _) = server.udp.recv_from(&mut datagram).await.unwrap();
            if let Some(found) = nacked(&datagram[..length]) {
                return found;
            }
        }
    })
    .await
    .unwrap()
}

/// The next sealed PLI the synthetic server receives.
async fn next_pli(server: &Server) -> (u32, u32) {
    let mut datagram = [0u8; 4096];
    timeout(WAIT, async {
        loop {
            let (length, _) = server.udp.recv_from(&mut datagram).await.unwrap();
            if let Some(found) = pli(&datagram[..length]) {
                return found;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_watched_stream_brings_decryptable_h264_and_its_sound_and_a_loss_asks_for_a_key_frame() {
    use openh264::formats::YUVSource;
    use rune_provider::video::video_link;
    let mut server = Server::new().await;
    let delivery = Proposer::new();
    let (port, mut link) = voice_link();
    let (video, mut frames) = video_link();
    let (keys, asked) = tokio::sync::mpsc::channel(8);
    let video = Video {
        kind: rune_model::VideoKind::Screen,
        incoming: video.incoming,
        keys: asked,
        wanted: None,
        send: None,
    };
    let (_controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    // The stream's own voice server: its id stands for the guild's.
    let stream = tokio::spawn(connection::run(
        Credentials {
            server: 900,
            ..credentials(server.url())
        },
        port.clone(),
        Some(video),
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(identify["server_id"], "900");
    assert_eq!(identify["video"], true);
    assert_eq!(identify["streams"], json!([]));
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (_, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(
        select["codecs"][1],
        json!({"name": "H264", "type": "video", "priority": 1000, "payload_type": 101,
            "rtx_payload_type": 102, "encode": false, "decode": true})
    );
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1}),
        )
        .await;
    expect(&mut peer, &mut acks, 5).await;
    next(&mut peer, &mut acks).await.expect_err("key package");
    assert_eq!(expect(&mut peer, &mut acks, 15).await, json!({"any": 100}));
    // Bob, who streams, joins the stream's group.
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    bob.connected(&[ALICE]).unwrap();
    bob.external_sender(&delivery.announcement()).unwrap();
    server
        .text(&mut peer, 11, json!({"user_ids": [BOB.to_string()]}))
        .await;
    server
        .binary(&mut peer, op::EXTERNAL_SENDER, &delivery.announcement())
        .await;
    let mut mirror = Dave::new(ALICE, CHANNEL).unwrap();
    mirror.external_sender(&delivery.announcement()).unwrap();
    let proposals = delivery.adding(&mirror, &bob.key_package().unwrap());
    server.binary(&mut peer, op::PROPOSALS, &proposals).await;
    let answer = next(&mut peer, &mut acks).await.expect_err("commit");
    let (commit, welcome) = split(&answer);
    server
        .binary(&mut peer, op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .await;
    bob.group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap();
    phase(&mut phases, CallPhase::Connected).await;
    // His streams: sound on 22, the screen on 23.
    server
        .text(
            &mut peer,
            12,
            json!({"user_id": BOB.to_string(), "audio_ssrc": 22, "video_ssrc": 23,
                "streams": [{"type": "screen", "rid": "100", "ssrc": 23, "rtx_ssrc": 24, "active": true}]}),
        )
        .await;
    assert_eq!(expect(&mut peer, &mut acks, 15).await, json!({"any": 100}));

    // Pictures end-to-end encrypted with H264's clear ranges, in RTP.
    let pictures = encoded(2);
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    let mut sequence = 100u16;
    // A piece skipped comes back as its retransmission, for when it is asked.
    let mut send = |bob: &mut Dave, picture: &[u8], skip: Option<usize>, timestamp: u32| {
        let sealed = bob
            .session
            .encrypt(davey::MediaType::VIDEO, davey::Codec::H264, picture)
            .unwrap()
            .into_owned();
        let payloads = packetize(&sealed, 300);
        let last = payloads.len() - 1;
        let mut packets = Vec::new();
        let mut skipped = None;
        for (index, payload) in payloads.iter().enumerate() {
            let mut header = rtp::header(sequence, timestamp, 23);
            header[1] = rtp::H264 | if index == last { 0x80 } else { 0 };
            if Some(index) != skip {
                packets.push(sealer.seal(&header, payload).unwrap());
            } else {
                let (header, payload) = super::nack::rtx(&(header, payload.clone()), 24, 7);
                skipped = Some((sequence, sealer.seal(&header, &payload).unwrap()));
            }
            sequence = sequence.wrapping_add(1);
        }
        (packets, skipped)
    };
    let (first, _) = send(&mut bob, &pictures[0], None, 0);
    assert!(first.len() > 2, "the key frame goes in pieces");
    let (second, _) = send(&mut bob, &pictures[1], None, 3_000);
    for packet in first.iter().chain(&second) {
        server.udp.send_to(packet, client).await.unwrap();
    }
    for (index, picture) in pictures[..2].iter().enumerate() {
        let unit = timeout(WAIT, frames.incoming.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unit.user, UserId(BOB));
        assert_eq!(unit.kind, rune_model::VideoKind::Screen);
        assert_eq!(unit.keyframe, index == 0);
        assert_eq!(&unit.data, picture, "picture {index} decrypted as encoded");
        if index == 0 {
            let mut decoder = openh264::decoder::Decoder::new().unwrap();
            let decoded = decoder.decode(&unit.data).unwrap().unwrap();
            assert_eq!(decoded.dimensions(), (320, 180));
        }
    }

    // A piece lost is asked for again (NACK); its retransmission on the RTX
    // SSRC completes the picture, which reaches the host whole.
    let (packets, skipped) = send(&mut bob, &pictures[0], Some(1), 6_000);
    let (lost, rtx) = skipped.unwrap();
    for packet in &packets {
        server.udp.send_to(packet, client).await.unwrap();
    }
    assert_eq!(next_nack(&server).await, (11, 23, vec![lost]));
    assert!(frames.incoming.try_recv().is_err(), "held for the piece");
    server.udp.send_to(&rtx, client).await.unwrap();
    let unit = timeout(WAIT, frames.incoming.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(unit.keyframe);
    assert_eq!(&unit.data, &pictures[0], "repaired");

    // The stream's sound reaches the call's mix, marked as the stream's.
    let mut sound = None;
    let mut audio = Transport::new(&Zeroizing::new(KEY));
    for sequence in 0..50u16 {
        let frame = bob.encrypt(b"synthetic stream sound").unwrap();
        let packet = audio
            .seal(
                &rtp::header(sequence, u32::from(sequence) * 960, 22),
                &frame,
            )
            .unwrap();
        server.udp.send_to(&packet, client).await.unwrap();
        if let Ok(Some(packet)) = timeout(Duration::from_millis(100), link.incoming.recv()).await {
            sound = Some(packet);
            break;
        }
    }
    let sound = sound.expect("the stream's sound");
    assert_eq!((sound.user, sound.stream), (UserId(BOB), true));
    assert_eq!(sound.opus, b"synthetic stream sound");

    // The host's decoder asks for a key frame: a sealed PLI to his screen.
    keys.try_send(BOB).unwrap();
    assert_eq!(next_pli(&server).await, (11, 23));
    // A piece lost: nothing reaches the host, and another PLI goes out.
    drain(&mut peer, &mut acks, Duration::from_millis(550)).await;
    // Its retransmission never comes: given up, the picture is lost.
    for packet in send(&mut bob, &pictures[0], Some(1), 9_000).0 {
        server.udp.send_to(&packet, client).await.unwrap();
    }
    assert_eq!(next_pli(&server).await, (11, 23));
    assert!(frames.incoming.try_recv().is_err());
    // A stream never takes the microphone: the call can still have it.
    assert!(port.outgoing.try_lock().is_ok());
    stream.abort();
}

#[tokio::test]
async fn watching_asks_the_gateway_connects_to_the_stream_s_server_and_ends_with_its_reason() {
    use rune_model::StreamPhase;
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let (video, _frames) = rune_provider::video::video_link();
    let mut calls = Calls::new(Some(port));
    calls.video = Some(video);
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    // Someone streaming in the channel.
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &json!({
            "guild_id": GUILD.to_string(), "channel_id": CHANNEL.to_string(),
            "user_id": BOB.to_string(), "self_stream": true,
        }),
    );
    assert!(
        calls
            .watch(&snapshot, ChannelId(CHANNEL), UserId(BOB))
            .is_err(),
        "not in the call"
    );
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    assert!(
        calls
            .watch(&snapshot, ChannelId(CHANNEL), UserId(77))
            .is_err(),
        "77 is not streaming"
    );
    let events = calls
        .watch(&snapshot, ChannelId(CHANNEL), UserId(BOB))
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase {
            phase: StreamPhase::Connecting,
            ..
        }]
    ));
    // Asked for once the call's voice session is known.
    assert!(wanted.borrow_and_update().unwrap().stream_keys().is_empty());
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("SYNTHETIC_VOICE_SESSION", CHANNEL),
    );
    let want = wanted.borrow_and_update().unwrap();
    assert_eq!(want.stream_keys(), [("guild:40:30:2".to_owned(), 0)]);
    // Another stream's dispatches are not this one's.
    let created = |key: &str| json!({"stream_key": key, "rtc_server_id": "901", "paused": false});
    calls.dispatch(&mut snapshot, "STREAM_CREATE", &created("guild:40:30:3"));
    // Without its server yet it waits, as the official client does.
    let events = calls.dispatch(
        &mut snapshot,
        "STREAM_CREATE",
        &json!({"stream_key": "guild:40:30:2", "rtc_server_id": null, "paused": false}),
    );
    assert!(events.is_some_and(|events| events.is_empty()));
    calls.dispatch(&mut snapshot, "STREAM_CREATE", &created("guild:40:30:2"));
    // A port other than 443 and a token past 512 bytes are Discord's too.
    let token = format!("SYNTHETIC_STREAM_TOKEN_{}", "s".repeat(600));
    let events = calls.dispatch(
        &mut snapshot,
        "STREAM_SERVER_UPDATE",
        &json!({
            "stream_key": "guild:40:30:2", "token": token,
            "endpoint": "c-stream.discord.media:2087",
        }),
    );
    assert!(events.is_some_and(|events| events.is_empty()), "accepted");
    let handshake = tokio::spawn(async move {
        let mut server = server;
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (server, peer, identify)
    });
    while !handshake.is_finished() {
        let _ = timeout(Duration::from_millis(20), calls.next()).await;
    }
    let (_server, _peer, identify) = handshake.await.unwrap();
    // The stream's server, the call's voice session, the stream's token.
    assert_eq!(identify["server_id"], "901");
    assert_eq!(identify["session_id"], "SYNTHETIC_VOICE_SESSION");
    assert_eq!(identify["token"], token.as_str());
    assert_eq!(identify["video"], true);
    // Its server goes away for now: reconnecting, and asked for again.
    let events = calls
        .dispatch(
            &mut snapshot,
            "STREAM_DELETE",
            &json!({"stream_key": "guild:40:30:2", "unavailable": true, "reason": null}),
        )
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase {
            phase: StreamPhase::Reconnecting,
            ..
        }]
    ));
    assert_eq!(
        wanted.borrow_and_update().unwrap().stream_keys(),
        [("guild:40:30:2".to_owned(), 1)]
    );
    // Then it ends, with why.
    let events = calls
        .dispatch(
            &mut snapshot,
            "STREAM_DELETE",
            &json!({"stream_key": "guild:40:30:2", "unavailable": false, "reason": "stream_full"}),
        )
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase { phase: StreamPhase::Ended(Some(reason)), .. }] if reason.contains("llena")
    ));
    assert!(wanted.borrow_and_update().unwrap().stream_keys().is_empty());
    // Its owner stopping it ends it without a reason; leaving lets go of it.
    calls
        .watch(&snapshot, ChannelId(CHANNEL), UserId(BOB))
        .unwrap();
    let events = calls
        .dispatch(
            &mut snapshot,
            "STREAM_DELETE",
            &json!({"stream_key": "guild:40:30:2", "reason": "user_requested"}),
        )
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase {
            phase: StreamPhase::Ended(None),
            ..
        }]
    ));
    calls
        .watch(&snapshot, ChannelId(CHANNEL), UserId(BOB))
        .unwrap();
    calls.leave();
    let left = wanted.borrow_and_update().unwrap();
    assert!(left.channel.is_none() && left.stream_keys().is_empty());
}

#[tokio::test]
async fn several_streams_are_watched_at_once_up_to_their_bound_each_on_its_own() {
    use rune_model::StreamPhase;
    let (port, _link) = voice_link();
    let (video, _frames) = rune_provider::video::video_link();
    let mut calls = Calls::new(Some(port));
    calls.video = Some(video);
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("SYNTHETIC_VOICE_SESSION", CHANNEL),
    );
    let streamers: Vec<u64> = (80..81 + MAX_STREAMS as u64).collect();
    for user in &streamers {
        let streaming = json!({"guild_id": GUILD.to_string(), "channel_id": CHANNEL.to_string(),
            "user_id": user.to_string(), "self_stream": true});
        calls.dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &streaming);
        calls
            .watch(&snapshot, ChannelId(CHANNEL), UserId(*user))
            .unwrap();
    }
    let keys = |users: &[u64]| -> Vec<(String, u32)> {
        users
            .iter()
            .map(|user| (format!("guild:40:30:{user}"), 0))
            .collect()
    };
    // One more than the bound: the one watched longest went.
    assert_eq!(
        wanted.borrow_and_update().unwrap().stream_keys(),
        keys(&streamers[1..])
    );
    // Each one's dispatches are its own: one ends, the others stay.
    let events = calls
        .dispatch(
            &mut snapshot,
            "STREAM_DELETE",
            &json!({"stream_key": format!("guild:40:30:{}", streamers[2]), "reason": "stream_full"}),
        )
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase { owner, phase: StreamPhase::Ended(Some(_)), .. }]
            if owner.0 == streamers[2]
    ));
    calls.stop_watching(ChannelId(CHANNEL), UserId(streamers[1]));
    let left: Vec<u64> = streamers[3..].to_vec();
    assert_eq!(
        wanted.borrow_and_update().unwrap().stream_keys(),
        keys(&left)
    );
}

#[tokio::test]
async fn the_call_brings_the_cameras_on_screen_only_and_asks_for_them_by_ssrc() {
    use rune_model::VideoKind;
    use rune_provider::video::video_link;
    let mut server = Server::new().await;
    let delivery = Proposer::new();
    let (port, _link) = voice_link();
    let (video, mut frames) = video_link();
    let (_keys, asked) = tokio::sync::mpsc::channel(8);
    let (cameras, wanted) = watch::channel(Vec::new());
    let video = Video {
        kind: VideoKind::Camera,
        incoming: video.incoming,
        keys: asked,
        wanted: Some(wanted),
        send: None,
    };
    let (_controls, controls) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    let call = tokio::spawn(connection::run(
        credentials(server.url()),
        port.clone(),
        Some(video),
        controls,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    // The call itself says it handles video and offers H264 to decode.
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(identify["server_id"], GUILD.to_string());
    assert_eq!(
        (&identify["video"], &identify["streams"]),
        (&json!(true), &json!([]))
    );
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (_, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(select["codecs"][1]["payload_type"], 101);
    assert_eq!(select["codecs"][1]["decode"], true);
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1}),
        )
        .await;
    expect(&mut peer, &mut acks, 5).await;
    // No camera known yet: no op 15, the key package comes next.
    next(&mut peer, &mut acks).await.expect_err("key package");
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    bob.connected(&[ALICE]).unwrap();
    bob.external_sender(&delivery.announcement()).unwrap();
    server
        .text(&mut peer, 11, json!({"user_ids": [BOB.to_string()]}))
        .await;
    server
        .binary(&mut peer, op::EXTERNAL_SENDER, &delivery.announcement())
        .await;
    let mut mirror = Dave::new(ALICE, CHANNEL).unwrap();
    mirror.external_sender(&delivery.announcement()).unwrap();
    let proposals = delivery.adding(&mirror, &bob.key_package().unwrap());
    server.binary(&mut peer, op::PROPOSALS, &proposals).await;
    let answer = next(&mut peer, &mut acks).await.expect_err("commit");
    let (commit, welcome) = split(&answer);
    server
        .binary(&mut peer, op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .await;
    bob.group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap();
    phase(&mut phases, CallPhase::Connected).await;
    // The call, unlike a stream, has the microphone.
    assert!(port.outgoing.try_lock().is_err());

    // Bob's camera in two layers (simulcast), 23 and the lighter 25: known,
    // but not on screen.
    server
        .text(
            &mut peer,
            12,
            json!({"user_id": BOB.to_string(), "audio_ssrc": 22, "video_ssrc": 23,
                "streams": [
                    {"type": "video", "rid": "100", "ssrc": 23, "rtx_ssrc": 24, "active": true, "quality": 100},
                    {"type": "video", "rid": "50", "ssrc": 25, "rtx_ssrc": 26, "active": true, "quality": 50},
                ]}),
        )
        .await;
    assert_eq!(
        expect(&mut peer, &mut acks, 15).await,
        json!({"23": 0, "25": 0})
    );
    let picture = encoded(1).remove(0);
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    let mut sequence = 100u16;
    let mut send = |bob: &mut Dave, timestamp: u32, ssrc: u32| {
        let sealed = bob
            .session
            .encrypt(davey::MediaType::VIDEO, davey::Codec::H264, &picture)
            .unwrap()
            .into_owned();
        let payloads = packetize(&sealed, 300);
        let last = payloads.len() - 1;
        let mut packets = Vec::new();
        for (index, payload) in payloads.iter().enumerate() {
            let mut header = rtp::header(sequence, timestamp, ssrc);
            header[1] = rtp::H264 | if index == last { 0x80 } else { 0 };
            sequence = sequence.wrapping_add(1);
            packets.push(sealer.seal(&header, payload).unwrap());
        }
        packets
    };
    for packet in send(&mut bob, 0, 23) {
        server.udp.send_to(&packet, client).await.unwrap();
    }
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;
    assert!(frames.incoming.try_recv().is_err(), "off screen, not shown");

    // On screen: its best layer asked for by its SSRC, the other not, again
    // a second later while it shows nothing; the other layer is not even
    // assembled, and the picture reaches the host as a camera.
    cameras.send_replace(vec![BOB]);
    let wants = json!({"23": 100, "25": 0});
    assert_eq!(expect(&mut peer, &mut acks, 15).await, wants);
    let again = std::time::Instant::now();
    assert_eq!(expect(&mut peer, &mut acks, 15).await, wants);
    assert!(again.elapsed() >= Duration::from_millis(900));
    for packet in send(&mut bob, 1_500, 25) {
        server.udp.send_to(&packet, client).await.unwrap();
    }
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;
    assert!(
        frames.incoming.try_recv().is_err(),
        "the layer not asked for"
    );
    for packet in send(&mut bob, 3_000, 23) {
        server.udp.send_to(&packet, client).await.unwrap();
    }
    let unit = timeout(WAIT, frames.incoming.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((unit.user, unit.kind), (UserId(BOB), VideoKind::Camera));
    assert!(unit.keyframe);
    assert_eq!(unit.data, picture, "decrypted as encoded");
    let mut decoder = openh264::decoder::Decoder::new().unwrap();
    let decoded = decoder.decode(&unit.data).unwrap().unwrap();
    use openh264::formats::YUVSource;
    assert_eq!(decoded.dimensions(), (320, 180));

    // His camera goes off: nothing of his is asked for any more.
    server
        .text(
            &mut peer,
            12,
            json!({"user_id": BOB.to_string(), "audio_ssrc": 22, "video_ssrc": 0, "streams": []}),
        )
        .await;
    drain(&mut peer, &mut acks, Duration::from_millis(100)).await;
    for packet in send(&mut bob, 6_000, 23) {
        server.udp.send_to(&packet, client).await.unwrap();
    }
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;
    assert!(frames.incoming.try_recv().is_err());
    call.abort();
}

#[tokio::test]
async fn the_host_s_requests_reach_the_connection_they_are_about_and_cameras_are_capped() {
    use rune_model::VideoKind;
    use rune_provider::video::{video_link, VideoRequest, MAX_CAMERAS};
    let (port, _link) = voice_link();
    let (video, link) = video_link();
    let mut calls = Calls::new(Some(port));
    calls.video = Some(video);
    let _wanted = calls.gateway();
    let snapshot = snapshot();
    let wants = |user, kind, on| VideoRequest::Wants {
        user: UserId(user),
        kind,
        on,
    };
    // Asked before the call: not part of it.
    link.requests
        .try_send(wants(9, VideoKind::Camera, true))
        .unwrap();
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    for user in 3..10 {
        link.requests
            .try_send(wants(user, VideoKind::Camera, true))
            .unwrap();
    }
    link.requests
        .try_send(wants(3, VideoKind::Camera, false))
        .unwrap();
    // The screen watched is chosen otherwise.
    link.requests
        .try_send(wants(20, VideoKind::Screen, true))
        .unwrap();
    let (keys, mut asked) = tokio::sync::mpsc::channel(8);
    calls.call.as_mut().unwrap().keys = Some(keys);
    link.requests
        .try_send(VideoRequest::KeyFrame {
            user: UserId(4),
            kind: VideoKind::Camera,
        })
        .unwrap();
    // The stream's key frames have nowhere to go without a stream.
    link.requests
        .try_send(VideoRequest::KeyFrame {
            user: UserId(5),
            kind: VideoKind::Screen,
        })
        .unwrap();
    for _ in 0..11 {
        let _ = timeout(Duration::from_millis(50), calls.next()).await;
    }
    let cameras = calls.call.as_ref().unwrap().cameras.borrow().clone();
    assert_eq!(cameras, vec![4, 5, 6], "at most {MAX_CAMERAS}, 3 let go");
    assert_eq!(asked.try_recv(), Ok(4));
    assert!(asked.try_recv().is_err());
}

/// The account's screen as another encoder may send it: the SPS of
/// OpenH264's first picture without its VUI, so nothing says it never
/// reorders.
fn without_vui(picture: &[u8]) -> Vec<u8> {
    super::h264::nals(picture)
        .into_iter()
        .flat_map(|nal| {
            let nal = if nal[0] & 0x1f == 7 {
                super::sps::without_vui(nal)
            } else {
                nal.to_vec()
            };
            [vec![0, 0, 0, 1], nal]
        })
        .flatten()
        .collect()
}

#[tokio::test]
async fn sharing_sends_the_screen_end_to_end_encrypted_and_answers_the_receivers() {
    use openh264::formats::YUVSource;
    use rune_provider::video::{video_link, Feedback, VideoUnit};
    use std::time::Instant;
    let mut server = Server::new().await;
    let delivery = Proposer::new();
    let (port, _link) = voice_link();
    let (video, mut link) = video_link();
    let (_keys, asked) = tokio::sync::mpsc::channel(8);
    let quality = rune_model::StreamQuality {
        height: 1080,
        fps: 60,
    };
    let units = video.outgoing.clone();
    let sharing = Video {
        kind: rune_model::VideoKind::Screen,
        incoming: video.incoming.clone(),
        keys: asked,
        wanted: None,
        send: Some(super::connection::Outgoing {
            quality,
            units: video.outgoing.clone(),
            feedback: video.feedback.clone(),
            sound: Some(video.sound.clone()),
        }),
    };
    let (controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    let stream = tokio::spawn(connection::run(
        Credentials {
            server: 900,
            ..credentials(server.url())
        },
        port.clone(),
        Some(sharing),
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(identify["video"], true);
    assert_eq!(
        identify["streams"],
        json!([{"type": "screen", "rid": "100", "quality": 100}])
    );
    // The screen's SSRC comes with Ready; its RTX is the next one.
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE],
                "streams": [{"type": "screen", "rid": "100", "ssrc": 33, "active": false}]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (_, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(select["codecs"][1]["encode"], true);
    assert_eq!(select["codecs"][1]["decode"], false);
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1,
                "video_codec": "H264"}),
        )
        .await;
    expect(&mut peer, &mut acks, 5).await;
    next(&mut peer, &mut acks).await.expect_err("key package");
    // What the screen is, for the receivers; no op 15 from the one sharing.
    assert_eq!(
        expect(&mut peer, &mut acks, 12).await,
        json!({"audio_ssrc": 11, "video_ssrc": 33, "rtx_ssrc": 34, "streams": [{
            "type": "screen", "rid": "100", "ssrc": 33, "rtx_ssrc": 34, "active": true,
            "quality": 100, "max_bitrate": 9_000_000, "max_framerate": 60,
            "max_resolution": {"type": "fixed", "width": 1920, "height": 1080},
        }]})
    );
    // Bob, who watches, joins the stream's group.
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    bob.connected(&[ALICE]).unwrap();
    bob.external_sender(&delivery.announcement()).unwrap();
    server
        .text(&mut peer, 11, json!({"user_ids": [BOB.to_string()]}))
        .await;
    server
        .binary(&mut peer, op::EXTERNAL_SENDER, &delivery.announcement())
        .await;
    let mut mirror = Dave::new(ALICE, CHANNEL).unwrap();
    mirror.external_sender(&delivery.announcement()).unwrap();
    let proposals = delivery.adding(&mirror, &bob.key_package().unwrap());
    server.binary(&mut peer, op::PROPOSALS, &proposals).await;
    let answer = next(&mut peer, &mut acks).await.expect_err("commit");
    let (commit, welcome) = split(&answer);
    server
        .binary(&mut peer, op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .await;
    bob.group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap();
    // Waiting while alone; connected once the group with Bob takes its commit.
    phase(&mut phases, CallPhase::Connected).await;
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;

    // A delta first: nothing goes out, and the host is asked for a key frame.
    let pictures = encoded(2);
    let unit = |keyframe, data: &[u8]| VideoUnit {
        user: UserId(ALICE),
        kind: rune_model::VideoKind::Screen,
        keyframe,
        at: Instant::now(),
        data: data.to_vec(),
    };
    units_send(&link.outgoing, unit(false, &pictures[1])).await;
    assert_eq!(
        timeout(WAIT, link.feedback.recv()).await.unwrap(),
        Some(Feedback::KeyFrame)
    );
    // The key frame, its SPS without a VUI, then a delta: RTP within 1200
    // bytes, sealed for the transport, encrypted for the group.
    let bare = without_vui(&pictures[0]);
    let first = unit(true, &bare);
    let then = VideoUnit {
        at: first.at + Duration::from_millis(33),
        ..unit(false, &pictures[1])
    };
    units_send(&link.outgoing, first).await;
    units_send(&link.outgoing, then).await;
    let opener = Transport::new(&Zeroizing::new(KEY));
    let mut depacketizer = super::h264::Depacketizer::default();
    let mut received = Vec::new();
    let mut sent = Vec::new();
    let mut last: Option<(u16, u32)> = None;
    while received.len() < 2 {
        let (length, _) = timeout(WAIT, server.udp.recv_from(&mut datagram))
            .await
            .unwrap()
            .unwrap();
        let Some(packet) = opener.open(&datagram[..length]) else {
            continue;
        };
        assert!(length <= 1200, "{length}");
        assert_eq!((packet.payload_type, packet.ssrc), (rtp::H264, 33));
        if let Some((sequence, _)) = last {
            assert_eq!(packet.sequence, sequence.wrapping_add(1), "in order");
        }
        last = Some((packet.sequence, packet.timestamp));
        sent.push((packet.sequence, packet.payload.clone()));
        let assembled = depacketizer
            .push(
                packet.sequence,
                packet.timestamp,
                packet.marker,
                &packet.payload,
                Instant::now(),
            )
            .unwrap();
        if let Some(assembled) = assembled {
            let plain = bob
                .session
                .decrypt(ALICE, davey::MediaType::VIDEO, &assembled.data)
                .unwrap();
            received.push((assembled.keyframe, packet.timestamp, plain));
        }
    }
    assert!(received[0].0 && !received[1].0);
    let ticks = received[1].1.wrapping_sub(received[0].1);
    assert!(
        (2_969..=2_971).contains(&ticks),
        "33 ms on a 90 kHz clock: {ticks}"
    );
    // The SPS reached Bob saying it never reorders, and the picture decodes.
    let sps = super::h264::nals(&bare)
        .into_iter()
        .find(|nal| nal[0] & 0x1f == 7)
        .unwrap()
        .to_vec();
    let fixed = super::sps::normalize(&sps).expect("rewritten before encrypting");
    let sent_sps = super::h264::nals(&received[0].2)
        .into_iter()
        .find(|nal| nal[0] & 0x1f == 7)
        .unwrap()
        .to_vec();
    assert_eq!(sent_sps, fixed);
    assert_eq!(&received[1].2, &pictures[1]);
    let mut decoder = openh264::decoder::Decoder::new().unwrap();
    let picture = decoder.decode(&received[0].2).unwrap().unwrap();
    assert_eq!(picture.dimensions(), (320, 180));

    // A NACK is answered on the RTX SSRC: the original sequence number,
    // then the same payload, sealed anew.
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    let rtcp =
        |sealer: &mut Transport, packet: &[u8]| sealer.seal(&packet[..8], &packet[8..]).unwrap();
    let (lost, payload) = sent[1].clone();
    let nack = rtp::nack(99, 33, &[lost]);
    server
        .udp
        .send_to(&rtcp(&mut sealer, &nack), client)
        .await
        .unwrap();
    let repaired = timeout(WAIT, async {
        loop {
            let (length, _) = server.udp.recv_from(&mut datagram).await.unwrap();
            if let Some(packet) = opener.open(&datagram[..length]) {
                if packet.payload_type == rtp::RTX {
                    return packet;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(repaired.ssrc, 34);
    assert_eq!(&repaired.payload[..2], &lost.to_be_bytes());
    assert_eq!(&repaired.payload[2..], &payload[..]);

    // The screen's sound: op 5 says it is a screen's (SOUNDSHARE), and its
    // frames go on the audio SSRC, end-to-end encrypted.
    link.sound
        .send(b"synthetic screen sound".to_vec())
        .await
        .unwrap();
    assert_eq!(
        expect(&mut peer, &mut acks, 5).await,
        json!({"speaking": 2, "delay": 0, "ssrc": 11})
    );
    let sound = timeout(WAIT, async {
        loop {
            let (length, _) = server.udp.recv_from(&mut datagram).await.unwrap();
            if let Some(packet) = opener.open(&datagram[..length]) {
                if packet.payload_type == rtp::OPUS {
                    // Marked as the screen's sound, as Serein does (voz-protocolo).
                    assert_eq!(
                        (datagram[0] & 0x10, &datagram[12..16]),
                        (0x10, &[0xbe, 0xde, 0, 1][..])
                    );
                    return packet;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(sound.ssrc, 11);
    assert_eq!(
        bob.decrypt(ALICE, &sound.payload).unwrap(),
        b"synthetic screen sound"
    );

    // A receiver's PLI asks the host for a key frame, once the gap allows.
    drain(&mut peer, &mut acks, Duration::from_millis(550)).await;
    let pli = rtp::pli(99, 33);
    server
        .udp
        .send_to(&rtcp(&mut sealer, &pli), client)
        .await
        .unwrap();
    assert_eq!(
        timeout(WAIT, link.feedback.recv()).await.unwrap(),
        Some(Feedback::KeyFrame)
    );
    // Half the packets lost: from the most it started at, the bit rate goes
    // down by a quarter at the next step.
    let mut report = vec![0x81, 201, 0, 7, 0, 0, 0, 99];
    report.extend(33u32.to_be_bytes());
    report.push(128);
    report.extend([0; 19]);
    server
        .udp
        .send_to(&rtcp(&mut sealer, &report), client)
        .await
        .unwrap();
    assert_eq!(
        timeout(WAIT, link.feedback.recv()).await.unwrap(),
        Some(Feedback::Bitrate(quality.max_bitrate() / 4 * 3))
    );
    // A NACK for what is no longer kept (past a second) asks for a key frame.
    drain(&mut peer, &mut acks, Duration::from_millis(550)).await;
    server
        .udp
        .send_to(&rtcp(&mut sealer, &nack), client)
        .await
        .unwrap();
    assert_eq!(
        timeout(WAIT, link.feedback.recv()).await.unwrap(),
        Some(Feedback::KeyFrame)
    );
    // Paused, nothing is taken from the host, whose encoder then skips
    // frames; resumed, what follows starts at a key frame.
    controls.send_modify(|controls| controls.paused = true);
    drain(&mut peer, &mut acks, Duration::from_millis(550)).await;
    for _ in 0..rune_provider::video::OUTGOING {
        units_send(&link.outgoing, unit(false, &pictures[1])).await;
    }
    drain(&mut peer, &mut acks, Duration::from_millis(100)).await;
    assert_eq!(link.outgoing.capacity(), 0, "nothing taken while paused");
    controls.send_modify(|controls| controls.paused = false);
    assert_eq!(
        timeout(WAIT, link.feedback.recv()).await.unwrap(),
        Some(Feedback::KeyFrame)
    );
    // Stopping lets go of the host's units.
    assert!(units.try_lock().is_err(), "held while sharing");
    stream.abort();
    let _ = stream.await;
    assert!(units.try_lock().is_ok());
}

/// Hand a unit to the connection as the host does.
async fn units_send(
    outgoing: &tokio::sync::mpsc::Sender<rune_provider::video::VideoUnit>,
    unit: rune_provider::video::VideoUnit,
) {
    timeout(WAIT, outgoing.send(unit)).await.unwrap().unwrap();
}

#[tokio::test]
async fn sharing_asks_the_gateway_connects_as_the_sender_and_lets_go_on_each_stop() {
    use rune_model::{StreamPhase, StreamQuality};
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let (video, _frames) = rune_provider::video::video_link();
    let units = video.outgoing.clone();
    let mut calls = Calls::new(Some(port));
    calls.video = Some(video);
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    let quality = StreamQuality::default();
    assert!(
        calls.share(ChannelId(CHANNEL), quality).is_err(),
        "not in the call"
    );
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let odd = StreamQuality {
        height: 1440,
        fps: 30,
    };
    assert!(calls.share(ChannelId(CHANNEL), odd).is_err(), "not offered");
    let events = calls.share(ChannelId(CHANNEL), quality).unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase { owner, phase: StreamPhase::Connecting, .. }] if owner.0 == ALICE
    ));
    // Created (op 18) once the call's voice session is known.
    assert_eq!(wanted.borrow_and_update().unwrap().share, None);
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("SYNTHETIC_VOICE_SESSION", CHANNEL),
    );
    let want = wanted.borrow_and_update().unwrap();
    assert_eq!(want.share_key().as_deref(), Some("guild:40:30:1"));
    assert!(want.stream_keys().is_empty(), "watching nothing");
    calls.dispatch(
        &mut snapshot,
        "STREAM_CREATE",
        &json!({"stream_key": "guild:40:30:1", "rtc_server_id": "901", "rtc_channel_id": "902"}),
    );
    calls.dispatch(
        &mut snapshot,
        "STREAM_SERVER_UPDATE",
        &json!({
            "stream_key": "guild:40:30:1", "token": "SYNTHETIC_STREAM_TOKEN",
            "endpoint": "c-stream.discord.media:443",
        }),
    );
    let handshake = tokio::spawn(async move {
        let mut server = server;
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (server, peer, identify)
    });
    while !handshake.is_finished() {
        let _ = timeout(Duration::from_millis(20), calls.next()).await;
    }
    let (_server, _peer, identify) = handshake.await.unwrap();
    assert_eq!(identify["server_id"], "901");
    assert_eq!(identify["session_id"], "SYNTHETIC_VOICE_SESSION");
    assert_eq!(identify["token"], "SYNTHETIC_STREAM_TOKEN");
    assert_eq!(identify["streams"][0]["type"], "screen");
    // The connection that sends holds the host's units.
    assert!(units.try_lock().is_err());
    // Paused: the service is told (op 22 follows the wish) and the phase
    // says so at once; resumed, it goes on.
    let events = calls.pause(ChannelId(CHANNEL), true);
    assert!(matches!(
        &events[..],
        [Event::StreamPhase { owner, phase: StreamPhase::Paused, .. }] if owner.0 == ALICE
    ));
    assert!(wanted.borrow_and_update().unwrap().share_paused);
    assert!(calls.pause(ChannelId(CHANNEL), true).is_empty(), "already");
    calls.pause(ChannelId(CHANNEL), false);
    assert!(!wanted.borrow_and_update().unwrap().share_paused);
    // Discord ends it with why: it goes, the units with it.
    let events = calls
        .dispatch(
            &mut snapshot,
            "STREAM_DELETE",
            &json!({"stream_key": "guild:40:30:1", "unavailable": false, "reason": "unauthorized"}),
        )
        .unwrap();
    assert!(matches!(
        &events[..],
        [Event::StreamPhase { owner, phase: StreamPhase::Ended(Some(reason)), .. }]
            if owner.0 == ALICE && reason.contains("transmitir")
    ));
    assert_eq!(wanted.borrow_and_update().unwrap().share, None);
    assert!(units.try_lock().is_ok());
    // «Dejar de compartir» tells the service (op 19 follows the wish).
    calls.share(ChannelId(CHANNEL), quality).unwrap();
    assert_eq!(wanted.borrow_and_update().unwrap().share, Some(ALICE));
    calls.stop_share(ChannelId(CHANNEL));
    assert_eq!(wanted.borrow_and_update().unwrap().share, None);
    // Leaving the call stops it too.
    calls.share(ChannelId(CHANNEL), quality).unwrap();
    calls.leave();
    let left = wanted.borrow_and_update().unwrap();
    assert_eq!((left.channel, left.share), (None, None));
    // A new account's session forgets it: nothing is shared without a call.
    calls.end();
    assert!(calls.share(ChannelId(CHANNEL), quality).is_err());
}

#[tokio::test]
async fn the_call_sends_the_camera_end_to_end_encrypted_while_it_is_on() {
    use rune_model::VideoKind;
    use rune_provider::video::{video_link, Feedback, VideoUnit, CAMERA};
    use std::time::Instant;
    let mut server = Server::new().await;
    let delivery = Proposer::new();
    let (port, _link) = voice_link();
    let (video, mut link) = video_link();
    let (_keys, asked) = tokio::sync::mpsc::channel(8);
    let (_cameras, wanted) = watch::channel(Vec::new());
    let units = video.camera_outgoing.clone();
    let call = Video {
        kind: VideoKind::Camera,
        incoming: video.incoming.clone(),
        keys: asked,
        wanted: Some(wanted),
        send: Some(super::connection::Outgoing {
            quality: CAMERA,
            units: video.camera_outgoing.clone(),
            feedback: video.camera_feedback.clone(),
            sound: None,
        }),
    };
    let (controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    let task = tokio::spawn(connection::run(
        credentials(server.url()),
        port.clone(),
        Some(call),
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    // The call always announces one camera, on or not, as the official client.
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(
        (&identify["video"], &identify["streams"]),
        (
            &json!(true),
            &json!([{"type": "video", "rid": "100", "quality": 100}])
        )
    );
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE],
                "streams": [{"type": "video", "rid": "100", "ssrc": 43, "active": false}]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (_, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    // It encodes its camera and decodes the others'.
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(
        (
            &select["codecs"][1]["encode"],
            &select["codecs"][1]["decode"]
        ),
        (&json!(true), &json!(true))
    );
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1,
                "video_codec": "H264"}),
        )
        .await;
    expect(&mut peer, &mut acks, 5).await;
    next(&mut peer, &mut acks).await.expect_err("key package");
    let mut bob = Dave::new(BOB, CHANNEL).unwrap();
    bob.connected(&[ALICE]).unwrap();
    bob.external_sender(&delivery.announcement()).unwrap();
    server
        .text(&mut peer, 11, json!({"user_ids": [BOB.to_string()]}))
        .await;
    server
        .binary(&mut peer, op::EXTERNAL_SENDER, &delivery.announcement())
        .await;
    let mut mirror = Dave::new(ALICE, CHANNEL).unwrap();
    mirror.external_sender(&delivery.announcement()).unwrap();
    let proposals = delivery.adding(&mirror, &bob.key_package().unwrap());
    server.binary(&mut peer, op::PROPOSALS, &proposals).await;
    let answer = next(&mut peer, &mut acks).await.expect_err("commit");
    let (commit, welcome) = split(&answer);
    server
        .binary(&mut peer, op::ANNOUNCE_COMMIT, &with_transition(0, &commit))
        .await;
    bob.group_changed(op::WELCOME, &with_transition(0, &welcome))
        .unwrap();
    phase(&mut phases, CallPhase::Connected).await;
    let pictures = encoded(1);
    let unit = |data: &[u8]| VideoUnit {
        user: UserId(ALICE),
        kind: VideoKind::Camera,
        keyframe: true,
        at: Instant::now(),
        data: data.to_vec(),
    };
    // Off: no op 12, and what the host sends goes nowhere.
    units_send(&link.camera_outgoing, unit(&pictures[0])).await;
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;
    let opener = Transport::new(&Zeroizing::new(KEY));
    let camera_packets = |server: &Server| {
        let (mut found, mut datagram) = (0, [0u8; 4096]);
        while let Ok((length, _)) = server.udp.try_recv_from(&mut datagram) {
            if opener
                .open(&datagram[..length])
                .is_some_and(|packet| packet.ssrc == 43)
            {
                found += 1;
            }
        }
        found
    };
    assert_eq!(camera_packets(&server), 0);

    // On: op 12 describes it at the camera's preset, and its key frame
    // reaches Bob, who decrypts and decodes it.
    controls.send_replace(Controls {
        camera: true,
        ..Controls::default()
    });
    assert_eq!(
        expect(&mut peer, &mut acks, 12).await,
        json!({"audio_ssrc": 11, "video_ssrc": 43, "rtx_ssrc": 44, "streams": [{
            "type": "video", "rid": "100", "ssrc": 43, "rtx_ssrc": 44, "active": true,
            "quality": 100, "max_bitrate": 2_500_000, "max_framerate": 30,
            "max_resolution": {"type": "fixed", "width": 1280, "height": 720},
        }]})
    );
    units_send(&link.camera_outgoing, unit(&pictures[0])).await;
    let mut depacketizer = super::h264::Depacketizer::default();
    let assembled = timeout(WAIT, async {
        loop {
            let (length, _) = server.udp.recv_from(&mut datagram).await.unwrap();
            let Some(packet) = opener.open(&datagram[..length]) else {
                continue;
            };
            assert!(length <= 1200, "{length}");
            assert_eq!((packet.payload_type, packet.ssrc), (rtp::H264, 43));
            let pushed = depacketizer
                .push(
                    packet.sequence,
                    packet.timestamp,
                    packet.marker,
                    &packet.payload,
                    Instant::now(),
                )
                .unwrap();
            if let Some(assembled) = pushed {
                return assembled;
            }
        }
    })
    .await
    .unwrap();
    assert!(assembled.keyframe);
    let plain = bob
        .session
        .decrypt(ALICE, davey::MediaType::VIDEO, &assembled.data)
        .unwrap();
    let mut decoder = openh264::decoder::Decoder::new().unwrap();
    use openh264::formats::YUVSource;
    let decoded = decoder.decode(&plain).unwrap().unwrap();
    assert_eq!(decoded.dimensions(), (320, 180));
    // A receiver's PLI to the camera asks the camera's encoder for a key frame.
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    let pli = rtp::pli(99, 43);
    let sealed = sealer.seal(&pli[..8], &pli[8..]).unwrap();
    server.udp.send_to(&sealed, client).await.unwrap();
    assert_eq!(
        timeout(WAIT, link.camera_feedback.recv()).await.unwrap(),
        Some(Feedback::KeyFrame)
    );
    assert!(link.feedback.try_recv().is_err(), "not the screen's");

    // Off again: op 12 takes it back, and nothing more goes out.
    controls.send_replace(Controls::default());
    assert_eq!(
        expect(&mut peer, &mut acks, 12).await,
        json!({"audio_ssrc": 11, "video_ssrc": 0, "rtx_ssrc": 0, "streams": []})
    );
    while server.udp.try_recv_from(&mut datagram).is_ok() {}
    units_send(&link.camera_outgoing, unit(&pictures[0])).await;
    drain(&mut peer, &mut acks, Duration::from_millis(200)).await;
    assert_eq!(camera_packets(&server), 0);
    // The call ending lets go of the host's camera units.
    assert!(units.try_lock().is_err(), "held by the call");
    task.abort();
    let _ = task.await;
    assert!(units.try_lock().is_ok());
}

#[tokio::test]
async fn the_camera_is_asked_of_the_gateway_with_the_call_and_goes_with_it() {
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let (video, _frames) = rune_provider::video::video_link();
    let units = video.camera_outgoing.clone();
    let mut calls = Calls::new(Some(port));
    calls.video = Some(video);
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    let mut snapshot = snapshot();
    assert!(
        calls.set_camera(ChannelId(CHANNEL), true).is_err(),
        "not in the call"
    );
    assert!(calls.set_camera(ChannelId(CHANNEL), false).is_ok());
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    assert!(!wanted.borrow_and_update().unwrap().camera);
    assert!(
        calls.set_camera(ChannelId(STAGE), true).is_err(),
        "another channel"
    );
    // On: op 4 says `self_video`, and the call's connection is told.
    calls.set_camera(ChannelId(CHANNEL), true).unwrap();
    assert!(wanted.borrow_and_update().unwrap().camera);
    assert!(calls.call.as_ref().unwrap().controls.borrow().camera);
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("SYNTHETIC_VOICE_SESSION", CHANNEL),
    );
    calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &json!({
        "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
    }));
    let handshake = tokio::spawn(async move {
        let mut server = server;
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (server, peer, identify)
    });
    while !handshake.is_finished() {
        let _ = timeout(Duration::from_millis(20), calls.next()).await;
    }
    let (_server, _peer, identify) = handshake.await.unwrap();
    assert_eq!(identify["streams"][0]["type"], "video");
    // The call's connection holds the host's camera units.
    assert!(units.try_lock().is_err());
    // Still on after a new Gateway session asks for the call again.
    calls.ready(&json!({"guilds": [{"id": GUILD.to_string(), "voice_states": []}]}));
    assert!(wanted.borrow_and_update().unwrap().camera);
    calls.set_camera(ChannelId(CHANNEL), false).unwrap();
    assert!(!wanted.borrow_and_update().unwrap().camera);
    // Leaving takes it off, and the units go with the call.
    calls.set_camera(ChannelId(CHANNEL), true).unwrap();
    calls.leave();
    let left = wanted.borrow_and_update().unwrap();
    assert_eq!((left.channel, left.camera), (None, false));
    assert!(units.try_lock().is_ok());
    // A new account's session has no call to turn it on in.
    calls.end();
    assert!(calls.set_camera(ChannelId(CHANNEL), true).is_err());
}

#[tokio::test]
async fn diagnostics_follow_each_step_and_never_show_secrets() {
    const TOKEN: &str = "DIAG_TOKEN_9f2c7e";
    const SESSION: &str = "DIAG_SESSION_41d8b0";
    // What IP discovery says of this machine: never in a line.
    const OWN_ADDRESS: &str = "203.0.113.77";
    let mut server = Server::new().await;
    let (port, _link) = voice_link();
    let (controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    let call = tokio::spawn(connection::run(
        Credentials {
            session: Zeroizing::new(SESSION.into()),
            token: Zeroizing::new(TOKEN.into()),
            ..credentials(server.url())
        },
        port,
        None,
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let identify = expect(&mut peer, &mut acks, 0).await;
    assert_eq!(identify["token"], TOKEN);
    assert_eq!(identify["channel_id"], CHANNEL.to_string());
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            &mut peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE]}),
        )
        .await;
    // The first discovery request is lost: the client asks again.
    let mut datagram = [0u8; 4096];
    let mut client = None;
    for _ in 0..2 {
        let (length, from) = timeout(WAIT, server.udp.recv_from(&mut datagram))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&datagram[..length], &rtp::discovery_request(11));
        client = Some(from);
    }
    let client = client.unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..8 + OWN_ADDRESS.len()].copy_from_slice(OWN_ADDRESS.as_bytes());
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    let select = expect(&mut peer, &mut acks, 1).await;
    assert_eq!(select["data"]["address"], OWN_ADDRESS);
    // An opcode of a newer protocol is not a reason to drop the call.
    server.text(&mut peer, 99, json!({"unknown": true})).await;
    server
        .text(
            &mut peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1}),
        )
        .await;
    assert_eq!(expect(&mut peer, &mut acks, 5).await["speaking"], 0);
    next(&mut peer, &mut acks)
        .await
        .expect_err("binary key package");
    phase(&mut phases, CallPhase::Waiting).await;
    // The voice server restarts: resumed, and the SSRC said again.
    peer.close(Some(CloseFrame {
        code: CloseCode::from(4015),
        reason: "Voice server crashed".into(),
    }))
    .await
    .unwrap();
    phase(&mut phases, CallPhase::Reconnecting { attempt: 1 }).await;
    let mut peer = server.accept().await;
    server
        .text(&mut peer, 8, json!({"heartbeat_interval": QUIET}))
        .await;
    let resume = expect(&mut peer, &mut acks, 7).await;
    assert_eq!(resume["session_id"], SESSION);
    assert_eq!(resume["channel_id"], CHANNEL.to_string());
    server.text(&mut peer, 9, json!(null)).await;
    assert_eq!(
        expect(&mut peer, &mut acks, 5).await,
        json!({"speaking": 0, "delay": 0, "ssrc": 11})
    );
    phase(&mut phases, CallPhase::Waiting).await;
    drop(controls);
    assert_eq!(timeout(WAIT, call).await.unwrap().unwrap(), Ok(()));

    let lines = super::diag::captured::lines();
    for step in [
        "rune: voice call: opening the voice gateway (v=8) at 127.0.0.1:",
        "rune: voice call: HELLO (op 8): heartbeat every 60000 ms",
        "rune: voice call: IDENTIFY sent (op 0): max_dave_protocol_version 1",
        "rune: voice call: READY (op 2): modes offered [aead_xchacha20_poly1305_rtpsize]; RUNE uses",
        "rune: voice call: IP discovery request sent (try 2)",
        "rune: voice call: IP discovery reply received (try 2, IPv4)",
        "rune: voice call: SELECT_PROTOCOL sent (op 1)",
        "rune: voice call: unknown opcode 99: ignored",
        "rune: voice call: SESSION_DESCRIPTION (op 4): mode aead_xchacha20_poly1305_rtpsize, dave_protocol_version 1",
        "rune: voice call: SPEAKING (op 5, off) and DAVE op 26 (key package) sent",
        "rune: voice call: connected: media may flow (alone in the channel: waiting for the others)",
        "rune: voice call: voice gateway closed by Discord: code 4015 «Voice server crashed»",
        "rune: voice call: dropped (El servidor de voz se reinició.); resume 1/3 (op 7, seq_ack",
        "rune: voice call: RESUMED (op 9)",
        "rune: voice call: connection closed: the call was left",
    ] {
        assert!(
            lines.iter().any(|line| line.starts_with(step)),
            "no line starting {step:?}"
        );
    }
    for secret in [TOKEN, SESSION, OWN_ADDRESS] {
        assert!(
            !lines.iter().any(|line| line.contains(secret)),
            "{secret} in a line"
        );
    }
}

/// Poll `calls` until the local voice server reads an Identify: the peer,
/// and the session it named.
async fn identified(calls: &mut Calls, server: &Server) -> (Peer, String) {
    let accepted = async {
        let mut peer = server.accept().await;
        peer.send(Message::Text(
            json!({"op": 8, "d": {"heartbeat_interval": QUIET}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (peer, identify["session_id"].as_str().unwrap().to_owned())
    };
    tokio::pin!(accepted);
    loop {
        tokio::select! {
            done = &mut accepted => return done,
            events = calls.next() => assert!(
                !events.iter().any(|event| matches!(event, Event::CallEnded { .. })),
                "{events:?}"
            ),
        }
    }
}

#[tokio::test]
async fn a_newer_session_before_the_voice_server_accepts_ours_replaces_it_in_either_order() {
    const TOKEN: &str = "RACE_TOKEN_77aa";
    let server_update = json!({
        "guild_id": GUILD.to_string(), "token": TOKEN, "endpoint": "c-test.discord.media:443",
    });
    for server_first in [false, true] {
        let server = Server::new().await;
        let (port, _link) = voice_link();
        let mut calls = Calls::new(Some(port));
        calls.local = Some(server.url());
        let _wanted = calls.gateway();
        let mut snapshot = snapshot();
        calls
            .join(&snapshot, ChannelId(CHANNEL), false, false)
            .unwrap();
        // Without the Gateway's session to compare, the service first
        // describes another client of the account, then this request: the
        // voice server update comes before or after.
        if server_first {
            calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &server_update);
        }
        let events = calls
            .dispatch(
                &mut snapshot,
                "VOICE_STATE_UPDATE",
                &own("RACE_OTHER_CLIENT", CHANNEL),
            )
            .unwrap();
        assert!(!events
            .iter()
            .any(|event| matches!(event, Event::CallEnded { .. })));
        if !server_first {
            calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &server_update);
        }
        let (mut first, session) = identified(&mut calls, &server).await;
        assert_eq!(session, "RACE_OTHER_CLIENT");
        // Not accepted yet: the newest session is the candidate, and the
        // connection starts over with it.
        let events = calls
            .dispatch(
                &mut snapshot,
                "VOICE_STATE_UPDATE",
                &own("RACE_THIS_DEVICE", CHANNEL),
            )
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::CallEnded { .. })),
            "not another device: {events:?}"
        );
        let (_second, session) = identified(&mut calls, &server).await;
        assert_eq!(session, "RACE_THIS_DEVICE");
        let closed = timeout(WAIT, async {
            loop {
                match first.next().await {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                    _ => {}
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "the first connection went");
        calls.leave();
    }
    let lines = super::diag::captured::lines();
    assert!(lines
        .iter()
        .any(|line| line.contains("a newer candidate: connecting again with it")));
    for secret in [TOKEN, "RACE_OTHER_CLIENT", "RACE_THIS_DEVICE"] {
        assert!(
            !lines.iter().any(|line| line.contains(secret)),
            "{secret} in a line"
        );
    }
}

#[tokio::test]
async fn the_gateway_session_tells_this_device_s_voice_state_from_another_s_in_either_order() {
    let server_update = json!({
        "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
    });
    for server_first in [false, true] {
        let (port, _link) = voice_link();
        let mut calls = Calls::new(Some(port));
        // Never polled: no connection is made.
        calls.local = Some("ws://127.0.0.1:9/?v=8".into());
        let mut wanted = calls.gateway();
        calls.gateway_session(Zeroizing::new("GATEWAY_SESSION".into()));
        let mut snapshot = snapshot();
        calls
            .join(&snapshot, ChannelId(CHANNEL), false, false)
            .unwrap();
        if server_first {
            calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &server_update);
        }
        // The account's other client, still described: not this call's.
        let events = calls
            .dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &own("PHONE", CHANNEL))
            .unwrap();
        assert!(!events
            .iter()
            .any(|event| matches!(event, Event::CallEnded { .. })));
        assert!(calls.call.as_ref().unwrap().session.is_none());
        calls.dispatch(
            &mut snapshot,
            "VOICE_STATE_UPDATE",
            &own("GATEWAY_SESSION", CHANNEL),
        );
        if !server_first {
            calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &server_update);
        }
        let call = calls.call.as_ref().unwrap();
        assert_eq!(
            call.session.as_deref().map(|session| session.as_str()),
            Some("GATEWAY_SESSION")
        );
        assert!(call.connection.is_some(), "connecting with both known");
        // Then the phone takes the call: it ends here, and nothing is told.
        wanted.borrow_and_update();
        let events = calls
            .dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &own("PHONE", 99))
            .unwrap();
        assert!(
            matches!(
                &events[..],
                [Event::VoiceChanged(_), Event::CallEnded { reason: Some(reason), .. }]
                    if reason == super::TAKEN_OVER
            ),
            "{events:?}"
        );
        assert_eq!(*wanted.borrow_and_update(), None);
    }
}

#[tokio::test]
async fn stale_own_states_before_the_voice_server_accepts_the_session_are_ignored() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let _wanted = calls.gateway();
    let mut snapshot = snapshot();
    let gone =
        json!({"guild_id": GUILD.to_string(), "channel_id": null, "user_id": ALICE.to_string()});
    for this_first in [false, true] {
        calls
            .join(&snapshot, ChannelId(CHANNEL), false, false)
            .unwrap();
        // A previous attempt's leave, or another channel's state, arrives
        // late: before or after this request's own state.
        if this_first {
            calls.dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &own("THIS", CHANNEL));
        }
        for stale in [gone.clone(), own("THIS", 99)] {
            let events = calls
                .dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &stale)
                .unwrap();
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, Event::CallEnded { .. })),
                "{events:?}"
            );
        }
        if !this_first {
            calls.dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &own("THIS", CHANNEL));
        }
        assert_eq!(
            calls
                .call
                .as_ref()
                .unwrap()
                .session
                .as_deref()
                .map(|session| session.as_str()),
            Some("THIS")
        );
        // Accepted by the voice server, the same leave is real.
        calls.call.as_mut().unwrap().confirmed = true;
        let events = calls
            .dispatch(&mut snapshot, "VOICE_STATE_UPDATE", &gone)
            .unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            Event::CallEnded { reason: Some(reason), .. } if reason.contains("desconectaron")
        )));
    }
}

#[tokio::test]
async fn a_call_that_had_connected_rejoins_after_any_failure_but_being_removed() {
    tokio::time::pause();
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let _wanted = calls.gateway();
    let snapshot = snapshot();
    // The voice server ended the session (4006): asked for again.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let (phase, lose) = connect_ending(&mut calls, "La sesión de voz terminó. Volvé a entrar.");
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 1);
    calls.leave();
    // Removed by Discord (4014): never rejoined.
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    let (phase, lose) = connect_ending(&mut calls, connection::KICKED);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let _ = lose.send(());
    assert!(matches!(
        &calls.next().await[..],
        [Event::VoiceLeft { .. }, Event::CallEnded { reason: Some(reason), .. }] if reason == connection::KICKED
    ));
}

#[tokio::test]
async fn a_rejoin_identifies_again_with_the_same_session_and_server() {
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    calls.gateway_session(Zeroizing::new("GATEWAY_SESSION".into()));
    let mut snapshot = snapshot();
    calls
        .join(&snapshot, ChannelId(CHANNEL), false, false)
        .unwrap();
    calls.dispatch(
        &mut snapshot,
        "VOICE_STATE_UPDATE",
        &own("GATEWAY_SESSION", CHANNEL),
    );
    calls.dispatch(&mut snapshot, "VOICE_SERVER_UPDATE", &json!({
        "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
    }));
    let (_first, session) = identified(&mut calls, &server).await;
    assert_eq!(session, "GATEWAY_SESSION");
    // It connected, then the voice server let it go for good.
    let (phase, lose) = connect_ending(&mut calls, "La sesión de voz terminó. Volvé a entrar.");
    phase.send_replace(CallPhase::Connected);
    // The first connection's privacy code goes with it, before or after,
    // and so does what it measured (voz-protocolo).
    while !matches!(
        &calls.next().await[..],
        [Event::CallPhase {
            phase: CallPhase::Connected,
            ..
        }]
    ) {}
    let call = calls.call.as_mut().unwrap();
    call.privacy = None;
    call.quality = None;
    let before = wanted.borrow_and_update().unwrap();
    let _ = lose.send(());
    assert_eq!(reconnecting(&mut calls).await, 1);
    // No new state or server from the service: a fresh Identify with what
    // it had, and op 4 again besides.
    let (_again, session) = identified(&mut calls, &server).await;
    assert_eq!(session, "GATEWAY_SESSION");
    assert_ne!(wanted.borrow_and_update().unwrap().request, before.request);
    calls.leave();
}

// Private calls.
mod private;
// Voice protocol (voz-protocolo).
mod protocol;
