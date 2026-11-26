//! Drives a real libmpv and the real renderer into an offscreen framebuffer.
//! Needs libmpv and a test file, so it only runs on request:
//!
//! ```sh
//! POPCORN_TEST_MEDIA=/path/clip.mkv cargo test engine -- --ignored --nocapture
//! ```
//!
//! The file must be at least two minutes of HEVC with two audio tracks (the
//! first tagged `eng`, the second `jpn`) and two embedded subtitle tracks,
//! with a `clip.srt` beside it.

use std::ffi::{c_int, c_void};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::gl::Renderer;
use super::mpv::{Event, EventReader, Mpv};
use super::{model, Player};

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLSetCurrentContext(context: *mut c_void) -> c_int;
    fn glGenFramebuffers(count: c_int, names: *mut u32);
    fn glBindFramebuffer(target: u32, name: u32);
    fn glGenRenderbuffers(count: c_int, names: *mut u32);
    fn glBindRenderbuffer(target: u32, name: u32);
    fn glRenderbufferStorage(target: u32, format: u32, width: c_int, height: c_int);
    fn glFramebufferRenderbuffer(target: u32, attachment: u32, kind: u32, name: u32);
    fn glCheckFramebufferStatus(target: u32) -> u32;
    fn glReadPixels(
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        format: u32,
        kind: u32,
        pixels: *mut c_void,
    );
}

const FRAMEBUFFER: u32 = 0x8D40;
const RENDERBUFFER: u32 = 0x8D41;
const COLOR_ATTACHMENT: u32 = 0x8CE0;
const COMPLETE: u32 = 0x8CD5;
const RGBA8: u32 = 0x8058;
const RGBA: u32 = 0x1908;
const UNSIGNED_BYTE: u32 = 0x1401;
const WIDTH: c_int = 640;
const HEIGHT: c_int = 360;

fn offscreen_framebuffer(renderer: &Renderer) -> u32 {
    let (mut framebuffer, mut colour) = (0, 0);
    // SAFETY: the renderer's context is made current on this thread, and the
    // calls follow the usual framebuffer setup.
    unsafe {
        CGLSetCurrentContext(renderer.context());
        glGenFramebuffers(1, &mut framebuffer);
        glBindFramebuffer(FRAMEBUFFER, framebuffer);
        glGenRenderbuffers(1, &mut colour);
        glBindRenderbuffer(RENDERBUFFER, colour);
        glRenderbufferStorage(RENDERBUFFER, RGBA8, WIDTH, HEIGHT);
        glFramebufferRenderbuffer(FRAMEBUFFER, COLOR_ATTACHMENT, RENDERBUFFER, colour);
        assert_eq!(glCheckFramebufferStatus(FRAMEBUFFER), COMPLETE);
    }
    framebuffer
}

/// Renders a frame and counts the pixels that are not black.
fn lit_pixels(renderer: &Renderer, framebuffer: u32) -> usize {
    renderer.render(c_int::try_from(framebuffer).unwrap(), WIDTH, HEIGHT, false);
    let mut pixels = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    // SAFETY: the context is current again on this thread, and the buffer
    // holds exactly the requested rectangle.
    unsafe {
        CGLSetCurrentContext(renderer.context());
        glBindFramebuffer(FRAMEBUFFER, framebuffer);
        glReadPixels(
            0,
            0,
            WIDTH,
            HEIGHT,
            RGBA,
            UNSIGNED_BYTE,
            pixels.as_mut_ptr().cast(),
        );
    }
    pixels
        .chunks(4)
        .filter(|p| p[..3].iter().any(|channel| *channel > 24))
        .count()
}

fn wait_for(reader: &mut EventReader, what: &str, wanted: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let event = reader.wait(0.1);
        if wanted(&event) {
            return event;
        }
        assert!(
            !matches!(event, Event::Failed(_)),
            "failed while waiting for {what}: {event:?}"
        );
    }
    panic!("timed out waiting for {what}");
}

fn eventually(what: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn tracks(mpv: &Mpv) -> model::Tracks {
    model::parse_tracks(&mpv.get_string("track-list").unwrap()).unwrap()
}

fn selected(list: &[model::Track]) -> Option<i64> {
    list.iter().find(|t| t.selected).map(|t| t.id)
}

fn core() -> (Arc<Mpv>, Arc<Renderer>, EventReader) {
    // No sound from a test run.
    let mpv = Mpv::new(&[("ao", "null")]).unwrap();
    let renderer = Renderer::new(Arc::clone(&mpv)).unwrap();
    let reader = mpv.event_reader().unwrap();
    assert!(mpv.event_reader().is_none());
    (mpv, renderer, reader)
}

#[test]
#[ignore = "needs libmpv and POPCORN_TEST_MEDIA"]
fn engine_plays_a_real_file() {
    let media = PathBuf::from(std::env::var("POPCORN_TEST_MEDIA").expect("POPCORN_TEST_MEDIA"));
    let path = media.to_str().unwrap();
    let (mpv, renderer, mut reader) = core();
    let framebuffer = offscreen_framebuffer(&renderer);

    mpv.load(path, Some(70.0)).unwrap();
    wait_for(&mut reader, "playback", |e| *e == Event::PlaybackRestart);
    let position = mpv.get_f64("time-pos").unwrap();
    println!("started at {position:.2}s");
    assert!((69.0..73.0).contains(&position));
    assert!(!mpv.get_flag("pause"));

    let found = tracks(&mpv);
    println!("tracks: {found:?}");
    assert_eq!(found.audio.len(), 2);
    assert_eq!(found.audio[0].lang.as_deref(), Some("eng"));
    assert_eq!(found.audio[1].lang.as_deref(), Some("jpn"));
    // Two embedded plus the sidecar that `sub-auto=fuzzy` picked up.
    assert_eq!(found.subtitles.len(), 3, "sidecar subtitle not loaded");

    println!(
        "video {}x{}, codec {:?}",
        mpv.get_string("video-params/w").unwrap(),
        mpv.get_string("video-params/h").unwrap(),
        mpv.get_string("video-codec")
    );
    let mut best = 0;
    for _ in 0..20 {
        best = best.max(lit_pixels(&renderer, framebuffer));
        thread::sleep(Duration::from_millis(50));
    }
    println!(
        "rendered {} frames ({} errors), up to {best} of {} pixels lit",
        renderer.draws(),
        renderer.errors(),
        WIDTH * HEIGHT
    );
    assert_eq!(renderer.errors(), 0);
    assert!(best > (WIDTH * HEIGHT / 4) as usize, "nothing was drawn");
    let hwdec = mpv.get_string("hwdec-current").unwrap_or_default();
    println!("hwdec-current: {hwdec}");
    assert!(hwdec.starts_with("videotoolbox"), "not hardware decoding");

    // The fallback window's keys go through mpv's built-in bindings.
    mpv.command(&["keypress", "SPACE"]).unwrap();
    eventually("SPACE to pause", || mpv.get_flag("pause"));
    let before = mpv.get_f64("time-pos").unwrap();
    mpv.command(&["keypress", "RIGHT"]).unwrap();
    eventually("RIGHT to seek", || {
        mpv.get_f64("time-pos").is_some_and(|t| t > before + 3.0)
    });
    mpv.command(&["keypress", "m"]).unwrap();
    eventually("m to mute", || mpv.get_flag("mute"));
    mpv.toggle_mute().unwrap();
    mpv.toggle_pause().unwrap();
    assert!(!mpv.get_flag("pause"));
    mpv.set_volume(250.0).unwrap();
    assert!((mpv.get_f64("volume").unwrap() - 100.0).abs() < f64::EPSILON);

    mpv.set_audio(found.audio[1].id).unwrap();
    eventually("the second audio track", || {
        selected(&tracks(&mpv).audio) == Some(found.audio[1].id)
    });
    mpv.set_subtitle(Some(found.subtitles[1].id)).unwrap();
    eventually("the second subtitle", || {
        selected(&tracks(&mpv).subtitles) == Some(found.subtitles[1].id)
    });
    mpv.set_subtitle(None).unwrap();
    eventually("subtitles off", || {
        selected(&tracks(&mpv).subtitles).is_none()
    });
    let sidecar = media.with_extension("srt");
    mpv.add_subtitle(sidecar.to_str().unwrap()).unwrap();
    eventually("the added subtitle", || {
        selected(&tracks(&mpv).subtitles).is_some()
    });

    // keep-open: at the end it pauses on the last frame.
    let duration = mpv.get_f64("duration").unwrap();
    mpv.seek(duration - 1.0, false).unwrap();
    for _ in 0..80 {
        lit_pixels(&renderer, framebuffer);
        thread::sleep(Duration::from_millis(50));
        if mpv.get_flag("eof-reached") {
            break;
        }
    }
    assert!(mpv.get_flag("eof-reached"));
    eventually("the keep-open pause", || mpv.get_flag("pause"));
    let end = mpv.get_f64("time-pos").unwrap();
    println!("end of file at {end:.2}s of {duration:.2}s");
    assert!(end > duration * 0.95);

    // Teardown order: render context, then core.
    renderer.shutdown();
    renderer.shutdown();
    renderer.render(0, WIDTH, HEIGHT, false);
    drop(reader);
    drop(renderer);
    assert_eq!(Arc::strong_count(&mpv), 1);
}

#[test]
#[ignore = "needs libmpv"]
fn engine_reports_a_bad_file_and_closes_while_loading() {
    let broken = std::env::temp_dir().join("popcorn-broken-test.mkv");
    std::fs::write(&broken, b"not a video at all").unwrap();
    let (mpv, renderer, mut reader) = core();
    mpv.load(broken.to_str().unwrap(), None).unwrap();
    let event = wait_for_failure(&mut reader);
    println!("{event:?}");
    std::fs::remove_file(&broken).unwrap();
    drop(reader);
    drop(renderer);
    drop(mpv);

    // Opening and closing in quick succession must neither hang nor crash.
    let media = std::env::var("POPCORN_TEST_MEDIA").ok();
    for round in 0..15 {
        let (mpv, renderer, reader) = core();
        if let Some(media) = &media {
            mpv.load(media, None).unwrap();
            thread::sleep(Duration::from_millis(round * 15));
        }
        drop(reader);
        renderer.shutdown();
        drop(mpv);
        drop(renderer);
    }
}

fn wait_for_failure(reader: &mut EventReader) -> Event {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let event = reader.wait(0.1);
        if matches!(event, Event::Failed(_)) {
            return event;
        }
    }
    panic!("no failure was reported");
}
