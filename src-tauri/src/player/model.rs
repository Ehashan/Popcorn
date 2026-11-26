//! What the player reports to the UI. Pure.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Track {
    pub id: i64,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Tracks {
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
}

#[derive(Deserialize)]
struct RawTrack {
    id: i64,
    #[serde(rename = "type")]
    kind: String,
    title: Option<String>,
    lang: Option<String>,
    #[serde(default)]
    selected: bool,
}

/// Reads mpv's `track-list` property in its JSON form.
pub fn parse_tracks(json: &str) -> Result<Tracks, serde_json::Error> {
    let mut tracks = Tracks::default();
    for raw in serde_json::from_str::<Vec<RawTrack>>(json)? {
        let list = match raw.kind.as_str() {
            "audio" => &mut tracks.audio,
            "sub" => &mut tracks.subtitles,
            _ => continue,
        };
        list.push(Track {
            id: raw.id,
            title: raw.title,
            lang: raw.lang,
            selected: raw.selected,
        });
    }
    Ok(tracks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    Playing,
    Paused,
    Buffering,
}

pub fn playback_state(ready: bool, paused: bool, waiting_for_data: bool) -> PlaybackState {
    if !ready {
        PlaybackState::Buffering
    } else if paused {
        PlaybackState::Paused
    } else if waiting_for_data {
        PlaybackState::Buffering
    } else {
        PlaybackState::Playing
    }
}

/// Lets something through at most once per interval.
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// True if enough time has passed, or `force` is set.
    pub fn pass(&mut self, now: Instant, force: bool) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due || force {
            self.last = Some(now);
        }
        due || force
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_track_list() {
        let json = r#"[
            {"id":1,"type":"video","selected":true},
            {"id":1,"type":"audio","lang":"eng","title":"Surround","selected":true},
            {"id":2,"type":"audio","lang":"jpn"},
            {"id":1,"type":"sub","selected":false,"external":true}
        ]"#;
        let tracks = parse_tracks(json).unwrap();
        assert_eq!(tracks.audio.len(), 2);
        assert_eq!(tracks.audio[0].title.as_deref(), Some("Surround"));
        assert!(tracks.audio[0].selected && !tracks.audio[1].selected);
        assert_eq!(tracks.subtitles.len(), 1);
        assert_eq!(tracks.subtitles[0].lang, None);
        assert!(parse_tracks("nonsense").is_err());
    }

    #[test]
    fn states() {
        assert_eq!(playback_state(false, true, false), PlaybackState::Buffering);
        assert_eq!(playback_state(true, false, true), PlaybackState::Buffering);
        assert_eq!(playback_state(true, true, true), PlaybackState::Paused);
        assert_eq!(playback_state(true, false, false), PlaybackState::Playing);
    }

    #[test]
    fn throttles_to_the_interval() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut throttle = Throttle::new(Duration::from_millis(250));
        assert!(throttle.pass(at(0), false));
        assert!(!throttle.pass(at(100), false));
        assert!(throttle.pass(at(110), true));
        assert!(!throttle.pass(at(300), false));
        assert!(throttle.pass(at(360), false));
    }
}
