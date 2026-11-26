//! The fixed mpv options. Nothing is read from the user's machine.

pub struct MpvOption {
    pub name: &'static str,
    pub value: &'static str,
    /// Optional ones may not exist in a slim build.
    pub required: bool,
}

const fn required(name: &'static str, value: &'static str) -> MpvOption {
    MpvOption {
        name,
        value,
        required: true,
    }
}

const fn optional(name: &'static str, value: &'static str) -> MpvOption {
    MpvOption {
        name,
        value,
        required: false,
    }
}

pub const OPTIONS: &[MpvOption] = &[
    // No user configuration, no scripts, no files written.
    required("config", "no"),
    required("terminal", "no"),
    required("resume-playback", "no"),
    required("save-position-on-quit", "no"),
    // A build with no scripting at all (ours) has no such option.
    optional("load-scripts", "no"),
    optional("osc", "no"),
    optional("load-osd-console", "no"),
    optional("load-stats-overlay", "no"),
    optional("load-auto-profiles", "no"),
    // Local files only.
    optional("ytdl", "no"),
    required("access-references", "no"),
    required("demuxer-lavf-o", "protocol_whitelist=[file]"),
    // Video is drawn through the render API on macOS, or via `wid` on Windows.
    #[cfg(target_os = "macos")]
    required("vo", "libmpv"),
    #[cfg(target_os = "windows")]
    required("vo", "gpu"),
    // Hardware decoding per platform, with software fallback.
    #[cfg(target_os = "macos")]
    required("hwdec", "videotoolbox,videotoolbox-copy,no"),
    #[cfg(target_os = "windows")]
    required("hwdec", "auto-safe,no"),
    required("sub-auto", "fuzzy"),
    required("keep-open", "yes"),
    required("idle", "yes"),
    required("volume-max", "100"),
    // Only used by the fallback window, which forwards its keys to mpv.
    required("input-default-bindings", "yes"),
    // Local files need little read-ahead; this caps memory use.
    required("demuxer-max-bytes", "64MiB"),
    required("demuxer-max-back-bytes", "32MiB"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn value(name: &str) -> Option<&str> {
        OPTIONS.iter().find(|o| o.name == name).map(|o| o.value)
    }

    #[test]
    fn user_config_and_scripts_stay_off() {
        assert_eq!(value("config"), Some("no"));
        assert_eq!(value("load-scripts"), Some("no"));
        assert_eq!(value("ytdl"), Some("no"));
        assert_eq!(value("keep-open"), Some("yes"));
        assert_eq!(value("sub-auto"), Some("fuzzy"));
    }
}
