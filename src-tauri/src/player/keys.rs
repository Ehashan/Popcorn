//! The fallback window's keyboard: macOS key events to mpv key names. Pure.
//!
//! Most keys go to mpv's built-in bindings. A few are handled by us because
//! mpv's default would quit the core, write files, or needs a window of its own.

#[derive(Debug, PartialEq, Eq)]
pub enum KeyAction {
    Close,
    ToggleFullscreen,
    Ignore,
    /// Send to mpv with the `keypress` command.
    Forward(String),
}

/// `NSEventModifierFlags`.
pub const SHIFT: usize = 1 << 17;
pub const CONTROL: usize = 1 << 18;
pub const OPTION: usize = 1 << 19;
pub const COMMAND: usize = 1 << 20;

fn special_name(key_code: u16) -> Option<&'static str> {
    Some(match key_code {
        36 => "ENTER",
        48 => "TAB",
        49 => "SPACE",
        51 => "BS",
        53 => "ESC",
        115 => "HOME",
        116 => "PGUP",
        117 => "DEL",
        119 => "END",
        121 => "PGDWN",
        123 => "LEFT",
        124 => "RIGHT",
        125 => "DOWN",
        126 => "UP",
        _ => return None,
    })
}

/// `characters` is `charactersIgnoringModifiers`, which still reflects Shift.
pub fn key_action(key_code: u16, characters: &str, modifiers: usize) -> KeyAction {
    let held = |flag: usize| modifiers & flag != 0;
    if held(COMMAND) {
        return if characters.eq_ignore_ascii_case("w") {
            KeyAction::Close
        } else {
            KeyAction::Ignore
        };
    }
    // The Control bindings include more screenshot and quit shortcuts.
    if held(CONTROL) {
        return KeyAction::Ignore;
    }
    let special = special_name(key_code);
    let name = match special {
        Some(name) => name.to_owned(),
        None => match characters.chars().next() {
            // mpv spells this one out because `#` starts a comment.
            Some('#') => "SHARP".to_owned(),
            // The private-use range is where AppKit puts function keys.
            Some(c) if !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c) => c.to_string(),
            _ => return KeyAction::Ignore,
        },
    };
    if !held(OPTION) {
        match name.as_str() {
            // Quit would tear the core down; "quit-watch-later" writes files.
            "ESC" | "q" | "Q" => return KeyAction::Close,
            "f" => return KeyAction::ToggleFullscreen,
            // Screenshots would write images to the user's disk.
            "s" | "S" => return KeyAction::Ignore,
            _ => {}
        }
    }
    let mut full = String::new();
    if held(OPTION) {
        full.push_str("Alt+");
    }
    if held(SHIFT) && special.is_some() {
        full.push_str("Shift+");
    }
    full.push_str(&name);
    KeyAction::Forward(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(name: &str) -> KeyAction {
        KeyAction::Forward(name.to_owned())
    }

    #[test]
    fn special_and_printable_keys() {
        assert_eq!(key_action(49, " ", 0), forward("SPACE"));
        assert_eq!(key_action(123, "\u{f702}", SHIFT), forward("Shift+LEFT"));
        assert_eq!(key_action(46, "m", 0), forward("m"));
        assert_eq!(key_action(20, "#", SHIFT), forward("SHARP"));
        assert_eq!(key_action(122, "\u{f704}", 0), KeyAction::Ignore);
    }

    #[test]
    fn dangerous_defaults_are_intercepted() {
        assert_eq!(key_action(53, "\u{1b}", 0), KeyAction::Close);
        assert_eq!(key_action(12, "q", 0), KeyAction::Close);
        assert_eq!(key_action(12, "Q", SHIFT), KeyAction::Close);
        assert_eq!(key_action(1, "s", 0), KeyAction::Ignore);
        assert_eq!(key_action(1, "s", CONTROL), KeyAction::Ignore);
        assert_eq!(key_action(13, "w", COMMAND), KeyAction::Close);
        assert_eq!(key_action(3, "f", 0), KeyAction::ToggleFullscreen);
    }
}
