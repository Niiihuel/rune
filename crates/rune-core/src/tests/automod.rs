//! AutoMod (automod): a message a server's rule blocks fails with what the
//! server says, through the simulation, and is no longer waited on.

use super::*;

#[test]
fn a_message_automod_blocks_fails_with_the_servers_reason() {
    let _spanish = rune_i18n::scoped(rune_i18n::Language::Spanish);
    let mut client = client(Arc::new(mock()), None);
    // «Taller de Rust»'s #general, where the account is a plain member.
    let channel = text_channels(&ready(&mut client))[2];
    let nonce = client.send_message(channel, "Vengan a discord.gg/rustaceos".into(), None, true);
    let (failed, ambiguous, reason) = wait(&mut client, send_failed);
    assert_eq!(failed, nonce);
    assert!(!ambiguous, "the server said no: it did not go");
    assert_eq!(
        reason,
        "la moderación automática del servidor lo bloqueó: «No compartas invitaciones a otros servidores.»"
    );
    assert!(!client.sending.contains_key(&nonce), "nothing waits on it");
}
