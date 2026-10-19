//! Measurements of the running window: how long each part of a frame takes,
//! how long input waits to be shown, memory and CPU. Every figure is
//! measured here, on this run; none is an estimate.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Frames kept for the averages and the worst case.
const WINDOW: usize = 120;
const RATE_SPAN: Duration = Duration::from_secs(2);

/// One frame's measured parts.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimes {
    pub view: Duration,
    pub layout: Duration,
    pub paint: Duration,
    /// From the first input event of the frame to the end of its paint.
    pub input_to_paint: Option<Duration>,
}

/// What the window measured lately.
pub struct Metrics {
    started: Instant,
    pub startup: Option<Duration>,
    frames: VecDeque<(Instant, FrameTimes)>,
    /// The CPU time of the previous frame as eframe reports it, rendering included.
    pub cpu_frame: Option<f32>,
    process: Option<(Instant, f64)>,
    pub process_cpu: Option<f64>,
    pub rss_mb: Option<f64>,
    sampled: Option<Instant>,
}

impl Metrics {
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            startup: None,
            frames: VecDeque::new(),
            cpu_frame: None,
            process: None,
            process_cpu: None,
            rss_mb: None,
            sampled: None,
        }
    }

    /// Record a painted frame. The first one dates the startup.
    pub fn frame(&mut self, times: FrameTimes) {
        let now = Instant::now();
        if self.startup.is_none() {
            let startup = now.duration_since(self.started);
            self.startup = Some(startup);
            eprintln!("rune: first frame painted {startup:.1?} after start");
        }
        self.frames.push_back((now, times));
        while self.frames.len() > WINDOW {
            self.frames.pop_front();
        }
        // The process is read twice a second: reading /proc is not free.
        if self
            .sampled
            .is_none_or(|sampled| now.duration_since(sampled) > Duration::from_millis(500))
        {
            self.sampled = Some(now);
            self.rss_mb = resident_megabytes();
            if let Some(cpu) = cpu_seconds() {
                if let Some((then, before)) = self.process {
                    let wall = now.duration_since(then).as_secs_f64();
                    if wall > 0.0 {
                        self.process_cpu = Some((cpu - before) / wall * 100.0);
                    }
                }
                self.process = Some((now, cpu));
            }
        }
    }

    fn average(&self, part: impl Fn(&FrameTimes) -> Duration) -> (Duration, Duration) {
        let count = self.frames.len().max(1) as u32;
        let total: Duration = self.frames.iter().map(|(_, times)| part(times)).sum();
        let worst = self
            .frames
            .iter()
            .map(|(_, times)| part(times))
            .max()
            .unwrap_or_default();
        (total / count, worst)
    }

    /// Completed paint intervals per elapsed second, from the retained frames
    /// near the last two seconds. Time since the last paint counts as well.
    fn rate(&self) -> f64 {
        self.rate_at(Instant::now())
    }

    fn rate_at(&self, now: Instant) -> f64 {
        // Keep the interval crossing the time boundary: dropping its older
        // endpoint would hide a slow frame or the pause before painting resumes.
        let start = self
            .frames
            .iter()
            .rposition(|(when, _)| now.saturating_duration_since(*when) >= RATE_SPAN)
            .unwrap_or(0);
        let Some((first, _)) = self.frames.get(start) else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(*first).as_secs_f64();
        if elapsed == 0.0 {
            return 0.0;
        }
        let intervals = self.frames.len() - start - 1;
        intervals as f64 / elapsed
    }

    /// The panel's lines.
    pub fn lines(&self, messages: usize, rows: Option<std::ops::Range<usize>>) -> Vec<String> {
        let ms = |duration: Duration| duration.as_secs_f64() * 1_000.0;
        let (view, view_worst) = self.average(|times| times.view);
        let (layout, layout_worst) = self.average(|times| times.layout);
        let (paint, paint_worst) = self.average(|times| times.paint);
        let latency: Vec<Duration> = self
            .frames
            .iter()
            .filter_map(|(_, times)| times.input_to_paint)
            .collect();
        let latency = latency
            .iter()
            .copied()
            .max()
            .map_or("—".to_owned(), |worst| {
                format!("{:.1} ms (peor)", ms(worst))
            });
        vec![
            format!("cuadros/s   {:>6.1}", self.rate()),
            format!(
                "vista       {:>6.2} ms  peor {:.2}",
                ms(view),
                ms(view_worst)
            ),
            format!(
                "layout      {:>6.2} ms  peor {:.2}",
                ms(layout),
                ms(layout_worst)
            ),
            format!(
                "pintado     {:>6.2} ms  peor {:.2}",
                ms(paint),
                ms(paint_worst)
            ),
            format!(
                "CPU cuadro  {}",
                self.cpu_frame
                    .map_or("—".into(), |cpu| format!("{:.2} ms", cpu * 1_000.0))
            ),
            format!("entrada→px  {latency}"),
            format!(
                "CPU proceso {}",
                self.process_cpu
                    .map_or("—".into(), |cpu| format!("{cpu:.1} %"))
            ),
            format!(
                "RAM (RSS)   {}",
                self.rss_mb.map_or("—".into(), |rss| format!("{rss:.1} MB"))
            ),
            format!(
                "arranque    {}",
                self.startup
                    .map_or("—".into(), |startup| format!("{:.0} ms", ms(startup)))
            ),
            format!("mensajes    {messages} en memoria"),
            format!(
                "filas       {}",
                rows.map_or("—".into(), |rows| format!(
                    "{}..{} construidas",
                    rows.start, rows.end
                ))
            ),
        ]
    }
}

/// The resident memory of this process, from /proc on Linux.
fn resident_megabytes() -> Option<f64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: f64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4_096.0 / 1_048_576.0)
}

/// CPU seconds this process has used, user and system, from /proc on Linux.
fn cpu_seconds() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The command name may hold spaces; the fields after it are fixed.
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let user: f64 = fields.get(11)?.parse().ok()?;
    let system: f64 = fields.get(12)?.parse().ok()?;
    // Clock ticks are 100 a second on Linux.
    Some((user + system) / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(started: Instant, millis: impl IntoIterator<Item = u64>) -> Metrics {
        let mut metrics = Metrics::new(started);
        metrics.frames = millis
            .into_iter()
            .map(|millis| {
                (
                    started + Duration::from_millis(millis),
                    FrameTimes::default(),
                )
            })
            .collect();
        metrics
    }

    #[test]
    fn frame_rate_can_exceed_sixty_with_the_retained_sample_limit() {
        let started = Instant::now();
        let metrics = frames(started, (0..WINDOW as u64).map(|i| i * 5));
        let now = started + Duration::from_millis((WINDOW as u64 - 1) * 5);
        assert!((metrics.rate_at(now) - 200.0).abs() < 0.001);
    }

    #[test]
    fn startup_rate_uses_observed_intervals_instead_of_two_unobserved_seconds() {
        let started = Instant::now();
        assert_eq!(frames(started, []).rate_at(started), 0.0);
        assert_eq!(
            frames(started, [0]).rate_at(started),
            0.0,
            "one paint has no interval"
        );
        assert_eq!(
            frames(started, [0, 0]).rate_at(started),
            0.0,
            "zero elapsed time is not a rate"
        );
        let now = started + Duration::from_millis(20);
        assert!((frames(started, [0, 10, 20]).rate_at(now) - 100.0).abs() < 0.001);
    }

    #[test]
    fn rate_decays_during_idle_time_and_expires_after_two_seconds_without_painting() {
        let started = Instant::now();
        let metrics = frames(started, [0, 10, 20]);
        let during_pause = started + Duration::from_millis(1_020);
        assert!((metrics.rate_at(during_pause) - 2.0 / 1.02).abs() < 0.001);
        let stale = started + Duration::from_millis(2_020);
        assert_eq!(metrics.rate_at(stale), 0.0);
        assert_eq!(metrics.rate_at(stale + Duration::from_secs(30)), 0.0);
    }

    #[test]
    fn resuming_after_a_pause_counts_the_whole_slow_interval() {
        let started = Instant::now();
        let metrics = frames(started, [0, 10, 20, 3_020]);
        let resumed = started + Duration::from_millis(3_020);
        assert!((metrics.rate_at(resumed) - 1.0 / 3.0).abs() < 0.001);
        let metrics = frames(started, [0, 10, 20, 3_020, 3_025]);
        let now = started + Duration::from_millis(3_025);
        assert!((metrics.rate_at(now) - 2.0 / 3.005).abs() < 0.001);
    }

    #[test]
    fn the_time_window_keeps_its_crossing_interval_and_excludes_older_ones() {
        let started = Instant::now();
        let metrics = frames(started, [0, 1_900, 2_100]);
        let now = started + Duration::from_millis(2_100);
        assert!((metrics.rate_at(now) - 2.0 / 2.1).abs() < 0.001);
        let metrics = frames(started, [0, 500, 1_000, 1_500, 2_000, 2_500]);
        let now = started + Duration::from_millis(2_500);
        assert!((metrics.rate_at(now) - 2.0).abs() < 0.001);
    }

    #[test]
    fn the_process_can_be_read_on_linux() {
        if cfg!(target_os = "linux") {
            assert!(resident_megabytes().is_some_and(|rss| rss > 1.0));
            assert!(cpu_seconds().is_some());
        }
    }

    #[test]
    fn averages_and_the_worst_case_come_from_the_frames() {
        let mut metrics = Metrics::new(Instant::now());
        for millis in [2, 4, 6] {
            metrics.frame(FrameTimes {
                layout: Duration::from_millis(millis),
                ..Default::default()
            });
        }
        assert_eq!(
            metrics.average(|times| times.layout),
            (Duration::from_millis(4), Duration::from_millis(6))
        );
        assert!(metrics.startup.is_some());
        assert_eq!(metrics.lines(10, Some(3..9)).len(), 11);
    }
}
