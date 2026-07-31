//! Runtime theme engine.
//!
//! Colors are exposed as functions backed by a palette chosen once at
//! startup from `[appearance]` in the config (preset name, optional
//! accent override). Layout metrics stay as constants.
//!
//! No UI code should hardcode colors — add a palette field instead.

use std::sync::RwLock;

use egui::Color32;

/// A full color palette. Presets fill this; the config may override the
/// accent.
#[derive(Clone, PartialEq)]
pub struct Palette {
    // Backgrounds
    pub bg_primary: Color32,
    pub bg_statusbar: Color32,
    pub bg_sidebar: Color32,
    pub bg_editor: Color32,
    pub bg_code: Color32,
    pub bg_quote: Color32,
    /// Raised elements: secondary buttons, cards, table fills.
    pub bg_raised: Color32,
    // Text
    pub text_primary: Color32,
    pub text_dim: Color32,
    pub text_strong: Color32,
    pub text_editor: Color32,
    pub text_code: Color32,
    // Accents
    /// Primary accent (links, highlights, primary buttons).
    pub accent: Color32,
    /// Gradient partner to `accent` (the icon's cyan→violet identity).
    pub accent_alt: Color32,
    pub accent_yellow: Color32,
    pub accent_red: Color32,
    pub accent_green: Color32,
    pub accent_purple: Color32,
    pub quote_bar: Color32,
    // Chips / badges
    pub badge_bg: Color32,
    pub badge_text: Color32,
}

/// The widest a centered dialog may be and still fit the app window.
///
/// Dialogs are anchored to the center, so one wider than the window
/// hangs off *both* sides at once and the outer columns become
/// unreachable — vertical scrolling cannot recover a part that never
/// fit horizontally. Every dialog clamps through here so a narrow
/// window shrinks them instead of clipping them.
pub fn dialog_max_width(ctx: &egui::Context) -> f32 {
    // 40pt keeps a little air around the edges; the floor stops an
    // absurdly small window from collapsing a dialog to nothing.
    (ctx.screen_rect().width() - 40.0).max(280.0)
}

/// A dialog's preferred width, shrunk to fit. Unchanged when it fits.
pub fn dialog_width(ctx: &egui::Context, preferred: f32) -> f32 {
    preferred.min(dialog_max_width(ctx))
}

/// Every selectable palette, in menu order.
///
/// The settings picker iterates this rather than its own list: a name
/// here that `preset` does not match falls silently through to the
/// default, which looks like "the theme did nothing". A test walks this
/// list and fails on exactly that.
pub const PRESETS: &[&str] = &[
    "slate", "midnight", "plum", "forest", "abyss", "nebula", "neon", "ember", "espresso",
    "sakura", "terminal", "phosphor", "paper", "linen", "frost", "moss",
];

fn preset(name: &str) -> Palette {
    match name {
        // Near-black with electric cyan → pink; maximum contrast.
        "midnight" => Palette {
            bg_primary: Color32::from_rgb(11, 14, 20),
            bg_statusbar: Color32::from_rgb(7, 9, 14),
            bg_sidebar: Color32::from_rgb(9, 12, 17),
            bg_editor: Color32::from_rgb(8, 11, 16),
            bg_code: Color32::from_rgb(13, 17, 24),
            bg_quote: Color32::from_rgb(18, 24, 34),
            bg_raised: Color32::from_rgb(22, 28, 40),
            text_primary: Color32::from_rgb(214, 224, 238),
            text_dim: Color32::from_rgb(120, 132, 152),
            text_strong: Color32::from_rgb(244, 248, 255),
            text_editor: Color32::from_rgb(228, 236, 248),
            text_code: Color32::from_rgb(240, 190, 130),
            accent: Color32::from_rgb(125, 249, 255),
            accent_alt: Color32::from_rgb(255, 121, 198),
            accent_yellow: Color32::from_rgb(235, 210, 110),
            accent_red: Color32::from_rgb(255, 105, 120),
            accent_green: Color32::from_rgb(80, 250, 123),
            accent_purple: Color32::from_rgb(189, 147, 249),
            quote_bar: Color32::from_rgb(70, 90, 120),
            badge_bg: Color32::from_rgb(125, 249, 255),
            badge_text: Color32::from_rgb(8, 20, 26),
        },
        // Deep plum with magenta → violet.
        "plum" => Palette {
            bg_primary: Color32::from_rgb(24, 18, 30),
            bg_statusbar: Color32::from_rgb(17, 12, 22),
            bg_sidebar: Color32::from_rgb(20, 15, 26),
            bg_editor: Color32::from_rgb(19, 14, 25),
            bg_code: Color32::from_rgb(27, 20, 35),
            bg_quote: Color32::from_rgb(36, 27, 48),
            bg_raised: Color32::from_rgb(43, 33, 56),
            text_primary: Color32::from_rgb(228, 219, 238),
            text_dim: Color32::from_rgb(150, 136, 168),
            text_strong: Color32::from_rgb(248, 242, 255),
            text_editor: Color32::from_rgb(238, 230, 248),
            text_code: Color32::from_rgb(240, 195, 130),
            accent: Color32::from_rgb(255, 121, 198),
            accent_alt: Color32::from_rgb(189, 147, 249),
            accent_yellow: Color32::from_rgb(235, 205, 120),
            accent_red: Color32::from_rgb(255, 100, 115),
            accent_green: Color32::from_rgb(105, 230, 150),
            accent_purple: Color32::from_rgb(189, 147, 249),
            quote_bar: Color32::from_rgb(110, 85, 145),
            badge_bg: Color32::from_rgb(255, 121, 198),
            badge_text: Color32::from_rgb(35, 10, 26),
        },
        // Dark green-tinted with spring-green → cyan.
        "forest" => Palette {
            bg_primary: Color32::from_rgb(16, 22, 19),
            bg_statusbar: Color32::from_rgb(11, 16, 14),
            bg_sidebar: Color32::from_rgb(13, 19, 16),
            bg_editor: Color32::from_rgb(12, 18, 15),
            bg_code: Color32::from_rgb(17, 25, 21),
            bg_quote: Color32::from_rgb(24, 34, 29),
            bg_raised: Color32::from_rgb(29, 41, 35),
            text_primary: Color32::from_rgb(216, 230, 222),
            text_dim: Color32::from_rgb(130, 150, 140),
            text_strong: Color32::from_rgb(242, 250, 245),
            text_editor: Color32::from_rgb(228, 240, 233),
            text_code: Color32::from_rgb(235, 195, 130),
            accent: Color32::from_rgb(74, 222, 128),
            accent_alt: Color32::from_rgb(34, 211, 238),
            accent_yellow: Color32::from_rgb(230, 205, 110),
            accent_red: Color32::from_rgb(240, 105, 110),
            accent_green: Color32::from_rgb(74, 222, 128),
            accent_purple: Color32::from_rgb(167, 139, 250),
            quote_bar: Color32::from_rgb(70, 105, 88),
            badge_bg: Color32::from_rgb(74, 222, 128),
            badge_text: Color32::from_rgb(8, 26, 15),
        },
        // Light: white background, near-black text, blue → violet accents.
        "paper" => Palette {
            bg_primary: Color32::from_rgb(252, 252, 253),
            bg_statusbar: Color32::from_rgb(238, 240, 244),
            bg_sidebar: Color32::from_rgb(245, 246, 248),
            bg_editor: Color32::from_rgb(255, 255, 255),
            bg_code: Color32::from_rgb(242, 244, 247),
            bg_quote: Color32::from_rgb(238, 241, 246),
            bg_raised: Color32::from_rgb(230, 233, 239),
            text_primary: Color32::from_rgb(26, 30, 38),
            text_dim: Color32::from_rgb(108, 116, 130),
            text_strong: Color32::from_rgb(8, 10, 14),
            text_editor: Color32::from_rgb(18, 22, 30),
            text_code: Color32::from_rgb(146, 84, 20),
            accent: Color32::from_rgb(2, 122, 199),
            accent_alt: Color32::from_rgb(124, 58, 237),
            accent_yellow: Color32::from_rgb(178, 128, 12),
            accent_red: Color32::from_rgb(196, 48, 60),
            accent_green: Color32::from_rgb(22, 140, 78),
            accent_purple: Color32::from_rgb(109, 40, 217),
            quote_bar: Color32::from_rgb(150, 160, 175),
            badge_bg: Color32::from_rgb(2, 122, 199),
            badge_text: Color32::from_rgb(250, 252, 255),
        },
        // Light: warm off-white ("old paper") with terracotta accents.
        "linen" => Palette {
            bg_primary: Color32::from_rgb(250, 247, 240),
            bg_statusbar: Color32::from_rgb(238, 233, 222),
            bg_sidebar: Color32::from_rgb(245, 241, 232),
            bg_editor: Color32::from_rgb(253, 251, 246),
            bg_code: Color32::from_rgb(241, 236, 226),
            bg_quote: Color32::from_rgb(240, 234, 222),
            bg_raised: Color32::from_rgb(232, 225, 211),
            text_primary: Color32::from_rgb(46, 40, 32),
            text_dim: Color32::from_rgb(128, 118, 102),
            text_strong: Color32::from_rgb(22, 18, 12),
            text_editor: Color32::from_rgb(40, 34, 26),
            text_code: Color32::from_rgb(146, 84, 14),
            accent: Color32::from_rgb(170, 84, 38),
            accent_alt: Color32::from_rgb(110, 88, 160),
            accent_yellow: Color32::from_rgb(168, 124, 14),
            accent_red: Color32::from_rgb(182, 54, 48),
            accent_green: Color32::from_rgb(62, 128, 70),
            accent_purple: Color32::from_rgb(122, 86, 170),
            quote_bar: Color32::from_rgb(182, 168, 144),
            badge_bg: Color32::from_rgb(170, 84, 38),
            badge_text: Color32::from_rgb(252, 248, 242),
        },
        // Warm charcoal lit by a fire: orange into red.
        "ember" => Palette {
            bg_primary: Color32::from_rgb(26, 22, 20),
            bg_statusbar: Color32::from_rgb(18, 15, 13),
            bg_sidebar: Color32::from_rgb(22, 18, 16),
            bg_editor: Color32::from_rgb(20, 17, 15),
            bg_code: Color32::from_rgb(30, 25, 21),
            bg_quote: Color32::from_rgb(40, 32, 26),
            bg_raised: Color32::from_rgb(48, 38, 31),
            text_primary: Color32::from_rgb(232, 220, 210),
            text_dim: Color32::from_rgb(158, 142, 130),
            text_strong: Color32::from_rgb(255, 245, 238),
            text_editor: Color32::from_rgb(240, 230, 220),
            text_code: Color32::from_rgb(240, 180, 120),
            accent: Color32::from_rgb(255, 138, 60),
            accent_alt: Color32::from_rgb(255, 92, 92),
            accent_yellow: Color32::from_rgb(240, 196, 90),
            accent_red: Color32::from_rgb(255, 118, 108),
            accent_green: Color32::from_rgb(150, 205, 130),
            accent_purple: Color32::from_rgb(198, 148, 220),
            quote_bar: Color32::from_rgb(120, 80, 50),
            badge_bg: Color32::from_rgb(255, 138, 60),
            badge_text: Color32::from_rgb(34, 18, 8),
        },
        // Deep water: near-black blue with bioluminescent teal.
        "abyss" => Palette {
            bg_primary: Color32::from_rgb(10, 22, 30),
            bg_statusbar: Color32::from_rgb(6, 15, 21),
            bg_sidebar: Color32::from_rgb(8, 18, 25),
            bg_editor: Color32::from_rgb(8, 19, 26),
            bg_code: Color32::from_rgb(12, 26, 35),
            bg_quote: Color32::from_rgb(16, 34, 45),
            bg_raised: Color32::from_rgb(20, 42, 55),
            text_primary: Color32::from_rgb(198, 222, 232),
            text_dim: Color32::from_rgb(120, 155, 172),
            text_strong: Color32::from_rgb(232, 246, 252),
            text_editor: Color32::from_rgb(212, 234, 244),
            text_code: Color32::from_rgb(126, 220, 200),
            accent: Color32::from_rgb(56, 214, 214),
            accent_alt: Color32::from_rgb(96, 158, 255),
            accent_yellow: Color32::from_rgb(226, 200, 110),
            accent_red: Color32::from_rgb(240, 118, 128),
            accent_green: Color32::from_rgb(90, 220, 170),
            accent_purple: Color32::from_rgb(158, 158, 250),
            quote_bar: Color32::from_rgb(44, 88, 110),
            badge_bg: Color32::from_rgb(56, 214, 214),
            badge_text: Color32::from_rgb(4, 26, 30),
        },
        // Cherry blossom at night: warm plum-grey with soft pink.
        "sakura" => Palette {
            bg_primary: Color32::from_rgb(30, 24, 28),
            bg_statusbar: Color32::from_rgb(21, 16, 20),
            bg_sidebar: Color32::from_rgb(26, 20, 24),
            bg_editor: Color32::from_rgb(24, 19, 23),
            bg_code: Color32::from_rgb(36, 28, 33),
            bg_quote: Color32::from_rgb(46, 35, 42),
            bg_raised: Color32::from_rgb(54, 42, 50),
            text_primary: Color32::from_rgb(234, 220, 228),
            text_dim: Color32::from_rgb(164, 144, 156),
            text_strong: Color32::from_rgb(252, 242, 248),
            text_editor: Color32::from_rgb(242, 230, 238),
            text_code: Color32::from_rgb(240, 170, 190),
            accent: Color32::from_rgb(247, 152, 184),
            accent_alt: Color32::from_rgb(198, 160, 246),
            accent_yellow: Color32::from_rgb(236, 200, 130),
            accent_red: Color32::from_rgb(244, 118, 138),
            accent_green: Color32::from_rgb(148, 210, 166),
            accent_purple: Color32::from_rgb(198, 160, 246),
            quote_bar: Color32::from_rgb(120, 84, 104),
            badge_bg: Color32::from_rgb(247, 152, 184),
            badge_text: Color32::from_rgb(40, 16, 26),
        },
        // Green phosphor CRT — monochrome, for the full terminal feeling.
        "terminal" => Palette {
            bg_primary: Color32::from_rgb(6, 14, 8),
            bg_statusbar: Color32::from_rgb(3, 9, 5),
            bg_sidebar: Color32::from_rgb(5, 12, 7),
            bg_editor: Color32::from_rgb(4, 11, 6),
            bg_code: Color32::from_rgb(8, 18, 11),
            bg_quote: Color32::from_rgb(11, 24, 14),
            bg_raised: Color32::from_rgb(14, 32, 18),
            text_primary: Color32::from_rgb(150, 240, 160),
            text_dim: Color32::from_rgb(98, 164, 108),
            text_strong: Color32::from_rgb(200, 255, 205),
            text_editor: Color32::from_rgb(150, 245, 160),
            text_code: Color32::from_rgb(186, 255, 146),
            accent: Color32::from_rgb(80, 250, 123),
            accent_alt: Color32::from_rgb(150, 255, 170),
            accent_yellow: Color32::from_rgb(200, 240, 110),
            accent_red: Color32::from_rgb(255, 128, 118),
            accent_green: Color32::from_rgb(80, 250, 123),
            accent_purple: Color32::from_rgb(160, 226, 190),
            quote_bar: Color32::from_rgb(40, 100, 55),
            badge_bg: Color32::from_rgb(80, 250, 123),
            badge_text: Color32::from_rgb(4, 24, 10),
        },
        // Amber phosphor CRT — the other monochrome monitor.
        "phosphor" => Palette {
            bg_primary: Color32::from_rgb(18, 13, 5),
            bg_statusbar: Color32::from_rgb(12, 8, 3),
            bg_sidebar: Color32::from_rgb(15, 11, 4),
            bg_editor: Color32::from_rgb(14, 10, 4),
            bg_code: Color32::from_rgb(24, 17, 7),
            bg_quote: Color32::from_rgb(32, 23, 9),
            bg_raised: Color32::from_rgb(42, 30, 12),
            text_primary: Color32::from_rgb(255, 190, 90),
            text_dim: Color32::from_rgb(182, 134, 62),
            text_strong: Color32::from_rgb(255, 216, 146),
            text_editor: Color32::from_rgb(255, 194, 100),
            text_code: Color32::from_rgb(255, 220, 150),
            accent: Color32::from_rgb(255, 176, 46),
            accent_alt: Color32::from_rgb(255, 214, 120),
            accent_yellow: Color32::from_rgb(255, 214, 120),
            accent_red: Color32::from_rgb(255, 132, 92),
            accent_green: Color32::from_rgb(216, 206, 96),
            accent_purple: Color32::from_rgb(232, 174, 124),
            quote_bar: Color32::from_rgb(110, 78, 28),
            badge_bg: Color32::from_rgb(255, 176, 46),
            badge_text: Color32::from_rgb(30, 18, 2),
        },
        // Deep space: indigo ground, magenta and violet starlight.
        "nebula" => Palette {
            bg_primary: Color32::from_rgb(16, 14, 34),
            bg_statusbar: Color32::from_rgb(10, 9, 24),
            bg_sidebar: Color32::from_rgb(13, 12, 29),
            bg_editor: Color32::from_rgb(12, 11, 28),
            bg_code: Color32::from_rgb(21, 19, 42),
            bg_quote: Color32::from_rgb(28, 25, 54),
            bg_raised: Color32::from_rgb(35, 31, 66),
            text_primary: Color32::from_rgb(216, 214, 240),
            text_dim: Color32::from_rgb(148, 144, 186),
            text_strong: Color32::from_rgb(244, 242, 255),
            text_editor: Color32::from_rgb(226, 224, 248),
            text_code: Color32::from_rgb(200, 170, 255),
            accent: Color32::from_rgb(168, 120, 255),
            accent_alt: Color32::from_rgb(255, 110, 200),
            accent_yellow: Color32::from_rgb(236, 206, 120),
            accent_red: Color32::from_rgb(250, 116, 146),
            accent_green: Color32::from_rgb(110, 226, 190),
            accent_purple: Color32::from_rgb(190, 140, 255),
            quote_bar: Color32::from_rgb(72, 62, 130),
            badge_bg: Color32::from_rgb(168, 120, 255),
            badge_text: Color32::from_rgb(16, 8, 34),
        },
        // Dark roast: coffee browns with cream and caramel.
        "espresso" => Palette {
            bg_primary: Color32::from_rgb(28, 22, 18),
            bg_statusbar: Color32::from_rgb(19, 15, 12),
            bg_sidebar: Color32::from_rgb(24, 19, 15),
            bg_editor: Color32::from_rgb(22, 17, 14),
            bg_code: Color32::from_rgb(34, 27, 21),
            bg_quote: Color32::from_rgb(44, 35, 27),
            bg_raised: Color32::from_rgb(54, 43, 33),
            text_primary: Color32::from_rgb(232, 219, 200),
            text_dim: Color32::from_rgb(164, 146, 124),
            text_strong: Color32::from_rgb(250, 242, 228),
            text_editor: Color32::from_rgb(240, 228, 208),
            text_code: Color32::from_rgb(214, 176, 118),
            accent: Color32::from_rgb(206, 158, 96),
            accent_alt: Color32::from_rgb(158, 196, 138),
            accent_yellow: Color32::from_rgb(226, 190, 110),
            accent_red: Color32::from_rgb(222, 120, 100),
            accent_green: Color32::from_rgb(158, 196, 138),
            accent_purple: Color32::from_rgb(194, 158, 198),
            quote_bar: Color32::from_rgb(110, 88, 62),
            badge_bg: Color32::from_rgb(206, 158, 96),
            badge_text: Color32::from_rgb(32, 22, 12),
        },
        // Rain-slick street at 2am: hot magenta against electric lime.
        "neon" => Palette {
            bg_primary: Color32::from_rgb(10, 10, 14),
            bg_statusbar: Color32::from_rgb(6, 6, 9),
            bg_sidebar: Color32::from_rgb(8, 8, 12),
            bg_editor: Color32::from_rgb(7, 7, 11),
            bg_code: Color32::from_rgb(14, 14, 20),
            bg_quote: Color32::from_rgb(20, 18, 28),
            bg_raised: Color32::from_rgb(26, 24, 36),
            text_primary: Color32::from_rgb(222, 224, 235),
            text_dim: Color32::from_rgb(136, 140, 158),
            text_strong: Color32::from_rgb(248, 250, 255),
            text_editor: Color32::from_rgb(232, 234, 245),
            text_code: Color32::from_rgb(200, 255, 90),
            accent: Color32::from_rgb(255, 60, 172),
            accent_alt: Color32::from_rgb(190, 255, 60),
            accent_yellow: Color32::from_rgb(250, 240, 90),
            accent_red: Color32::from_rgb(255, 80, 100),
            accent_green: Color32::from_rgb(120, 255, 160),
            accent_purple: Color32::from_rgb(200, 110, 255),
            quote_bar: Color32::from_rgb(96, 44, 96),
            badge_bg: Color32::from_rgb(255, 60, 172),
            badge_text: Color32::from_rgb(18, 4, 12),
        },
        // Light: cool ice-white with glacier blue.
        "frost" => Palette {
            bg_primary: Color32::from_rgb(247, 250, 252),
            bg_statusbar: Color32::from_rgb(231, 238, 244),
            bg_sidebar: Color32::from_rgb(240, 245, 250),
            bg_editor: Color32::from_rgb(252, 254, 255),
            bg_code: Color32::from_rgb(235, 242, 248),
            bg_quote: Color32::from_rgb(232, 240, 247),
            bg_raised: Color32::from_rgb(222, 232, 241),
            text_primary: Color32::from_rgb(24, 36, 48),
            text_dim: Color32::from_rgb(92, 112, 130),
            text_strong: Color32::from_rgb(8, 18, 28),
            text_editor: Color32::from_rgb(18, 30, 42),
            text_code: Color32::from_rgb(18, 96, 130),
            accent: Color32::from_rgb(14, 124, 168),
            accent_alt: Color32::from_rgb(86, 92, 200),
            accent_yellow: Color32::from_rgb(160, 116, 16),
            accent_red: Color32::from_rgb(186, 48, 58),
            accent_green: Color32::from_rgb(18, 124, 88),
            accent_purple: Color32::from_rgb(98, 70, 200),
            quote_bar: Color32::from_rgb(150, 172, 190),
            badge_bg: Color32::from_rgb(14, 124, 168),
            badge_text: Color32::from_rgb(246, 252, 255),
        },
        // Light: soft sage and paper, easy on tired eyes.
        "moss" => Palette {
            bg_primary: Color32::from_rgb(247, 250, 244),
            bg_statusbar: Color32::from_rgb(232, 238, 226),
            bg_sidebar: Color32::from_rgb(240, 245, 234),
            bg_editor: Color32::from_rgb(252, 254, 250),
            bg_code: Color32::from_rgb(236, 242, 230),
            bg_quote: Color32::from_rgb(234, 241, 228),
            bg_raised: Color32::from_rgb(224, 233, 216),
            text_primary: Color32::from_rgb(32, 42, 30),
            text_dim: Color32::from_rgb(100, 116, 94),
            text_strong: Color32::from_rgb(14, 22, 12),
            text_editor: Color32::from_rgb(26, 36, 24),
            text_code: Color32::from_rgb(104, 90, 18),
            accent: Color32::from_rgb(52, 122, 68),
            accent_alt: Color32::from_rgb(146, 106, 56),
            accent_yellow: Color32::from_rgb(150, 118, 16),
            accent_red: Color32::from_rgb(178, 54, 48),
            accent_green: Color32::from_rgb(42, 122, 62),
            accent_purple: Color32::from_rgb(108, 80, 156),
            quote_bar: Color32::from_rgb(162, 178, 150),
            badge_bg: Color32::from_rgb(52, 122, 68),
            badge_text: Color32::from_rgb(246, 252, 244),
        },
        // Default: blue-slate dark with cyan → violet (matches the app icon).
        _ => Palette {
            bg_primary: Color32::from_rgb(23, 27, 34),
            bg_statusbar: Color32::from_rgb(15, 18, 24),
            bg_sidebar: Color32::from_rgb(18, 21, 28),
            bg_editor: Color32::from_rgb(17, 20, 27),
            bg_code: Color32::from_rgb(15, 19, 26),
            bg_quote: Color32::from_rgb(29, 36, 48),
            bg_raised: Color32::from_rgb(36, 43, 56),
            text_primary: Color32::from_rgb(215, 222, 232),
            text_dim: Color32::from_rgb(138, 148, 166),
            text_strong: Color32::from_rgb(242, 246, 252),
            text_editor: Color32::from_rgb(228, 234, 244),
            text_code: Color32::from_rgb(232, 184, 123),
            accent: Color32::from_rgb(34, 211, 238),
            accent_alt: Color32::from_rgb(167, 139, 250),
            accent_yellow: Color32::from_rgb(230, 197, 102),
            accent_red: Color32::from_rgb(224, 108, 117),
            accent_green: Color32::from_rgb(87, 217, 133),
            accent_purple: Color32::from_rgb(167, 139, 250),
            quote_bar: Color32::from_rgb(76, 90, 115),
            badge_bg: Color32::from_rgb(34, 211, 238),
            badge_text: Color32::from_rgb(10, 22, 30),
        },
    }
}

/// Const default palette (blue-slate) so the RwLock has a value before
/// `init`/`set` is ever called. Kept in sync with the "slate" preset.
const DEFAULT: Palette = Palette {
    bg_primary: Color32::from_rgb(23, 27, 34),
    bg_statusbar: Color32::from_rgb(15, 18, 24),
    bg_sidebar: Color32::from_rgb(18, 21, 28),
    bg_editor: Color32::from_rgb(17, 20, 27),
    bg_code: Color32::from_rgb(15, 19, 26),
    bg_quote: Color32::from_rgb(29, 36, 48),
    bg_raised: Color32::from_rgb(36, 43, 56),
    text_primary: Color32::from_rgb(215, 222, 232),
    text_dim: Color32::from_rgb(138, 148, 166),
    text_strong: Color32::from_rgb(242, 246, 252),
    text_editor: Color32::from_rgb(228, 234, 244),
    text_code: Color32::from_rgb(232, 184, 123),
    accent: Color32::from_rgb(34, 211, 238),
    accent_alt: Color32::from_rgb(167, 139, 250),
    accent_yellow: Color32::from_rgb(230, 197, 102),
    accent_red: Color32::from_rgb(224, 108, 117),
    accent_green: Color32::from_rgb(87, 217, 133),
    accent_purple: Color32::from_rgb(167, 139, 250),
    quote_bar: Color32::from_rgb(76, 90, 115),
    badge_bg: Color32::from_rgb(34, 211, 238),
    badge_text: Color32::from_rgb(10, 22, 30),
};

static THEME: RwLock<Palette> = RwLock::new(DEFAULT);

/// Set the active theme from config. Applies live — call at startup and
/// whenever the appearance settings change.
pub fn set(appearance: &crate::config::Appearance) {
    let mut p = preset(&appearance.theme);
    if let Some(accent) = parse_hex(&appearance.accent) {
        p.accent = accent;
        p.badge_bg = accent;
        p.badge_text = contrast_text_for(accent);
    }
    *THEME.write().unwrap_or_else(|e| e.into_inner()) = p;
}

/// Alias for `set`, for the startup call site.
pub fn init(appearance: &crate::config::Appearance) {
    set(appearance);
}

/// Read a field from the active palette. Colors are `Copy`, so the lock is
/// held only momentarily.
fn read() -> std::sync::RwLockReadGuard<'static, Palette> {
    THEME.read().unwrap_or_else(|e| e.into_inner())
}

/// Parse "#RRGGBB" / "RRGGBB" into a color. Empty or invalid → None.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

/// Whether the active theme is light (bright background).
pub fn is_light() -> bool {
    let bg = read().bg_primary;
    0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32 > 140.0
}

/// Dark or light text for readable contrast on `bg` (public helper for
/// filled chips like toasts and destructive buttons).
pub fn contrast_text(bg: Color32) -> Color32 {
    contrast_text_for(bg)
}

/// Dark or light text for readable contrast on `bg`.
fn contrast_text_for(bg: Color32) -> Color32 {
    // Perceived luminance (ITU-R BT.601)
    let lum = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if lum > 140.0 {
        Color32::from_rgb(12, 18, 26)
    } else {
        Color32::from_rgb(245, 248, 252)
    }
}

// ── Backgrounds (always fully opaque — translucency would let other
// windows shine through next to decrypted text, so it isn't supported) ─
pub fn bg_primary() -> Color32 {
    read().bg_primary
}
pub fn bg_statusbar() -> Color32 {
    read().bg_statusbar
}
pub fn bg_sidebar() -> Color32 {
    read().bg_sidebar
}
pub fn bg_editor() -> Color32 {
    read().bg_editor
}
// Opaque surfaces layered on top of the window background:
pub fn bg_code() -> Color32 {
    read().bg_code
}
pub fn bg_quote() -> Color32 {
    read().bg_quote
}
pub fn bg_raised() -> Color32 {
    read().bg_raised
}

// ── Text ─────────────────────────────────────────────────────────────
pub fn text_primary() -> Color32 {
    read().text_primary
}
pub fn text_dim() -> Color32 {
    read().text_dim
}
pub fn text_strong() -> Color32 {
    read().text_strong
}
pub fn text_editor() -> Color32 {
    read().text_editor
}
pub fn text_code() -> Color32 {
    read().text_code
}

// ── Accents ──────────────────────────────────────────────────────────
pub fn accent() -> Color32 {
    read().accent
}
pub fn accent_alt() -> Color32 {
    read().accent_alt
}
pub fn accent_yellow() -> Color32 {
    read().accent_yellow
}
pub fn accent_red() -> Color32 {
    read().accent_red
}
pub fn accent_green() -> Color32 {
    read().accent_green
}
pub fn accent_purple() -> Color32 {
    read().accent_purple
}
pub fn quote_bar() -> Color32 {
    read().quote_bar
}
pub fn badge_bg() -> Color32 {
    read().badge_bg
}
pub fn badge_text() -> Color32 {
    read().badge_text
}

// ── Fonts ────────────────────────────────────────────────────────────

/// Selectable font families: (config value, display label, system path).
/// An empty config value keeps egui's built-in fonts (Hack + Ubuntu).
/// Only plain .ttf system fonts are offered (no .ttc collections).
pub const FONT_CHOICES: &[(&str, &str, &str)] = &[
    ("", "Built-in (Hack)", ""),
    ("monaco", "Monaco", "/System/Library/Fonts/Monaco.ttf"),
    (
        "courier",
        "Courier New",
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
    ),
    (
        "arial",
        "Arial",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ),
    (
        "georgia",
        "Georgia",
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
    ),
    (
        "verdana",
        "Verdana",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
    ),
    (
        "times",
        "Times New Roman",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
    ),
];

/// Display label for a font config value.
pub fn font_label(choice: &str) -> &'static str {
    FONT_CHOICES
        .iter()
        .find(|(value, _, _)| *value == choice)
        .map(|(_, label, _)| *label)
        .unwrap_or("Built-in (Hack)")
}

/// Apply the configured font family (live). Loads the chosen system font
/// and puts it first for both proportional and monospace text; unknown
/// values or unreadable files fall back to the built-in fonts.
pub fn apply_font(ctx: &egui::Context, choice: &str) {
    let mut fonts = egui::FontDefinitions::default();
    let path = FONT_CHOICES
        .iter()
        .find(|(value, _, _)| *value == choice && !value.is_empty())
        .map(|(_, _, path)| *path);
    if let Some(path) = path {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    "user_font".to_string(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, "user_font".to_string());
                }
            }
            Err(e) => eprintln!("warning: could not load font {path}: {e}"),
        }
    }
    ctx.set_fonts(fonts);
}

/// Apply the interface scale (live).
///
/// egui's zoom factor is used rather than rescaling `text_styles`: almost
/// every label in this app sets an explicit point size, which text_styles
/// would not touch, so scaling them would appear to do nothing. The zoom
/// factor multiplies the whole layout — text, spacing and widgets alike —
/// which is what "make it bigger" actually means to a reader.
///
/// The value is clamped so a bad config can't scale the settings window
/// itself out of reach; `Config::load` clamps too, and this is the
/// belt-and-braces for values set at runtime.
pub fn apply_font_scale(ctx: &egui::Context, scale: f32) {
    let scale = if scale.is_finite() {
        scale.clamp(crate::config::MIN_FONT_SCALE, crate::config::MAX_FONT_SCALE)
    } else {
        1.0
    };
    if (ctx.zoom_factor() - scale).abs() > f32::EPSILON {
        ctx.set_zoom_factor(scale);
    }
}

// ── Painted flourishes ───────────────────────────────────────────────

/// Paint a horizontal accent→accent_alt gradient bar (the app's identity
/// stripe, matching the icon's cyan→violet).
pub fn paint_accent_gradient(painter: &egui::Painter, rect: egui::Rect) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let (c1, c2) = (accent(), accent_alt());
    let mut mesh = Mesh::default();
    let v = |pos: egui::Pos2, color: Color32| Vertex {
        pos,
        uv: WHITE_UV,
        color,
    };
    let i = mesh.vertices.len() as u32;
    mesh.vertices.push(v(rect.left_top(), c1));
    mesh.vertices.push(v(rect.right_top(), c2));
    mesh.vertices.push(v(rect.right_bottom(), c2));
    mesh.vertices.push(v(rect.left_bottom(), c1));
    mesh.indices
        .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    painter.add(mesh);
}

/// Build a LayoutJob rendering `text` with a per-character accent→accent_alt
/// gradient (used for the wordmark on the picker screen).
pub fn gradient_text(text: &str, size: f32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(1) as f32;
    let (c1, c2) = (accent(), accent_alt());
    for (idx, ch) in chars.iter().enumerate() {
        let f = idx as f32 / (n - 1.0).max(1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
        let color = Color32::from_rgb(
            lerp(c1.r(), c2.r()),
            lerp(c1.g(), c2.g()),
            lerp(c1.b(), c2.b()),
        );
        job.append(
            &ch.to_string(),
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(size),
                color,
                ..Default::default()
            },
        );
    }
    job
}

// ── Layout metrics (compile-time constants) ──────────────────────────
pub const FONT_SIZE: f32 = 15.0;
pub const FONT_SIZE_STATUS: f32 = 12.0;
/// Heading font sizes for h1..h6.
pub const HEADING_SIZES: [f32; 6] = [26.0, 22.0, 19.0, 17.0, 16.0, 15.0];
pub const MD_INDENT: f32 = 20.0;
pub const STATUSBAR_HEIGHT: f32 = 28.0;
pub const STATUSBAR_HEIGHT_EDIT: f32 = 36.0;
pub const CONTENT_PADDING: f32 = 16.0;
pub const LINE_SPACING: f32 = 4.0;
/// Corner radius for buttons/cards/windows.
pub const RADIUS: f32 = 8.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#FF7900"), Some(Color32::from_rgb(255, 121, 0)));
        assert_eq!(parse_hex("22d3ee"), Some(Color32::from_rgb(34, 211, 238)));
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn contrast_picks_dark_text_on_bright_accent() {
        let dark = contrast_text_for(Color32::from_rgb(125, 249, 255));
        assert!(dark.r() < 100, "bright accent should get dark text");
        let light = contrast_text_for(Color32::from_rgb(40, 20, 60));
        assert!(light.r() > 200, "dark accent should get light text");
    }

    #[test]
    fn presets_exist_and_default_falls_back() {
        for name in [
            "slate", "midnight", "plum", "forest", "paper", "linen", "nonsense",
        ] {
            let p = preset(name);
            // Backgrounds are always fully opaque.
            assert_eq!(p.bg_primary.a(), 255);
        }
    }
}

#[cfg(test)]
mod palettes {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f32 {
        let ch = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    /// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Every listed preset must actually be implemented.
    ///
    /// A name in `PRESETS` that `preset` has no arm for falls through to
    /// the default, so picking it in Settings silently does nothing —
    /// the kind of typo that is invisible until someone tries that one
    /// theme.
    #[test]
    fn every_listed_preset_is_distinct() {
        let slate = preset("slate");
        for name in PRESETS {
            if *name == "slate" {
                continue;
            }
            assert!(
                preset(name) != slate,
                "{name:?} is listed in PRESETS but falls through to the                  default palette — it has no match arm, or is an exact                  copy of slate"
            );
        }
        // And no two presets are the same palette under different names.
        for (i, a) in PRESETS.iter().enumerate() {
            for b in &PRESETS[i + 1..] {
                assert!(preset(a) != preset(b), "{a:?} and {b:?} are identical");
            }
        }
    }

    /// An unknown name must land on the default rather than panic — old
    /// configs, hand-edits, and themes removed in a later version all
    /// reach this path.
    #[test]
    fn unknown_names_fall_back_to_slate() {
        assert!(preset("no-such-theme") == preset("slate"));
        assert!(preset("") == preset("slate"));
    }

    /// Text has to be readable on the surface it sits on.
    ///
    /// Checked rather than eyeballed because a palette is 22 hand-picked
    /// colors and it is easy to ship one that looks fine in the picker
    /// and is unreadable in the editor. 4.5:1 is the WCAG AA threshold
    /// for body text; dimmed and decorative text gets the 3.0:1 large-text
    /// allowance.
    #[test]
    fn every_palette_is_readable() {
        for name in PRESETS {
            let p = preset(name);
            let pairs: [(&str, Color32, Color32, f32); 6] = [
                (
                    "text_primary on bg_primary",
                    p.text_primary,
                    p.bg_primary,
                    4.5,
                ),
                (
                    "text_strong on bg_primary",
                    p.text_strong,
                    p.bg_primary,
                    4.5,
                ),
                ("text_editor on bg_editor", p.text_editor, p.bg_editor, 4.5),
                ("text_code on bg_code", p.text_code, p.bg_code, 4.5),
                ("text_dim on bg_primary", p.text_dim, p.bg_primary, 3.0),
                ("badge_text on badge_bg", p.badge_text, p.badge_bg, 3.0),
            ];
            for (what, fg, bg, min) in pairs {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= min,
                    "theme {name:?}: {what} is {ratio:.2}:1, below the                      {min:.1}:1 minimum — pick a lighter or darker color"
                );
            }
        }
    }

    /// The statusbar and raised surfaces must be visibly separate from
    /// the main background, or the chrome dissolves into the page.
    #[test]
    fn surfaces_are_distinguishable() {
        for name in PRESETS {
            let p = preset(name);
            for (what, other) in [("bg_statusbar", p.bg_statusbar), ("bg_raised", p.bg_raised)] {
                assert!(
                    other != p.bg_primary,
                    "theme {name:?}: {what} is identical to bg_primary"
                );
            }
        }
    }

    /// `DEFAULT` backs the palette lock before `init` runs, so a drift
    /// from "slate" would show as a one-frame flash of another theme.
    #[test]
    fn const_default_matches_the_slate_preset() {
        assert!(DEFAULT == preset("slate"));
    }
}

#[cfg(test)]
mod glyph_coverage {

    /// Pull every character that egui might have to *draw* out of a Rust
    /// source file: the contents of string literals, with `\u{…}`
    /// escapes decoded, skipping `//` comments.
    ///
    /// Deliberately naive — there are no raw strings in the UI code, and
    /// a state machine that handles `\"`, `\\` and multi-line literals
    /// covers everything that is actually written here. If raw strings
    /// ever appear this under-reports, which is a missed warning rather
    /// than a false alarm.
    fn literal_chars(src: &str) -> Vec<(char, usize)> {
        let b: Vec<char> = src.chars().collect();
        let (mut out, mut i, mut line) = (Vec::new(), 0usize, 1usize);
        let (mut in_str, mut in_comment) = (false, false);
        while i < b.len() {
            let c = b[i];
            if c == '\n' {
                line += 1;
                in_comment = false;
                i += 1;
                continue;
            }
            if in_comment {
                i += 1;
                continue;
            }
            if !in_str && c == '/' && b.get(i + 1) == Some(&'/') {
                in_comment = true;
                i += 2;
                continue;
            }
            if !in_str && c == '"' {
                in_str = true;
                i += 1;
                continue;
            }
            if in_str {
                if c == '\\' {
                    // `\u{XXXX}` names a character the user will see.
                    if b.get(i + 1) == Some(&'u') && b.get(i + 2) == Some(&'{') {
                        let mut j = i + 3;
                        let mut hex = String::new();
                        while j < b.len() && b[j] != '}' {
                            hex.push(b[j]);
                            j += 1;
                        }
                        if let Some(ch) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            out.push((ch, line));
                        }
                        i = j + 1;
                        continue;
                    }
                    i += 2; // any other escape is ASCII
                    continue;
                }
                if c == '"' {
                    in_str = false;
                    i += 1;
                    continue;
                }
                if !c.is_ascii() {
                    out.push((c, line));
                }
            }
            i += 1;
        }
        out
    }

    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    /// Scan the code that actually draws, and prove every character in
    /// it has a glyph.
    ///
    /// This exists because the hand-written list below did not catch a
    /// real regression: `→` was added to three dialogs while the list
    /// sat there unchanged, and the app shipped boxes. A list of symbols
    /// somebody has to remember to update is not a check.
    ///
    /// Scope is `src/ui/` plus `src/app.rs` — where egui strings are
    /// authored. The agent modules print to a terminal and write files,
    /// where the font is not ours to worry about, so `→` is fine there
    /// and stays.
    #[test]
    fn no_ui_string_can_render_as_a_box() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![root.join("src/app.rs")];
        rs_files(&root.join("src/ui"), &mut files);
        files.sort();

        let fonts = egui::epaint::text::Fonts::new(1.0, 2048, egui::FontDefinitions::default());
        let mut bad = Vec::new();

        for path in &files {
            let Ok(src) = std::fs::read_to_string(path) else {
                continue;
            };
            for (c, line) in literal_chars(&src) {
                // Required in *both* fonts: a string's font is chosen at
                // the call site and cannot be known from here, so the
                // strict rule is the only sound one.
                for id in [
                    egui::FontId::proportional(14.0),
                    egui::FontId::monospace(14.0),
                ] {
                    if !fonts.has_glyph(&id, c) {
                        let rel = path.strip_prefix(root).unwrap_or(path);
                        bad.push(format!(
                            "{}:{line}  U+{:04X} {c}  missing from {:?}",
                            rel.display(),
                            c as u32,
                            id.family
                        ));
                    }
                }
            }
        }
        bad.dedup();
        assert!(
            bad.is_empty(),
            "these characters would render as tofu boxes:\n  {}\n\n\
             Pick one the bundled fonts have. Known-good substitutes: \
             \u{203A} or \u{00BB} in place of an arrow (U+2192 is absent \
             from the proportional font), \u{2714} in place of U+2713, \
             \u{00B7} as a separator.",
            bad.join("\n  ")
        );
    }

    /// A short explicit list, kept alongside the scan above.
    ///
    /// The scan covers `src/ui/` and `src/app.rs`, where egui strings are
    /// written. This covers the rest: characters that reach the screen
    /// indirectly, formatted into a toast or label from an error raised
    /// in `config`, `crypto` or `document`.
    ///
    /// Absent from the bundled fonts — do not reintroduce into anything
    /// drawn: ⌃ U+2303, ⌥ U+2325, ⇧ U+21E7, ⇥ U+21E5, ▸ U+25B8,
    /// ✎ U+270E, ✕ U+2715, U+2713 (use ✔ U+2714), and → U+2192, which
    /// exists in the monospace font but not the proportional one — use
    /// › U+203A or » U+00BB for an arrow in a label.
    #[test]
    fn every_symbol_the_ui_uses_has_a_glyph() {
        let fonts = egui::epaint::text::Fonts::new(1.0, 2048, egui::FontDefinitions::default());
        // Every non-ASCII char used in a user-visible string.
        let used = "\u{2318}\u{2014}\u{B7}\u{26A0}\u{2026}\u{201C}\u{201D}\u{2022}\
                    \u{1F511}\u{2714}\u{2611}\u{2610}\u{21BB}\u{2197}\u{1F5C0}\
                    \u{1F512}\u{1F4DD}\u{1F4C4}\u{00A9}\u{21A9}\u{2605}\u{2630}\u{23F8}";
        for id in [
            egui::FontId::proportional(14.0),
            egui::FontId::monospace(14.0),
        ] {
            for c in used.chars() {
                assert!(
                    fonts.has_glyph(&id, c),
                    "U+{:04X} ({c}) has no glyph in {id:?} — it would render as a \
                     tofu box. Pick a covered character.",
                    c as u32
                );
            }
        }
    }
}
