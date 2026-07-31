//! Parse user-friendly hotkey strings like "ctrl+cmd+j" into
//! `global_hotkey` registrations.

use anyhow::{Context, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// Parse a spec like "ctrl+cmd+j" or "cmd+shift+space".
/// Requires at least one modifier (a bare letter would shadow normal
/// typing system-wide).
pub fn parse(spec: &str) -> Result<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for token in spec.split('+') {
        let t = token.trim().to_ascii_lowercase();
        match t.as_str() {
            "cmd" | "command" | "super" | "meta" => mods |= Modifiers::SUPER,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "opt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "" => {}
            key => {
                if code.is_some() {
                    anyhow::bail!("hotkey \"{spec}\" has more than one non-modifier key");
                }
                code = Some(parse_code(key)?);
            }
        }
    }

    let code = code.with_context(|| format!("hotkey \"{spec}\" is missing a key"))?;
    if mods.is_empty() {
        anyhow::bail!("hotkey \"{spec}\" needs at least one modifier (ctrl/cmd/alt/shift)");
    }
    Ok(HotKey::new(Some(mods), code))
}

fn parse_code(key: &str) -> Result<Code> {
    use std::str::FromStr;

    let normalized = if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) {
        format!("Key{}", key.to_ascii_uppercase())
    } else if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) {
        format!("Digit{key}")
    } else {
        match key {
            "space" => "Space".to_string(),
            "enter" | "return" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "esc" | "escape" => "Escape".to_string(),
            f if f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit()) => {
                f.to_ascii_uppercase()
            }
            other => other.to_string(),
        }
    };

    Code::from_str(&normalized).map_err(|_| anyhow::anyhow!("unrecognized key \"{key}\" in hotkey"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkey() {
        let hk = parse("ctrl+cmd+j").unwrap();
        assert!(hk.mods.contains(Modifiers::CONTROL));
        assert!(hk.mods.contains(Modifiers::SUPER));
        assert_eq!(hk.key, Code::KeyJ);
    }

    #[test]
    fn parses_named_keys_and_digits() {
        assert_eq!(parse("cmd+shift+space").unwrap().key, Code::Space);
        assert_eq!(parse("alt+3").unwrap().key, Code::Digit3);
        assert_eq!(parse("cmd+f5").unwrap().key, Code::F5);
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(parse("j").is_err()); // no modifier
        assert!(parse("cmd+ctrl").is_err()); // no key
        assert!(parse("cmd+j+k").is_err()); // two keys
        assert!(parse("cmd+banana").is_err()); // unknown key
    }
}
