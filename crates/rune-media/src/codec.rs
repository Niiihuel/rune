//! Opus at 48 kHz mono in 20 ms frames: what a call sends and receives.
//!
//! Mono halves the encoder's work, and a mono packet decodes the same on a
//! stereo receiver. Decoding always yields mono.

use crate::{MediaError, PLAYBACK_RATE};
use rune_i18n::{t, tr};

/// Samples in one 20 ms frame at [`PLAYBACK_RATE`].
pub const FRAME: usize = PLAYBACK_RATE as usize / 50;
/// The largest Opus packet for one frame (RFC 6716 §3.4).
pub const MAX_PACKET: usize = 1275;
/// The longest a packet can last: 120 ms.
pub const MAX_PACKET_SAMPLES: usize = 6 * FRAME;

fn failed(what: &str) -> MediaError {
    MediaError(tr!("El códec de voz falló ({what}).", what))
}

/// Opus encoder for the microphone: VoIP mode, 64 kbit/s, in-band FEC.
pub struct Encoder(opus::Encoder);

impl Encoder {
    pub fn new() -> Result<Self, MediaError> {
        let mut encoder =
            opus::Encoder::new(PLAYBACK_RATE, opus::Channels::Mono, opus::Application::Voip)
                .map_err(|_| failed(t("crear el codificador")))?;
        encoder
            .set_bitrate(opus::Bitrate::Bits(64_000))
            .map_err(|_| failed(t("tasa de bits")))?;
        // Lets the receiver rebuild one lost packet from the next.
        encoder
            .set_inband_fec(true)
            .map_err(|_| failed(t("corrección de errores")))?;
        encoder
            .set_packet_loss_perc(10)
            .map_err(|_| failed(t("pérdida esperada")))?;
        Ok(Self(encoder))
    }

    /// Encode one frame. Out-of-range samples are clipped and invalid ones
    /// are silence, so a broken input never reaches the network as noise.
    pub fn encode(&mut self, frame: &[f32; FRAME]) -> Result<Vec<u8>, MediaError> {
        let mut clean = [0.0; FRAME];
        for (out, sample) in clean.iter_mut().zip(frame) {
            *out = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        let mut packet = [0; MAX_PACKET];
        let length = self
            .0
            .encode_float(&clean, &mut packet)
            .map_err(|_| failed(t("codificar")))?;
        Ok(packet[..length].to_vec())
    }
}

/// Opus decoder for one remote stream.
pub struct Decoder(opus::Decoder);

impl Decoder {
    pub fn new() -> Result<Self, MediaError> {
        opus::Decoder::new(PLAYBACK_RATE, opus::Channels::Mono)
            .map(Self)
            .map_err(|_| failed(t("crear el decodificador")))
    }

    /// Decode `packet` into `out`, which must hold [`MAX_PACKET_SAMPLES`];
    /// returns the samples written.
    pub fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, MediaError> {
        if packet.is_empty() || packet.len() > MAX_PACKET {
            return Err(failed(t("paquete inválido")));
        }
        self.0
            .decode_float(packet, out, false)
            .map_err(|_| failed(t("decodificar")))
    }

    /// Stand in for one lost frame: from `next`'s redundancy when it already
    /// arrived, otherwise by concealment. Writes [`FRAME`] samples.
    pub fn conceal(&mut self, next: Option<&[u8]>, out: &mut [f32; FRAME]) {
        let recovered = next
            .filter(|next| !next.is_empty() && next.len() <= MAX_PACKET)
            .and_then(|next| self.0.decode_float(next, out, true).ok())
            == Some(FRAME);
        if !recovered && self.0.decode_float(&[], out, false).ok() != Some(FRAME) {
            out.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frame: usize) -> [f32; FRAME] {
        std::array::from_fn(|n| {
            let t = (frame * FRAME + n) as f32 / PLAYBACK_RATE as f32;
            0.4 * (t * 440.0 * std::f32::consts::TAU).sin()
        })
    }

    #[test]
    fn a_tone_survives_encoding_and_a_lost_frame_is_concealed() {
        let mut encoder = Encoder::new().unwrap();
        let mut decoder = Decoder::new().unwrap();
        let mut out = vec![0.0; MAX_PACKET_SAMPLES];
        let mut energy = 0.0;
        for frame in 0..10 {
            let packet = encoder.encode(&tone(frame)).unwrap();
            assert!(!packet.is_empty() && packet.len() <= MAX_PACKET);
            assert_eq!(decoder.decode(&packet, &mut out).unwrap(), FRAME);
            energy = crate::rms(&out[..FRAME]);
        }
        assert!(energy > 0.2, "the tone is heard: {energy}");
        let next = encoder.encode(&tone(11)).unwrap();
        let mut lost = [0.0; FRAME];
        decoder.conceal(Some(&next), &mut lost);
        assert!(crate::rms(&lost) > 0.05, "recovered, not a hole");
        decoder.conceal(None, &mut lost);
        assert!(lost.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn invalid_input_is_rejected_or_cleaned() {
        let mut encoder = Encoder::new().unwrap();
        let mut decoder = Decoder::new().unwrap();
        let mut noisy = [f32::NAN; FRAME];
        noisy[0] = 9.0;
        assert!(encoder.encode(&noisy).is_ok());
        let mut out = vec![0.0; MAX_PACKET_SAMPLES];
        assert!(decoder.decode(&[], &mut out).is_err());
        assert!(decoder.decode(&[0; MAX_PACKET + 1], &mut out).is_err());
    }
}
