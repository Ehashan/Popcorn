//! When to save watch progress for the one file a session plays. Pure.
//!
//! A tracker belongs to exactly one file, and each file gets its own mpv core,
//! so a position can never be written for the wrong file.

use std::time::{Duration, Instant};

use crate::progress::reaches_watched;

pub const SAVE_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Save {
    pub file_id: i64,
    pub position: f64,
    pub duration: Option<f64>,
}

impl Save {
    pub fn marks_watched(&self) -> bool {
        reaches_watched(self.position, self.duration)
    }
}

pub struct Tracker {
    file_id: i64,
    /// Playback has started, so positions are real. Until then nothing is
    /// saved, which protects the stored resume point while a file loads.
    ready: bool,
    /// At the end of the file, which has been marked watched.
    finished: bool,
    closed: bool,
    position: Option<f64>,
    duration: Option<f64>,
    saved_at: Option<Instant>,
    saved_position: Option<f64>,
}

impl Tracker {
    pub fn new(file_id: i64) -> Self {
        Self {
            file_id,
            ready: false,
            finished: false,
            closed: false,
            position: None,
            duration: None,
            saved_at: None,
            saved_position: None,
        }
    }

    pub fn file_id(&self) -> i64 {
        self.file_id
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn duration(&self) -> Option<f64> {
        self.duration
    }

    pub fn position(&self) -> Option<f64> {
        self.position
    }

    /// Playback started. The periodic clock starts here.
    pub fn start(&mut self, now: Instant) {
        if !self.ready {
            self.ready = true;
            self.saved_at = Some(now);
        }
    }

    pub fn set_duration(&mut self, duration: f64) {
        if duration > 0.0 {
            self.duration = Some(duration);
        }
    }

    /// Ignored until playback has started.
    pub fn set_position(&mut self, position: f64) {
        if self.ready {
            self.position = Some(position.max(0.0));
        }
    }

    /// The five-second save. Skips a position that is already stored.
    pub fn periodic(&mut self, now: Instant) -> Option<Save> {
        let due = self
            .saved_at
            .is_some_and(|at| now.saturating_duration_since(at) >= SAVE_EVERY);
        if !due || self.position == self.saved_position {
            return None;
        }
        self.checkpoint(now)
    }

    /// A save right now: on pause, after a seek and on close.
    pub fn checkpoint(&mut self, now: Instant) -> Option<Save> {
        if !self.ready || self.finished || self.closed {
            return None;
        }
        let position = self.position?;
        self.saved_at = Some(now);
        self.saved_position = Some(position);
        Some(Save {
            file_id: self.file_id,
            position,
            duration: self.duration,
        })
    }

    /// The last save. Nothing is saved after this.
    pub fn close(&mut self, now: Instant) -> Option<Save> {
        let save = self.checkpoint(now);
        self.closed = true;
        save
    }

    /// Follows mpv's `eof-reached`. Returns true when the end has just been
    /// reached and the file must be marked watched.
    pub fn set_at_end(&mut self, at_end: bool) -> bool {
        if self.closed || !self.ready {
            return false;
        }
        let reached = at_end && !self.finished;
        if self.finished && !at_end {
            // The user went back: store the next position even if unchanged.
            self.saved_position = None;
        }
        self.finished = at_end;
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn nothing_is_saved_while_loading() {
        let start = Instant::now();
        let mut tracker = Tracker::new(7);
        tracker.set_duration(1000.0);
        tracker.set_position(0.0);
        assert_eq!(tracker.checkpoint(start), None);
        assert_eq!(tracker.periodic(at(start, 60)), None);
        assert!(!tracker.set_at_end(true));
        assert_eq!(tracker.close(at(start, 61)), None);
    }

    #[test]
    fn saves_every_five_seconds_when_moving() {
        let start = Instant::now();
        let mut tracker = Tracker::new(7);
        tracker.start(start);
        tracker.set_duration(1000.0);
        tracker.set_position(100.0);
        assert_eq!(tracker.periodic(at(start, 4)), None);
        let save = tracker.periodic(at(start, 5)).unwrap();
        assert_eq!((save.file_id, save.position), (7, 100.0));
        // Unchanged, as when paused: no repeat.
        assert_eq!(tracker.periodic(at(start, 20)), None);
        tracker.set_position(104.0);
        assert!(tracker.periodic(at(start, 21)).is_some());
        tracker.set_position(105.0);
        assert_eq!(tracker.periodic(at(start, 22)), None);
        assert!(tracker.checkpoint(at(start, 22)).is_some());
    }

    #[test]
    fn nothing_is_saved_after_close() {
        let start = Instant::now();
        let mut tracker = Tracker::new(7);
        tracker.start(start);
        tracker.set_position(300.0);
        assert!(tracker.close(start).is_some());
        tracker.set_position(0.0);
        assert_eq!(tracker.checkpoint(start), None);
        assert_eq!(tracker.periodic(at(start, 30)), None);
    }

    #[test]
    fn the_end_blocks_saves_until_the_user_goes_back() {
        let start = Instant::now();
        let mut tracker = Tracker::new(7);
        tracker.start(start);
        tracker.set_duration(100.0);
        tracker.set_position(99.9);
        assert!(tracker.set_at_end(true));
        assert!(!tracker.set_at_end(true));
        // The pause that comes with keep-open must not undo "watched".
        assert_eq!(tracker.checkpoint(start), None);
        assert!(!tracker.set_at_end(false));
        tracker.set_position(50.0);
        let save = tracker.close(start).unwrap();
        assert!(!save.marks_watched());
    }

    #[test]
    fn ninety_percent_counts_as_watched() {
        let save = Save {
            file_id: 1,
            position: 90.0,
            duration: Some(100.0),
        };
        assert!(save.marks_watched());
    }
}
