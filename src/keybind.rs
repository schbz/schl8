//! In-app keyboard shortcut combos.
//!
//! Distinct from `hotkey.rs` (the system-wide global hotkey via the
//! `global-hotkey` crate): this matches against egui input for shortcuts
//! that only fire while a Schl8 window is focused. A `KeyCombo` parses
//! from / serializes to strings like `"cmd+shift+s"` and renders a mac
//! glyph label like `⌘⇧S`.

use egui::{InputState, Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCombo {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl KeyCombo {
    /// Parse a spec like `"cmd+shift+s"`. Requires exactly one non-modifier
    /// key. Returns None on unknown tokens or a missing/extra key.
    pub fn parse(spec: &str) -> Option<Self> {
        let mut combo = KeyCombo {
            cmd: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: Key::Space,
        };
        let mut key: Option<Key> = None;
        for token in spec.split('+') {
            match token.trim().to_ascii_lowercase().as_str() {
                "" => {}
                "cmd" | "command" | "super" | "meta" | "win" => combo.cmd = true,
                "ctrl" | "control" => combo.ctrl = true,
                "alt" | "opt" | "option" => combo.alt = true,
                "shift" => combo.shift = true,
                other => {
                    if key.is_some() {
                        return None; // more than one non-modifier key
                    }
                    key = Some(parse_key(other)?);
                }
            }
        }
        combo.key = key?;
        Some(combo)
    }

    /// Build a combo from a captured key event.
    pub fn from_event(key: Key, m: &Modifiers) -> Self {
        KeyCombo {
            cmd: m.command || m.mac_cmd,
            ctrl: m.ctrl,
            alt: m.alt,
            shift: m.shift,
            key,
        }
    }

    /// Whether this combo has at least one modifier (required for a
    /// system-wide hotkey; recommended for in-app ones too).
    pub fn has_modifier(&self) -> bool {
        self.cmd || self.ctrl || self.alt || self.shift
    }

    /// True if `input` shows this exact combo pressed this frame. Modifier
    /// state must match exactly so `cmd+s` doesn't also fire on
    /// `cmd+shift+s`.
    pub fn matches(&self, input: &InputState) -> bool {
        let m = &input.modifiers;
        input.key_pressed(self.key)
            && (m.command || m.mac_cmd) == self.cmd
            && m.ctrl == self.ctrl
            && m.alt == self.alt
            && m.shift == self.shift
    }

    /// Config form, e.g. `"cmd+shift+s"`.
    pub fn to_config_string(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.cmd {
            parts.push("cmd");
        }
        let key = key_name(self.key);
        let mut s = parts.join("+");
        if !s.is_empty() {
            s.push('+');
        }
        s.push_str(&key);
        s
    }

    /// Readable shortcut label, e.g. `Ctrl+Shift+Cmd+S`.
    ///
    /// Spelled out rather than using the Mac glyphs: egui bundles its own
    /// fonts with no system fallback, and of ⌘⌃⌥⇧ only ⌘ is covered — the
    /// rest render as tofu boxes. A consistent word form beats a mix of
    /// symbols and squares (see the glyph-coverage test in ui::theme).
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Opt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.cmd {
            s.push_str("Cmd+");
        }
        s.push_str(&key_glyph(self.key));
        s
    }
}

/// Parse a key name (lowercase) into an egui `Key`.
pub fn parse_key(name: &str) -> Option<Key> {
    // Single ascii letter or digit
    if name.len() == 1 {
        let c = name.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Key::from_name(&c.to_ascii_uppercase().to_string());
        }
        if c.is_ascii_digit() {
            return Key::from_name(&format!("Num{c}")).or_else(|| Key::from_name(name));
        }
    }
    match name {
        "space" => Some(Key::Space),
        "enter" | "return" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "esc" | "escape" => Some(Key::Escape),
        "," | "comma" => Some(Key::Comma),
        "." | "period" => Some(Key::Period),
        "/" | "slash" => Some(Key::Slash),
        ";" | "semicolon" => Some(Key::Semicolon),
        "-" | "minus" => Some(Key::Minus),
        "=" | "equals" | "plus" => Some(Key::Equals),
        "[" | "openbracket" => Some(Key::OpenBracket),
        "]" | "closebracket" => Some(Key::CloseBracket),
        "backslash" | "\\" => Some(Key::Backslash),
        "backtick" | "`" => Some(Key::Backtick),
        f if f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit()) && f.len() > 1 => {
            Key::from_name(&f.to_ascii_uppercase())
        }
        other => Key::from_name(other),
    }
}

/// Canonical lowercase name of a key for config output.
pub fn key_name(key: Key) -> String {
    match key {
        Key::Space => "space".to_string(),
        Key::Enter => "enter".to_string(),
        Key::Tab => "tab".to_string(),
        Key::Escape => "esc".to_string(),
        Key::Comma => "comma".to_string(),
        Key::Period => "period".to_string(),
        Key::Slash => "slash".to_string(),
        Key::Semicolon => "semicolon".to_string(),
        Key::Minus => "minus".to_string(),
        Key::Equals => "equals".to_string(),
        _ => key.name().to_ascii_lowercase(),
    }
}

/// A short glyph for display (single letters for A–Z, symbols for punctuation).
fn key_glyph(key: Key) -> String {
    match key {
        Key::Space => "Space".to_string(),
        Key::Enter => "Enter".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::Escape => "Esc".to_string(),
        Key::Comma => ",".to_string(),
        Key::Period => ".".to_string(),
        Key::Slash => "/".to_string(),
        Key::Semicolon => ";".to_string(),
        Key::Minus => "-".to_string(),
        Key::Equals => "=".to_string(),
        _ => key.name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for spec in ["cmd+s", "cmd+shift+s", "ctrl+alt+j", "cmd+comma"] {
            let c = KeyCombo::parse(spec).unwrap_or_else(|| panic!("parse {spec}"));
            // Re-parsing the serialized form yields the same combo.
            let back = KeyCombo::parse(&c.to_config_string()).unwrap();
            assert_eq!(c, back, "roundtrip for {spec}");
        }
    }

    #[test]
    fn parse_specifics() {
        let c = KeyCombo::parse("cmd+shift+s").unwrap();
        assert!(c.cmd && c.shift && !c.alt && !c.ctrl);
        assert_eq!(c.key, Key::S);
        assert_eq!(c.to_config_string(), "shift+cmd+s");
        assert!(c.display().contains("Cmd+"));
    }

    #[test]
    fn rejects_bad() {
        assert!(KeyCombo::parse("cmd").is_none()); // no key
        assert!(KeyCombo::parse("cmd+s+t").is_none()); // two keys
        assert!(KeyCombo::parse("cmd+banana").is_none()); // unknown key
    }

    #[test]
    fn comma_key() {
        let c = KeyCombo::parse("cmd+,").unwrap();
        assert_eq!(c.key, Key::Comma);
    }
}
