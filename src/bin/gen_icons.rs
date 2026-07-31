//! Regenerates every icon asset procedurally, so the logo has a single
//! source of truth in code and re-rendering any size is one command:
//!
//! ```sh
//! cargo run --bin gen_icons
//! iconutil -c icns assets/schl8.iconset -o assets/schl8.icns
//! ```
//!
//! The mark: a folded-corner note page with a keyhole punched through it
//! ("a secured note"), in the app's cyan→violet identity gradient, on a
//! deep neutral-slate squircle that sits well beside any of the theme
//! palettes, dark or light. `tray_glyph.png` is the same mark as a
//! single-tone template (black + alpha) for the macOS menu bar, which
//! recolors template images itself for light/dark menu bars.

use image::{Rgba, RgbaImage};

const CYAN: [f32; 3] = [34.0, 211.0, 238.0];
const VIOLET: [f32; 3] = [167.0, 139.0, 250.0];
const BG_TOP: [f32; 3] = [30.0, 38.0, 52.0];
const BG_BOTTOM: [f32; 3] = [10.0, 13.0, 19.0];

/// Supersampling grid per pixel edge (4 → 16 samples per pixel).
const SS: u32 = 4;

/// Everything in unit coordinates (0..1 across the canvas).
struct Mark {
    /// Page center / half-extents / corner radius.
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    r: f32,
    /// Folded-corner leg length.
    fold: f32,
    /// Peak horizontal displacement of the S-curve spine. The page's
    /// vertical edges both shift by the same amount at a given y, so the
    /// silhouette stays a constant-width slab that snakes like a very
    /// fat "S".
    swing: f32,
    /// Keyhole circle center-y, radius, stem half-width, stem length.
    key_cy: f32,
    key_r: f32,
    stem_hw: f32,
    stem_len: f32,
}

const APP_MARK: Mark = Mark {
    cx: 0.5,
    cy: 0.52,
    hw: 0.225,
    hh: 0.285,
    r: 0.05,
    fold: 0.145,
    swing: 0.055,
    key_cy: 0.565,
    key_r: 0.052,
    stem_hw: 0.020,
    stem_len: 0.105,
};

/// Larger proportions for the tiny menu-bar glyph.
const TRAY_MARK: Mark = Mark {
    cx: 0.5,
    cy: 0.5,
    hw: 0.30,
    hh: 0.40,
    r: 0.07,
    fold: 0.20,
    swing: 0.070,
    key_cy: 0.565,
    key_r: 0.085,
    stem_hw: 0.034,
    stem_len: 0.16,
};

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Horizontal displacement of the S-curve spine at height `y`.
///
/// `t(1 - t²)²` over t in [-1, 1] is odd (so the two halves bow opposite
/// ways — an S, not a bow) and has BOTH value and slope zero at t = ±1, so
/// the curved sides meet the flat top and bottom edges tangentially
/// instead of kinking into them. Scaled so its peak is exactly `swing`.
fn spine(m: &Mark, y: f32) -> f32 {
    const PEAK: f32 = 0.286_221_2; // max of t(1-t²)² on [0,1], at t = 1/√5
    let t = ((y - m.cy) / m.hh).clamp(-1.0, 1.0);
    let f = t * (1.0 - t * t).powi(2) / PEAK;
    m.swing * f
}

/// Signed test: inside a rounded rectangle?
fn in_round_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> bool {
    let dx = (x - cx).abs() - (hw - r);
    let dy = (y - cy).abs() - (hh - r);
    let dx = dx.max(0.0);
    let dy = dy.max(0.0);
    dx * dx + dy * dy <= r * r
}

/// Which part of the mark (if any) a point falls in.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    None,
    Page,
    Flap,
}

fn mark_part(m: &Mark, x: f32, y: f32) -> Part {
    // The spine bends the page (and everything punched through it) left
    // and right as it rises, turning the slab into a fat "S".
    let cx = m.cx + spine(m, y);
    if !in_round_rect(x, y, cx, m.cy, m.hw, m.hh, m.r) {
        return Part::None;
    }
    // Keyhole: circle + stem capsule, punched out entirely. It rides the
    // spine at ONE height (its own centre) rather than per-scanline —
    // bending the keyhole itself just reads as a smudge at icon sizes.
    let key_cx = m.cx + spine(m, m.key_cy);
    let kx = x - key_cx;
    let ky = y - m.key_cy;
    if kx * kx + ky * ky <= m.key_r * m.key_r {
        return Part::None;
    }
    let stem_top = m.key_cy;
    let stem_bot = m.key_cy + m.stem_len;
    if kx.abs() <= m.stem_hw && y >= stem_top && y <= stem_bot {
        return Part::None;
    }
    let sy = y - stem_bot;
    if kx * kx + sy * sy <= m.stem_hw * m.stem_hw {
        return Part::None;
    }

    // Folded top-right corner: the outer triangle is cut away; the
    // mirrored inner triangle is the folded-down flap.
    let xr = m.cx + spine(m, m.cy - m.hh) + m.hw;
    let yt = m.cy - m.hh;
    let u = x - (xr - m.fold);
    let v = y - yt;
    if u > 0.0 && v < m.fold {
        if u > v {
            return Part::None; // cut corner
        }
        if v <= m.fold && u >= 0.0 {
            return Part::Flap;
        }
    }
    Part::Page
}

/// Gradient color of the mark at a point (cyan → violet across the page
/// diagonal).
fn mark_color(m: &Mark, x: f32, y: f32) -> [f32; 3] {
    let tx = (x - (m.cx + spine(m, y) - m.hw)) / (2.0 * m.hw);
    let ty = (y - (m.cy - m.hh)) / (2.0 * m.hh);
    let t = ((tx + ty) / 2.0).clamp(0.0, 1.0);
    lerp3(CYAN, VIOLET, t)
}

/// Render the full app icon (squircle background + gradient mark).
fn render_app_icon(size: u32) -> RgbaImage {
    let mut img = RgbaImage::new(size, size);
    let n = SS * SS;
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;

                    // Squircle background with the Big Sur-style margin.
                    let (rgb, a) = if in_round_rect(x, y, 0.5, 0.5, 0.44, 0.44, 0.20) {
                        let bg = lerp3(BG_TOP, BG_BOTTOM, y);
                        match mark_part(&APP_MARK, x, y) {
                            Part::None => (bg, 1.0),
                            Part::Page => (mark_color(&APP_MARK, x, y), 1.0),
                            Part::Flap => {
                                // The fold catches the light.
                                let c = mark_color(&APP_MARK, x, y);
                                (lerp3(c, [255.0, 255.0, 255.0], 0.45), 1.0)
                            }
                        }
                    } else {
                        ([0.0, 0.0, 0.0], 0.0)
                    };
                    acc[0] += rgb[0] * a;
                    acc[1] += rgb[1] * a;
                    acc[2] += rgb[2] * a;
                    acc[3] += a;
                }
            }
            let a = acc[3] / n as f32;
            let px_color = if a > 0.0 {
                Rgba([
                    (acc[0] / acc[3]).round() as u8,
                    (acc[1] / acc[3]).round() as u8,
                    (acc[2] / acc[3]).round() as u8,
                    (a * 255.0).round() as u8,
                ])
            } else {
                Rgba([0, 0, 0, 0])
            };
            img.put_pixel(px, py, px_color);
        }
    }
    img
}

/// Render the one-tone menu-bar glyph (black + alpha template image —
/// macOS recolors it for light/dark menu bars).
///
/// With `badge`, a filled dot is added at the top-right and the mark is
/// shrunk slightly to make room, signalling unmerged offline entries. It
/// has to read at 36px in both light and dark menu bars, so it is a solid
/// disc with a punched-out ring separating it from the mark rather than a
/// small glyph.
fn render_tray_glyph_badged(size: u32, badge: bool) -> RgbaImage {
    let mut img = RgbaImage::new(size, size);
    let n = SS * SS;
    for py in 0..size {
        for px in 0..size {
            let mut cov = 0.0f32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    // Badge geometry, in unit coordinates.
                    const BADGE_CX: f32 = 0.80;
                    const BADGE_CY: f32 = 0.20;
                    const BADGE_R: f32 = 0.17;
                    const BADGE_GAP: f32 = 0.055;

                    let mut mark = &TRAY_MARK;
                    let shrunk;
                    if badge {
                        // Pull the mark down-left so the badge has room.
                        shrunk = Mark {
                            cx: TRAY_MARK.cx - 0.045,
                            cy: TRAY_MARK.cy + 0.045,
                            hw: TRAY_MARK.hw * 0.88,
                            hh: TRAY_MARK.hh * 0.88,
                            ..TRAY_MARK
                        };
                        mark = &shrunk;
                    }

                    let mut inside = mark_part(mark, x, y) != Part::None;
                    if badge {
                        let d = ((x - BADGE_CX).powi(2) + (y - BADGE_CY).powi(2)).sqrt();
                        // Punch a transparent ring so the disc stays
                        // distinct where it overlaps the mark.
                        if d <= BADGE_R + BADGE_GAP {
                            inside = false;
                        }
                        if d <= BADGE_R {
                            inside = true;
                        }
                    }
                    if inside {
                        cov += 1.0;
                    }
                }
            }
            let a = (cov / n as f32 * 255.0).round() as u8;
            img.put_pixel(px, py, Rgba([0, 0, 0, a]));
        }
    }
    img
}

fn main() {
    let iconset = std::path::Path::new("assets/schl8.iconset");
    std::fs::create_dir_all(iconset).expect("create iconset dir");

    // (filename, pixel size) — the standard macOS iconset layout.
    let sizes: &[(&str, u32)] = &[
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ];
    for (name, size) in sizes {
        let img = render_app_icon(*size);
        img.save(iconset.join(name)).expect("write icon png");
        println!("wrote {name} ({size}px)");
    }

    let tray = render_tray_glyph_badged(36, false);
    tray.save("assets/tray_glyph.png")
        .expect("write tray glyph");
    println!("wrote tray_glyph.png (36px, template)");

    let tray_badged = render_tray_glyph_badged(36, true);
    tray_badged
        .save("assets/tray_glyph_pending.png")
        .expect("write badged tray glyph");
    println!("wrote tray_glyph_pending.png (36px, template, badged)");

    println!("\nNow rebuild the .icns:");
    println!("  iconutil -c icns assets/schl8.iconset -o assets/schl8.icns");
}
