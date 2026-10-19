//! A video's pictures, decoded a little ahead of the clip's clock and handed
//! over as each is due; a jump starts again from the picture before it that
//! the decoder can start at. Only the newest due picture waits for the host.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use rune_i18n::t;

use super::Shared;
use crate::mp4::{self, VideoTrack};
use crate::video::decode::{file_decoder, rgba};
use crate::video::Frame;
use crate::PLAYBACK_RATE;

/// Pictures decoded ahead of the clock.
const AHEAD: usize = 2;
const IDLE: Duration = Duration::from_millis(5);
/// Pictures that would not decode in a row before the video is given up.
const MAX_ERRORS: usize = 30;

/// Where decoding is: the next sample to decode and when each picture that
/// comes out shows, in order.
struct Place {
    next: usize,
    shows: VecDeque<i64>,
}

/// Decode from the picture before `show` the decoder can start at.
fn place(track: &VideoTrack, show: i64) -> Place {
    let range = mp4::from_sync(track, show);
    let mut shows: Vec<i64> = track.samples[range.clone()]
        .iter()
        .map(|sample| sample.show)
        .collect();
    // Pictures come out in the order they show.
    shows.sort_unstable();
    Place {
        next: range.start,
        shows: shows.into(),
    }
}

pub(super) fn run(file: &[u8], track: &VideoTrack, shared: &Shared) {
    let Ok(mut decoder) = file_decoder() else {
        return shared.fail(crate::MediaError(
            t("No se pudo iniciar el decodificador de video.").into(),
        ));
    };
    let scale = i64::from(track.timescale);
    let start = track
        .samples
        .iter()
        .map(|sample| sample.show)
        .min()
        .unwrap_or(0);
    // The clock, in the track's scale.
    let now = || {
        let heard = shared.position.load(Ordering::Acquire) as i128;
        start + (heard * i128::from(scale) / i128::from(PLAYBACK_RATE)) as i64
    };
    let mut seeks = shared.seeks.load(Ordering::Acquire);
    let mut at = place(track, now());
    let mut ready: VecDeque<(i64, Frame)> = VecDeque::new();
    let mut errors = 0;
    while !shared.stopped() {
        let jumps = shared.seeks.load(Ordering::Acquire);
        if jumps != seeks {
            seeks = jumps;
            decoder = match file_decoder() {
                Ok(decoder) => decoder,
                Err(_) => return,
            };
            at = place(track, now());
            ready.clear();
        }
        // Hand over the newest picture that is due; earlier due ones are late.
        let clock = now();
        let mut due = None;
        while ready.front().is_some_and(|(show, _)| *show <= clock) {
            due = ready.pop_front();
        }
        if let Some((_, frame)) = due {
            *shared
                .picture
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(frame);
        }
        if ready.len() >= AHEAD || at.shows.is_empty() {
            thread::park_timeout(IDLE);
            continue;
        }
        let decoded: Result<Vec<Frame>, _> = if at.next < track.samples.len() {
            let Some(unit) = track.annex_b(file, at.next) else {
                return shared.fail(crate::MediaError(t("El video está dañado.").into()));
            };
            at.next += 1;
            decoder.decode(&unit).map(|picture| {
                picture
                    .and_then(|picture| rgba(&picture))
                    .into_iter()
                    .collect()
            })
        } else {
            // The last pictures wait in the decoder until it is flushed.
            let flushed = decoder
                .flush_remaining()
                .map(|pictures| pictures.iter().filter_map(rgba).collect::<Vec<_>>());
            if flushed.as_ref().is_ok_and(Vec::is_empty) || flushed.is_err() {
                at.shows.clear();
            }
            flushed
        };
        match decoded {
            Ok(frames) => {
                errors = 0;
                for frame in frames {
                    if let Some(show) = at.shows.pop_front() {
                        ready.push_back((show, frame));
                    }
                }
            }
            Err(_) => {
                errors += 1;
                if errors > MAX_ERRORS {
                    return shared.fail(crate::MediaError(
                        t("RUNE no pudo decodificar este video.").into(),
                    ));
                }
            }
        }
    }
}
