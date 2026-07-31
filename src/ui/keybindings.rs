use egui::{Key, Modifiers};

/// Actions that can be triggered by keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    GoToTop,
    GoToBottom,
    Quit,
}

/// Physical keyboard layout, so the vim-style motion keys land under the
/// same fingers regardless of the OS layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Qwerty,
    Dvorak,
    Colemak,
    Workman,
}

impl Layout {
    pub fn parse(name: &str) -> Layout {
        match name.trim().to_ascii_lowercase().as_str() {
            "dvorak" => Layout::Dvorak,
            "colemak" => Layout::Colemak,
            "workman" => Layout::Workman,
            _ => Layout::Qwerty,
        }
    }
}

/// The egui keys that sit at the QWERTY j/k/d/u/g physical positions for a
/// given layout (what the OS actually reports when those keys are pressed).
struct NavKeys {
    down: Key, // QWERTY 'j'
    up: Key,   // QWERTY 'k'
    pgdn: Key, // QWERTY 'd'
    pgup: Key, // QWERTY 'u'
    goto: Key, // QWERTY 'g'
}

fn nav_keys(layout: Layout) -> NavKeys {
    match layout {
        Layout::Qwerty => NavKeys {
            down: Key::J,
            up: Key::K,
            pgdn: Key::D,
            pgup: Key::U,
            goto: Key::G,
        },
        Layout::Dvorak => NavKeys {
            down: Key::H,
            up: Key::T,
            pgdn: Key::E,
            pgup: Key::G,
            goto: Key::I,
        },
        Layout::Colemak => NavKeys {
            down: Key::N,
            up: Key::E,
            pgdn: Key::S,
            pgup: Key::L,
            goto: Key::D,
        },
        Layout::Workman => NavKeys {
            down: Key::N,
            up: Key::E,
            pgdn: Key::H,
            pgup: Key::F,
            goto: Key::G,
        },
    }
}

/// Map an egui key event to a navigation Action for the given layout.
/// Arrow/Page/Home/End keys are always available and layout-independent.
pub fn map_key(key: Key, modifiers: &Modifiers, layout: Layout) -> Option<Action> {
    // Standard navigation — same on every layout.
    match key {
        Key::ArrowDown => return Some(Action::ScrollDown),
        Key::ArrowUp => return Some(Action::ScrollUp),
        Key::PageDown => return Some(Action::PageDown),
        Key::PageUp => return Some(Action::PageUp),
        Key::Home => return Some(Action::GoToTop),
        Key::End => return Some(Action::GoToBottom),
        Key::Q => return Some(Action::Quit), // quit stays mnemonic 'q'
        _ => {}
    }

    // Vim-style motions, remapped to physical position for the layout.
    let nav = nav_keys(layout);
    if key == nav.down {
        Some(Action::ScrollDown)
    } else if key == nav.up {
        Some(Action::ScrollUp)
    } else if key == nav.pgdn {
        Some(Action::PageDown)
    } else if key == nav.pgup {
        Some(Action::PageUp)
    } else if key == nav.goto {
        if modifiers.shift {
            Some(Action::GoToBottom)
        } else {
            Some(Action::GoToTop)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwerty_vim_keys() {
        let m = Modifiers::default();
        assert_eq!(
            map_key(Key::J, &m, Layout::Qwerty),
            Some(Action::ScrollDown)
        );
        assert_eq!(map_key(Key::K, &m, Layout::Qwerty), Some(Action::ScrollUp));
        assert_eq!(map_key(Key::G, &m, Layout::Qwerty), Some(Action::GoToTop));
    }

    #[test]
    fn shift_goto_is_bottom() {
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            map_key(Key::G, &shift, Layout::Qwerty),
            Some(Action::GoToBottom)
        );
        // Colemak: physical QWERTY-g is Key::D
        assert_eq!(
            map_key(Key::D, &shift, Layout::Colemak),
            Some(Action::GoToBottom)
        );
    }

    #[test]
    fn colemak_positions() {
        let m = Modifiers::default();
        // Physical QWERTY-j (Colemak 'n') should scroll down.
        assert_eq!(
            map_key(Key::N, &m, Layout::Colemak),
            Some(Action::ScrollDown)
        );
        // Physical QWERTY-k (Colemak 'e') should scroll up.
        assert_eq!(map_key(Key::E, &m, Layout::Colemak), Some(Action::ScrollUp));
        // Literal 'j' on Colemak is not a motion.
        assert_eq!(map_key(Key::J, &m, Layout::Colemak), None);
    }

    #[test]
    fn dvorak_positions() {
        let m = Modifiers::default();
        assert_eq!(
            map_key(Key::H, &m, Layout::Dvorak),
            Some(Action::ScrollDown)
        );
        assert_eq!(map_key(Key::T, &m, Layout::Dvorak), Some(Action::ScrollUp));
    }

    #[test]
    fn arrows_always_work() {
        let m = Modifiers::default();
        for layout in [
            Layout::Qwerty,
            Layout::Dvorak,
            Layout::Colemak,
            Layout::Workman,
        ] {
            assert_eq!(
                map_key(Key::ArrowDown, &m, layout),
                Some(Action::ScrollDown)
            );
            assert_eq!(map_key(Key::Home, &m, layout), Some(Action::GoToTop));
        }
    }
}
