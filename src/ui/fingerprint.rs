//! Visual file fingerprints: a hash you can recognise without reading.
//!
//! Eight hex digits are perfectly precise and almost useless to a human
//! eye. Nobody remembers `dfdc256a`, so nobody notices when it becomes
//! `9e805359` — and noticing is the whole point. The file in front of
//! you should look like the file you have been working in for a month,
//! and look *wrong* the moment it isn't.
//!
//! So the digest is drawn as well as printed: a small **circuit**. Six
//! nodes are placed by digest bytes and joined in sequence by
//! right-angle traces, the way tracks are routed on a board; each node
//! is either a square pad or a round star, decided by its own byte, and
//! every element takes its hue from the byte that placed it.
//!
//! The identity lives in three channels at once — where the nodes sit,
//! which of them are square, and what colour everything is. The first
//! two survive with all colour removed, so the mark still works for the
//! roughly one man in twelve with a colour-vision deficiency, and on a
//! washed-out projector.
//!
//! The background is transparent: the glyph sits directly on whatever
//! surface it is drawn over, which is what lets one design serve the
//! status bar, the picker cards, and any of the sixteen themes.
//!
//! **Why OKLCH and not raw RGB bytes.** The obvious implementation —
//! `Color32::from_rgb(d[0], d[1], d[2])` — produces muddy olive next to
//! searing magenta, some of it invisible on `paper` and some of it
//! painful on `abyss`. Here only the *hue* comes from the digest, while
//! lightness and chroma are fixed per theme family, so every colour is
//! equally vivid and equally readable. A test checks the contrast holds
//! for all 256 hues against the extremes of both theme families.
//!
//! SECURITY: this draws the hash of the **ciphertext** on disk. Nothing
//! here touches plaintext, and every byte it renders is something any
//! observer of the encrypted file could compute themselves.

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use super::theme;

/// Nodes in the circuit. Six is enough to make a distinctive route and
/// few enough that the figure never turns to noise at 13 px.
const NODES: usize = 6;

/// A file's on-disk identity, in a form that can be drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    digest: [u8; 32],
}

impl Fingerprint {
    pub fn new(digest: [u8; 32]) -> Self {
        Self { digest }
    }

    /// Parse a full 64-character hex digest. `None` if it isn't one.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut digest = [0u8; 32];
        for (i, out) in digest.iter_mut().enumerate() {
            *out = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Self { digest })
    }

    /// The full digest as lowercase hex — what the tooltip shows, and
    /// the only form anyone should compare byte for byte.
    pub fn hex(&self) -> String {
        self.digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The first 8 hex digits, matching the badge text.
    pub fn short_hex(&self) -> String {
        self.digest[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Three words that name this file out loud.
    ///
    /// A memory aid and a way to check a file against someone on the
    /// phone — *not* a comparison of record. Three 32-word lists are
    /// 32768 combinations, which is plenty to notice a change and
    /// nowhere near enough to prove two files identical. The full hex
    /// is right there for that.
    pub fn mnemonic(&self) -> String {
        let d = &self.digest;
        format!(
            "{}-{}-{}",
            ADJECTIVES[(d[27] & 31) as usize],
            NOUNS[(d[28] & 31) as usize],
            VERBS[(d[29] & 31) as usize],
        )
    }
}

// ── Colour ───────────────────────────────────────────────────────────

/// An OKLCH colour, converted to sRGB and clamped into gamut.
///
/// OKLCH because it is perceptually uniform: a fixed `l` and `c` across
/// every hue really does look equally bright and equally saturated,
/// which is exactly the property that keeps 256 generated colours from
/// including a few unreadable ones.
fn oklch(l: f32, c: f32, hue_deg: f32) -> Color32 {
    let h = hue_deg.to_radians();
    let (a, b) = (c * h.cos(), c * h.sin());

    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    let enc = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    Color32::from_rgb(enc(lin[0]), enc(lin[1]), enc(lin[2]))
}

fn hue_of(byte: u8) -> f32 {
    byte as f32 / 256.0 * 360.0
}

// The two (lightness, chroma) pairs the whole feature rests on. Named
// constants rather than inline values so the contrast test checks
// *these* — a test that restated them would pass happily while the
// drawing code went unreadable. With no plate behind the glyph any
// more, the ink must clear the theme backgrounds themselves: bright
// enough for the darkest dark theme, dark enough for paper-white.
const DARK_INK: (f32, f32) = (0.66, 0.175);
const LIGHT_INK: (f32, f32) = (0.52, 0.17);

/// The colour of a node or trace. `light` is the theme family; it is a
/// parameter rather than a read of theme state so the colour is a pure
/// function of its inputs — which is also what makes it testable.
fn ink(byte: u8, light: bool) -> Color32 {
    let (l, c) = if light { LIGHT_INK } else { DARK_INK };
    oklch(l, c, hue_of(byte))
}

// ── Geometry ─────────────────────────────────────────────────────────

/// Layout for a fingerprint drawn at `height` points. The design is
/// authored on a 22×13 grid; everything scales from there.
struct Layout {
    /// Points per grid unit.
    s: f32,
    /// Circle-node radius, square-pad half-side, and trace width — all
    /// floored so nothing vanishes at a small interface scale.
    star_r: f32,
    pad_half: f32,
    trace_w: f32,
    size: Vec2,
}

fn layout(height: f32) -> Layout {
    // A floor, not just a clamp: at a tiny interface scale a stroke that
    // rounds to zero would draw nothing, and the fingerprint would
    // silently become empty space.
    let height = height.max(8.0);
    let s = height / 13.0;
    Layout {
        s,
        star_r: (1.5 * s).max(1.0),
        pad_half: (1.4 * s).max(1.0),
        trace_w: (1.0 * s).max(0.8),
        size: Vec2::new(22.0 * s, height),
    }
}

/// The space a fingerprint occupies at a given height.
pub fn size_for(height: f32) -> Vec2 {
    layout(height).size
}

/// Default height, used in the status bar and the picker cards.
///
/// Taller than the text beside it on purpose. Sized to the font
/// (`FONT_SIZE_STATUS + 2`) the mark disappeared on small or distant
/// screens — a fingerprint nobody can see identifies nothing. 18pt
/// still fits the status bar inside its margins.
pub fn default_height() -> f32 {
    theme::FONT_SIZE_STATUS + 6.0
}

/// Where node `i` of the circuit sits inside `rect`.
///
/// X from one byte, Y from another, quantised to a coarse grid — 9×7
/// positions — so nodes land in visibly *different* places rather than
/// in subtly different ones. A one-pixel nudge is invisible; a grid
/// step is not, and being noticed is the job.
fn node_pos(d: &[u8; 32], i: usize, rect: Rect, s: f32) -> Pos2 {
    let m = 2.5 * s;
    egui::pos2(
        rect.left() + m + (d[i] % 9) as f32 / 8.0 * (rect.width() - 2.0 * m),
        rect.top() + m + (d[i + NODES] % 7) as f32 / 6.0 * (rect.height() - 2.0 * m),
    )
}

// ── Painting ─────────────────────────────────────────────────────────

/// Draw a fingerprint and return its `Response`, so the caller can hang
/// a tooltip on it.
pub fn paint(ui: &mut Ui, fp: &Fingerprint, height: f32) -> Response {
    let l = layout(height);
    let (rect, response) = ui.allocate_exact_size(l.size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let p = ui.painter();
    let d = &fp.digest;
    let light = theme::is_light();

    // Traces first, so the nodes sit on top of them. Each trace runs
    // horizontal then vertical — Manhattan routing, the elbow being
    // what makes it read as an etched track rather than a scribble.
    for i in 0..NODES - 1 {
        let a = node_pos(d, i, rect, l.s);
        let b = node_pos(d, i + 1, rect, l.s);
        let elbow = egui::pos2(b.x, a.y);
        // Dimmed against the nodes so the route reads as wiring, not as
        // five more marks competing with the pads for attention.
        let stroke = Stroke::new(l.trace_w, ink(d[12 + i], light).gamma_multiply(0.7));
        p.line_segment([a, elbow], stroke);
        p.line_segment([elbow, b], stroke);
    }

    // Nodes: the low bit of the placing byte picks the shape, so shape
    // is exactly as digest-determined as position and colour.
    for i in 0..NODES {
        let at = node_pos(d, i, rect, l.s);
        let color = ink(d[i], light);
        if d[i] & 1 == 1 {
            p.rect_filled(
                Rect::from_center_size(at, Vec2::splat(l.pad_half * 2.0)),
                0.5 * l.s,
                color,
            );
        } else {
            p.circle_filled(at, l.star_r, color);
        }
    }

    response
}

// ── Explanation ──────────────────────────────────────────────────────

/// The hover text: what this is, and what it tells you.
///
/// `previous` is the last digest seen for this file, when one was
/// recorded and it differs — the case worth interrupting someone for.
pub fn tooltip(fp: &Fingerprint, previous: Option<&Fingerprint>) -> String {
    let mut s = format!(
        "File fingerprint\n{}\n\nSpoken name: {}\n\n\
         SHA-256 of the encrypted file on disk. The picture is drawn from it, \
         so any change to the file redraws it completely — if it looks the way \
         it always has, the file is byte for byte what you last saved.",
        fp.hex(),
        fp.mnemonic(),
    );
    if let Some(old) = previous {
        s.push_str(&format!(
            "\n\n\u{26A0} Changed since you last opened this.\n\
             It was {} ({}).\n\
             That is expected if you saved it, or something else wrote to it — \
             and worth a look if neither is true.",
            old.short_hex(),
            old.mnemonic(),
        ));
    }
    s
}

// ── Wordlists ────────────────────────────────────────────────────────
//
// FROZEN. These name every file a user has ever looked at; reordering a
// list or swapping a word silently renames all of them, and the name
// someone half-remembers stops matching. Treat this exactly like the AGE
// salt: append never, edit never. A test pins the outputs.

const ADJECTIVES: [&str; 32] = [
    "amber", "azure", "brass", "coral", "cobalt", "crimson", "dusty", "ember", "frost", "golden",
    "hazel", "indigo", "ivory", "jade", "lilac", "maroon", "misty", "olive", "onyx", "opal",
    "pearl", "plum", "quiet", "russet", "saffron", "sage", "sepia", "silver", "slate", "teal",
    "umber", "violet",
];

const NOUNS: [&str; 32] = [
    "anchor", "arbor", "beacon", "bramble", "cedar", "comet", "cove", "dune", "elm", "falcon",
    "fjord", "harbor", "heron", "lantern", "ledger", "meadow", "mesa", "otter", "quarry", "quartz",
    "raven", "ridge", "sable", "sparrow", "thistle", "thorn", "tide", "vault", "willow", "warren",
    "yarrow", "zephyr",
];

const VERBS: [&str; 32] = [
    "banks", "bends", "carries", "climbs", "crests", "drifts", "echoes", "fades", "folds",
    "gathers", "glides", "gleams", "hushes", "kindles", "lands", "leans", "lingers", "listens",
    "opens", "rests", "rises", "roams", "settles", "shifts", "sparks", "steadies", "surges",
    "turns", "wakes", "wanders", "weaves", "winds",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the digest the drawing actually reads: six node X
    /// positions (whose low bits also pick the shapes), six node Y
    /// positions, and five trace colours. The mnemonic reads three more
    /// at 27–29; the rest is headroom.
    const DRAWN: usize = 17;

    fn fp(seed: u8) -> Fingerprint {
        let mut d = [0u8; 32];
        for (i, b) in d.iter_mut().enumerate() {
            *b = seed.wrapping_mul(31).wrapping_add(i as u8).wrapping_mul(7);
        }
        Fingerprint::new(d)
    }

    /// Everything drawn must be a pure function of the digest, or the
    /// same file would look different between two frames.
    #[test]
    fn the_same_digest_always_yields_the_same_visual() {
        let a = fp(9);
        let b = Fingerprint::new(a.digest);
        assert_eq!(a.hex(), b.hex());
        assert_eq!(a.mnemonic(), b.mnemonic());
        for i in 0..=255u8 {
            assert_eq!(ink(i, false), ink(i, false));
            assert_eq!(ink(i, true), ink(i, true));
        }
    }

    /// The point of the feature: a file that changed must not still look
    /// familiar. One flipped bit has to move most of the drawn bytes.
    #[test]
    fn one_flipped_bit_redraws_the_whole_fingerprint() {
        // Real digests: README.md, and README.md with one byte appended.
        let a = Fingerprint::from_hex(
            "dfdc256a5219be4354e6f3c63e18a9c235a0f6fc3648288efcb04009c808f2a1",
        )
        .unwrap();
        let b = Fingerprint::from_hex(
            "9e805359ea4af972ec1a5ac8d55c73e475d13fdd504e1ec719415e9f9063b59b",
        )
        .unwrap();

        let differing = (0..DRAWN).filter(|&i| a.digest[i] != b.digest[i]).count();
        assert!(
            differing >= DRAWN * 3 / 4,
            "only {differing} of the {DRAWN} drawn bytes differ — the two \
             files would still look alike"
        );
        assert_ne!(a.mnemonic(), b.mnemonic());
    }

    /// With no plate behind it any more, the ink must clear the theme
    /// backgrounds themselves — every hue, against the worst case of
    /// each family. This is the check that stops a generated palette
    /// from shipping an invisible colour.
    #[test]
    fn every_hue_clears_both_theme_families() {
        fn luminance(c: Color32) -> f32 {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        fn ratio(a: Color32, b: Color32) -> f32 {
            let (x, y) = (luminance(a), luminance(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        }

        // Worst cases, chosen adversarially rather than typically: the
        // *brightest* surface a dark theme plausibly puts under the
        // glyph (a raised card), and pure white for the light family —
        // brighter than any light preset actually is.
        let dark_worst = Color32::from_rgb(50, 56, 70);
        let light_worst = Color32::WHITE;

        for byte in 0..=255u8 {
            let h = hue_of(byte);
            let dark = ratio(ink(byte, false), dark_worst);
            assert!(
                dark >= 3.0,
                "hue {h:.0}° at DARK_INK is {dark:.2}:1 against a raised \
                 dark surface — an invisible node is not a fingerprint"
            );
            let light = ratio(ink(byte, true), light_worst);
            assert!(
                light >= 3.0,
                "hue {h:.0}° at LIGHT_INK is {light:.2}:1 against white"
            );
        }
    }

    /// A fingerprint must still be a fingerprint at a small interface
    /// scale — never a zero-width nothing — and every node must land
    /// inside the rect whatever the digest says.
    #[test]
    fn geometry_survives_being_shrunk_and_stays_in_bounds() {
        for h in [4.0, 8.0, 11.0, 13.0, 24.0, 64.0] {
            let l = layout(h);
            assert!(l.star_r >= 1.0, "stars vanished at height {h}");
            assert!(l.pad_half >= 1.0, "pads vanished at height {h}");
            assert!(l.trace_w >= 0.8, "traces vanished at height {h}");
            assert!(l.size.x > l.size.y, "fingerprint should be wider than tall");

            // All 9×7 grid positions stay inside the rect, with room
            // for the node drawn on top of them.
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), l.size);
            let mut d = [0u8; 32];
            for x in 0..9u8 {
                for y in 0..7u8 {
                    d[0] = x; // % 9 == x
                    d[NODES] = y; // % 7 == y
                    let p = node_pos(&d, 0, rect, l.s);
                    let margin = l.pad_half.max(l.star_r);
                    assert!(
                        rect.expand(-margin + 0.51).contains(p),
                        "node at grid ({x},{y}) leaves the rect at height {h}"
                    );
                }
            }
        }
    }

    /// The wordlists name every file the user has ever seen. Changing
    /// one renames all of them, so the outputs are pinned.
    #[test]
    fn the_wordlists_are_frozen() {
        assert_eq!(ADJECTIVES.len(), 32);
        assert_eq!(NOUNS.len(), 32);
        assert_eq!(VERBS.len(), 32);
        // Across all three lists, not just within each: a word in two
        // lists lets a name repeat itself ("ember-ember-…"), which reads
        // as a bug in the generator rather than as a name.
        let mut all: Vec<&str> = ADJECTIVES
            .iter()
            .chain(&NOUNS)
            .chain(&VERBS)
            .copied()
            .collect();
        all.sort_unstable();
        let before = all.len();
        all.dedup();
        assert_eq!(before, all.len(), "a word appears in more than one list");

        // Pinned against a hand-worked derivation, not against whatever
        // the code happens to do: for this digest bytes 27/28/29 are
        // 0x09, 0xc8, 0x08, so the indices are 9, 8 and 8.
        let known = Fingerprint::from_hex(
            "dfdc256a5219be4354e6f3c63e18a9c235a0f6fc3648288efcb04009c808f2a1",
        )
        .unwrap();
        assert_eq!(known.digest[27], 0x09);
        assert_eq!(known.digest[28], 0xc8);
        assert_eq!(known.digest[29], 0x08);
        assert_eq!(
            known.mnemonic(),
            format!("{}-{}-{}", ADJECTIVES[9], NOUNS[8], VERBS[8])
        );
        assert_eq!(known.mnemonic(), "golden-elm-folds");
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let h = "dfdc256a5219be4354e6f3c63e18a9c235a0f6fc3648288efcb04009c808f2a1";
        assert_eq!(Fingerprint::from_hex(h).unwrap().hex(), h);
        assert_eq!(Fingerprint::from_hex(h).unwrap().short_hex(), "dfdc256a");
        assert!(Fingerprint::from_hex("").is_none());
        assert!(Fingerprint::from_hex("dfdc256a").is_none(), "too short");
        assert!(
            Fingerprint::from_hex(&"z".repeat(64)).is_none(),
            "non-hex must be refused, not silently zeroed"
        );
    }

    /// The tooltip has to explain itself to someone who has never heard
    /// of this feature, and say plainly when something changed.
    #[test]
    fn the_tooltip_explains_itself() {
        let a = fp(1);
        let plain = tooltip(&a, None);
        assert!(plain.contains(&a.hex()), "shows the full hash");
        assert!(plain.contains(&a.mnemonic()), "shows the spoken name");
        assert!(
            plain.contains("encrypted file on disk"),
            "says what it is of"
        );
        assert!(
            !plain.contains("Changed since"),
            "no alarm without a change"
        );

        let b = fp(2);
        let changed = tooltip(&b, Some(&a));
        assert!(changed.contains("Changed since you last opened this"));
        assert!(changed.contains(&a.short_hex()), "shows what it was before");
    }
}
