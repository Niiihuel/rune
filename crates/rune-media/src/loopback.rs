//! What the whole system plays on an output (a game, music, a video, and
//! RUNE's own call), as an echo reference only: the canceller learns how it
//! leaks into the microphone and removes it. It is never encoded, sent,
//! played, stored or measured for anyone: it goes from the callback's ring
//! to [`crate::CaptureProcessor::render`] and nowhere else.
//!
//! On PipeWire it is the output's monitor (cpal opens a sink's `Duplex`
//! input with `stream.capture.sink`); on Windows, WASAPI's loopback of the
//! render endpoint (cpal sets `AUDCLNT_STREAMFLAGS_LOOPBACK` when an output
//! is opened as an input). Other backends refuse, and the canceller keeps
//! RUNE's own reference.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use rtrb::{Producer, RingBuffer};
use rune_i18n::t;

use crate::playback::Linear;
use crate::{EchoReference, MediaError, PLAYBACK_RATE};

/// The monitor's ring: 200 ms, as the output's own reference.
const RING: usize = PLAYBACK_RATE as usize / 5;
const MAX_CHANNELS: u16 = 8;

/// The only state the loopback's real-time callback owns: a downmix to
/// mono and, off 48 kHz, the interpolator to it.
struct Monitor {
    producer: Producer<f32>,
    channels: u16,
    channel: u16,
    sum: f32,
    resampler: Option<Linear>,
}

impl Monitor {
    fn write<T>(&mut self, samples: &[T])
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        for sample in samples {
            let sample = sample.to_sample::<f32>();
            self.sum += if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.channel += 1;
            if self.channel < self.channels {
                continue;
            }
            let mut pending = Some(self.sum / f32::from(self.channels));
            (self.channel, self.sum) = (0, 0.0);
            loop {
                let next = match self.resampler.as_mut() {
                    Some(resampler) => resampler.next(|| pending.take()),
                    None => pending.take(),
                };
                let Some(value) = next else { break };
                // Full: nobody reads it now; the capture trims what is old.
                let _ = self.producer.push(value);
            }
        }
    }
}

/// The device whose output is recorded: `output`, RUNE's output, or the
/// system's default. PipeWire's default output is output-only; its
/// monitor is the `sink_default` cpal lists as `Duplex`.
fn device(host: &cpal::Host, output: Option<&str>) -> Option<cpal::Device> {
    let pipewire = format!("{:?}", host.id()) == "PipeWire";
    let id = match output {
        Some(id) if id.len() > 4096 => return None,
        None | Some("pipewire:output_default") if pipewire => "pipewire:sink_default",
        Some(id) => id,
        None => return host.default_output_device(),
    };
    host.device_by_id(&id.parse().ok()?)
}

/// Open the loopback of `output` (`None`: the system's default output):
/// the stream (dropping it closes it), a flag its errors raise, and what it
/// records. Run it on the capture worker: the audio system may take a moment.
pub(crate) fn open(
    output: Option<&str>,
) -> Result<(cpal::Stream, Arc<AtomicBool>, EchoReference), MediaError> {
    let unavailable = || MediaError(t("No se pudo escuchar lo que suena en la salida.").into());
    let host = cpal::default_host();
    let device = device(&host, output).ok_or_else(unavailable)?;
    // WASAPI offers an output's own format for its loopback.
    let config = device
        .default_input_config()
        .or_else(|_| device.default_output_config())
        .map_err(|_| unavailable())?;
    let rate = config.sample_rate();
    if !(8_000..=192_000).contains(&rate) || !(1..=MAX_CHANNELS).contains(&config.channels()) {
        return Err(unavailable());
    }
    let (producer, consumer) = RingBuffer::new(RING);
    let monitor = Monitor {
        producer,
        channels: config.channels(),
        channel: 0,
        sum: 0.0,
        resampler: (rate != PLAYBACK_RATE).then(|| Linear::new(rate, PLAYBACK_RATE)),
    };
    let failed = Arc::new(AtomicBool::new(false));
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let stream = match format {
        SampleFormat::F32 => stream::<f32>(&device, config, monitor, failed.clone()),
        SampleFormat::F64 => stream::<f64>(&device, config, monitor, failed.clone()),
        SampleFormat::I16 => stream::<i16>(&device, config, monitor, failed.clone()),
        SampleFormat::I32 => stream::<i32>(&device, config, monitor, failed.clone()),
        SampleFormat::U8 => stream::<u8>(&device, config, monitor, failed.clone()),
        SampleFormat::U16 => stream::<u16>(&device, config, monitor, failed.clone()),
        _ => None,
    }
    .ok_or_else(unavailable)?;
    stream.play().map_err(|_| unavailable())?;
    Ok((stream, failed, EchoReference::from_consumer(consumer)))
}

fn stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut monitor: Monitor,
    failed: Arc<AtomicBool>,
) -> Option<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            config,
            move |samples, _| monitor.write(samples),
            move |error| {
                if !matches!(error.kind(), cpal::ErrorKind::Xrun) {
                    failed.store(true, Ordering::Release);
                }
            },
            Some(Duration::from_secs(2)),
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_monitor_downmixes_and_brings_any_rate_to_48_khz() {
        let (producer, mut consumer) = RingBuffer::new(RING);
        let mut monitor = Monitor {
            producer,
            channels: 2,
            channel: 0,
            sum: 0.0,
            resampler: Some(Linear::new(24_000, PLAYBACK_RATE)),
        };
        // A split frame, then nonsense: finite, clamped, averaged.
        monitor.write(&[0.5_f32]);
        monitor.write(&[0.3_f32, f32::NAN, 4.0]);
        monitor.write(&vec![0.25_f32; 2 * 100]);
        let samples: Vec<f32> = std::iter::from_fn(|| consumer.pop().ok()).collect();
        // 102 frames at 24 kHz are about 204 at 48 kHz.
        assert!((200..=206).contains(&samples.len()), "{}", samples.len());
        assert!(samples.iter().all(|sample| (0.0..=0.5).contains(sample)));
        assert!((samples[samples.len() - 1] - 0.25).abs() < 1e-6);
    }
}
