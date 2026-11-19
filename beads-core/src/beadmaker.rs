//! Bead Maker: draws natural-looking bead pictures at 300 dpi, the same scale as
//! the photographed beads (4 mm = 47 px, 6 mm = 71 px).
//!
//! Each bead is a shape (an outline plus a surface height), lit like a photo
//! (soft light from the top left, a shine spot and a slightly darker rim), with
//! a material that colours every pixel. Stone patterns come from layered random
//! "value noise". The picture is drawn 3x larger and shrunk for smooth edges.

use crate::catalog::{mm_to_px, PX_PER_MM};
use crate::render::Rgba;
use crate::rng::Rng;

/// Supersampling: draw this many times larger, then average down.
const SS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Round,
    Oval,
    Faceted,
    Bicone,
    Rondelle,
    FacetRondelle,
    FacetOval,
    Barrel,
    Heishi,
    Tube,
    Cube,
    Chip,
}

impl Shape {
    pub const ALL: [Shape; 12] = [
        Shape::Round,
        Shape::Oval,
        Shape::Faceted,
        Shape::FacetOval,
        Shape::Bicone,
        Shape::Rondelle,
        Shape::FacetRondelle,
        Shape::Barrel,
        Shape::Heishi,
        Shape::Tube,
        Shape::Cube,
        Shape::Chip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Round => "Round",
            Shape::Oval => "Oval",
            Shape::Faceted => "Faceted round",
            Shape::Bicone => "Bicone",
            Shape::Rondelle => "Rondelle",
            Shape::FacetRondelle => "Faceted rondelle",
            Shape::FacetOval => "Faceted oval",
            Shape::Barrel => "Barrel",
            Shape::Heishi => "Heishi (thin disc)",
            Shape::Tube => "Tube",
            Shape::Cube => "Cube",
            Shape::Chip => "Chip (tumbled)",
        }
    }

    /// The end of the file name, matching the photographed beads (e.g. `_8mmRD`).
    pub fn suffix(self) -> &'static str {
        match self {
            Shape::Round => "RD",
            Shape::Oval => "Oval",
            Shape::Faceted => "FacetRD",
            Shape::Bicone => "Bicone",
            Shape::Rondelle => "Rondelle",
            Shape::FacetRondelle => "FacetRondelle",
            Shape::FacetOval => "FacetOval",
            Shape::Barrel => "Barrel",
            Shape::Heishi => "Heishi",
            Shape::Tube => "Tube",
            Shape::Cube => "Cube",
            Shape::Chip => "Chip",
        }
    }

    /// Shapes whose width and height are always the same.
    pub fn is_square(self) -> bool {
        matches!(self, Shape::Round | Shape::Faceted | Shape::Bicone | Shape::Cube)
    }

    /// A sensible (width along the strand, height) in mm when switching to this shape.
    pub fn default_size(self) -> (f64, f64) {
        match self {
            Shape::Round | Shape::Faceted => (8.0, 8.0),
            Shape::Bicone | Shape::Cube => (6.0, 6.0),
            Shape::Oval => (12.0, 8.0),
            Shape::Rondelle | Shape::FacetRondelle => (5.0, 8.0),
            Shape::FacetOval => (12.0, 8.0),
            Shape::Barrel => (10.0, 8.0),
            Shape::Heishi => (1.5, 6.0),
            Shape::Tube => (10.0, 4.0),
            Shape::Chip => (10.0, 7.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Glass,
    Stone,
    Agate,
    Veined,
    Speckled,
    Lava,
    Wood,
    Pearl,
    Metal,
    /// A stone (or any) picture chosen by the user, wrapped round the bead.
    Photo,
}

impl Material {
    pub const ALL: [Material; 10] = [
        Material::Glass,
        Material::Stone,
        Material::Agate,
        Material::Veined,
        Material::Speckled,
        Material::Lava,
        Material::Wood,
        Material::Pearl,
        Material::Metal,
        Material::Photo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Material::Glass => "Glass",
            Material::Stone => "Mottled stone",
            Material::Agate => "Banded agate",
            Material::Veined => "Veined stone",
            Material::Speckled => "Speckled stone",
            Material::Lava => "Lava rock",
            Material::Wood => "Wood",
            Material::Pearl => "Pearl",
            Material::Metal => "Metal",
            Material::Photo => "Photo texture",
        }
    }

    /// Does this material use a second colour (veins, bands, spots, grain)?
    pub fn uses_second_colour(self) -> bool {
        matches!(
            self,
            Material::Stone | Material::Agate | Material::Veined | Material::Speckled | Material::Wood
        )
    }

    /// What the second colour is called on screen.
    pub fn second_colour_label(self) -> &'static str {
        match self {
            Material::Agate => "Band colour",
            Material::Veined => "Vein colour",
            Material::Speckled => "Spot colour",
            Material::Wood => "Grain colour",
            _ => "Second colour",
        }
    }

    /// Sub-folder used for this material in the generated library.
    pub fn folder(self) -> &'static str {
        match self {
            Material::Glass => "Glass",
            Material::Pearl => "Pearls",
            Material::Metal => "Metal",
            Material::Wood => "Wood",
            _ => "Gemstones",
        }
    }
}

/// Everything needed to draw one bead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeadSpec {
    pub shape: Shape,
    pub material: Material,
    /// Width along the strand, in mm.
    pub width_mm: f64,
    /// Height across the strand, in mm.
    pub height_mm: f64,
    pub colour: [u8; 3],
    pub colour2: [u8; 3],
    /// Changes the random pattern (stone mottling, chip outline, which part of a photo).
    pub seed: u64,
    /// Photo texture: how many mm of stone the width of the picture shows.
    pub texture_mm: f64,
}

impl Default for BeadSpec {
    fn default() -> Self {
        BeadSpec {
            shape: Shape::Round,
            material: Material::Stone,
            width_mm: 8.0,
            height_mm: 8.0,
            colour: [150, 30, 25],
            colour2: [200, 70, 45],
            seed: 1,
            texture_mm: 20.0,
        }
    }
}

fn fmt_mm(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

impl BeadSpec {
    /// "8mm" or "12x8mm" (largest first, as bead shops write it).
    pub fn size_text(&self) -> String {
        let (a, b) = (self.width_mm.max(self.height_mm), self.width_mm.min(self.height_mm));
        if (a - b).abs() < 0.05 {
            format!("{}mm", fmt_mm(a))
        } else {
            format!("{}x{}mm", fmt_mm(a), fmt_mm(b))
        }
    }

    /// File name without ".png", e.g. `RedJasper_8mmRD`.
    pub fn file_name(&self, base: &str) -> String {
        let base: String = base
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let base = if base.is_empty() { "Bead".to_string() } else { base };
        format!("{base}_{}{}", self.size_text(), self.shape.suffix())
    }

    /// Picture size in pixels (300 dpi).
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            mm_to_px(self.width_mm).max(4) as u32,
            mm_to_px(self.height_mm).max(4) as u32,
        )
    }
}

/// A named colour starting point (e.g. "Turquoise").
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub name: &'static str,
    pub material: Material,
    pub colour: [u8; 3],
    pub colour2: [u8; 3],
}

const fn p(name: &'static str, material: Material, colour: [u8; 3], colour2: [u8; 3]) -> Preset {
    Preset {
        name,
        material,
        colour,
        colour2,
    }
}

use Material as M;
pub const PRESETS: &[Preset] = &[
    // Glass
    p("RubyGlass", M::Glass, [190, 20, 45], [0, 0, 0]),
    p("CobaltGlass", M::Glass, [30, 60, 190], [0, 0, 0]),
    p("SapphireGlass", M::Glass, [20, 45, 130], [0, 0, 0]),
    p("EmeraldGlass", M::Glass, [20, 140, 70], [0, 0, 0]),
    p("PeridotGlass", M::Glass, [150, 190, 50], [0, 0, 0]),
    p("AquaGlass", M::Glass, [60, 180, 200], [0, 0, 0]),
    p("AmethystGlass", M::Glass, [130, 60, 170], [0, 0, 0]),
    p("RoseGlass", M::Glass, [235, 140, 160], [0, 0, 0]),
    p("TopazGlass", M::Glass, [230, 170, 60], [0, 0, 0]),
    p("AmberGlass", M::Glass, [220, 130, 30], [0, 0, 0]),
    p("Crystal", M::Glass, [215, 225, 240], [0, 0, 0]),
    p("JetGlass", M::Glass, [35, 35, 40], [0, 0, 0]),
    // Mottled stones
    p("RedJasper", M::Stone, [150, 30, 25], [200, 70, 45]),
    p("Carnelian", M::Stone, [200, 80, 30], [235, 130, 60]),
    p("Aventurine", M::Stone, [40, 120, 70], [90, 170, 110]),
    p("RoseQuartz", M::Stone, [235, 180, 185], [250, 220, 222]),
    p("Amazonite", M::Stone, [110, 190, 180], [200, 235, 225]),
    p("Unakite", M::Stone, [190, 120, 110], [90, 130, 80]),
    p("Amethyst", M::Stone, [110, 60, 150], [175, 130, 200]),
    p("Hematite", M::Stone, [55, 55, 60], [90, 90, 95]),
    // Agates
    p("Agate", M::Agate, [120, 60, 30], [230, 190, 140]),
    p("BlueAgate", M::Agate, [40, 70, 130], [170, 195, 225]),
    p("GreenAgate", M::Agate, [30, 100, 60], [170, 210, 170]),
    p("TigerEye", M::Agate, [110, 70, 20], [200, 150, 60]),
    // Veined stones
    p("Turquoise", M::Veined, [60, 175, 170], [70, 60, 45]),
    p("Howlite", M::Veined, [238, 236, 230], [90, 90, 95]),
    p("Sodalite", M::Veined, [35, 55, 140], [220, 225, 235]),
    p("Malachite", M::Veined, [20, 130, 80], [10, 60, 35]),
    // Speckled stones
    p("Dalmatian", M::Speckled, [225, 215, 195], [25, 25, 25]),
    p("SnowflakeObsidian", M::Speckled, [30, 30, 32], [200, 200, 200]),
    p("Rhodonite", M::Speckled, [215, 120, 130], [40, 35, 35]),
    // Lava
    p("LavaRock", M::Lava, [70, 68, 66], [0, 0, 0]),
    p("RedLava", M::Lava, [120, 55, 45], [0, 0, 0]),
    // Wood
    p("Wood", M::Wood, [120, 75, 40], [165, 115, 65]),
    p("LightWood", M::Wood, [200, 160, 110], [170, 125, 80]),
    p("DarkWood", M::Wood, [70, 40, 25], [110, 70, 40]),
    // Pearls
    p("WhitePearl", M::Pearl, [238, 232, 222], [0, 0, 0]),
    p("CreamPearl", M::Pearl, [240, 225, 195], [0, 0, 0]),
    p("PinkPearl", M::Pearl, [240, 205, 200], [0, 0, 0]),
    p("GreyPearl", M::Pearl, [170, 170, 180], [0, 0, 0]),
    p("BlackPearl", M::Pearl, [60, 60, 72], [0, 0, 0]),
    // Metals
    p("Gold", M::Metal, [230, 180, 70], [0, 0, 0]),
    p("Silver", M::Metal, [205, 208, 212], [0, 0, 0]),
    p("Copper", M::Metal, [200, 110, 70], [0, 0, 0]),
    p("Bronze", M::Metal, [170, 120, 60], [0, 0, 0]),
    p("Gunmetal", M::Metal, [95, 95, 100], [0, 0, 0]),
];

// ------------------------------------------------------------------ helpers

fn rand01(rng: &mut Rng) -> f32 {
    (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32
}

fn cubic(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    // Catmull-Rom spline through p1..p2.
    p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

/// Smooth fractal noise in 0..1. `k` = roughly how many blobs across the width.
fn noise(w: usize, h: usize, k: f32, octaves: i32, rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    let mut amp = 1.0f32;
    let longest = w.max(h) as f32;
    for o in 0..octaves {
        let cells = ((longest / (w as f32 / k)) * 2f32.powi(o)).max(2.0) as usize;
        let n = cells + 4;
        let grid: Vec<f32> = (0..n * n).map(|_| rand01(rng)).collect();
        let (sx, sy) = (cells as f32 / w as f32, cells as f32 / h as f32);
        for y in 0..h {
            let gy = y as f32 * sy + 1.0;
            let iy = gy.floor() as usize;
            let fy = gy - iy as f32;
            for x in 0..w {
                let gx = x as f32 * sx + 1.0;
                let ix = gx.floor() as usize;
                let fx = gx - ix as f32;
                let mut rows = [0.0f32; 4];
                for (j, row) in rows.iter_mut().enumerate() {
                    let r = (iy + j - 1) * n;
                    *row = cubic(grid[r + ix - 1], grid[r + ix], grid[r + ix + 1], grid[r + ix + 2], fx);
                }
                out[y * w + x] += amp * cubic(rows[0], rows[1], rows[2], rows[3], fy);
            }
        }
        amp *= 0.5;
    }
    let (lo, hi) = out
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let range = (hi - lo).max(1e-9);
    for v in &mut out {
        *v = (*v - lo) / range;
    }
    out
}

fn box_pass(src: &[f32], dst: &mut [f32], len: usize, count: usize, stride: usize, step: usize, r: usize) {
    // Running-sum box blur along `count` lines of `len` samples, edges clamped.
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..count {
        let base = line * stride;
        let at = |i: isize| src[base + (i.clamp(0, len as isize - 1) as usize) * step];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for i in 0..len {
            dst[base + i * step] = sum * norm;
            sum += at(i as isize + r as isize + 1) - at(i as isize - r as isize);
        }
    }
}

/// Approximate Gaussian blur (three box blurs).
fn blur(v: &mut [f32], w: usize, h: usize, sigma: f32) {
    let r = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
    if r == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; v.len()];
    for _ in 0..3 {
        box_pass(v, &mut tmp, w, h, w, 1, r);
        box_pass(&tmp, v, h, w, 1, w, r);
    }
}

fn rgbf(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

// ------------------------------------------------------------------ shapes

/// Mask (0/1) and surface height (0..1) on the supersampled grid.
fn shape_field(shape: Shape, w: usize, h: usize, rng: &mut Rng) -> (Vec<f32>, Vec<f32>) {
    let mut mask = vec![0.0f32; w * h];
    let mut height = vec![0.0f32; w * h];
    // Chip outline: a few random radii around the centre, joined smoothly.
    let chip: Vec<f32> = if shape == Shape::Chip {
        let k = 5 + (rng.next_u64() % 3) as usize;
        (0..k).map(|_| 0.7 + 0.3 * rand01(rng)).collect()
    } else {
        Vec::new()
    };
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let i = y * w + x;
            let (inside, ht) = match shape {
                Shape::Round | Shape::Oval | Shape::Faceted | Shape::FacetOval => {
                    let r2 = u * u + v * v;
                    (r2 <= 1.0, (1.0 - r2).max(0.0).sqrt())
                }
                Shape::Bicone => {
                    // Two cones joined at the middle, lying along the strand: round
                    // across (top to bottom), with a sharp girdle down the centre.
                    let r = (1.0 - u.abs()).max(0.0);
                    (v.abs() * 0.98 <= r, (r * r - v * v).max(0.0).sqrt())
                }
                Shape::Barrel => {
                    // Fullest in the middle, cut flat at both ends.
                    let r = 1.0 - 0.25 * u * u;
                    (v.abs() <= r, (r * r - v * v).max(0.0).sqrt())
                }
                Shape::Heishi => {
                    // A thin flat disc seen edge-on.
                    let r = (1.0 - u.abs().powi(8)).max(0.0).powf(0.125);
                    (v.abs() <= r, (r * r - v * v).max(0.0).sqrt())
                }
                Shape::Rondelle | Shape::FacetRondelle => {
                    // A squashed ball seen edge-on, hole along the strand: round from
                    // top to bottom, with nearly flat faces where the hole goes in.
                    let r = (1.0 - u.abs().powi(4)).max(0.0).powf(0.25);
                    (v.abs() <= r, (r * r - v * v).max(0.0).sqrt())
                }
                Shape::Tube => (
                    u.abs().powi(8) + v * v <= 1.0,
                    (1.0 - v * v).max(0.0).sqrt() * (1.0 - u.abs().powi(8)).max(0.0).powf(0.25),
                ),
                Shape::Cube => {
                    let edge = (1.0 - u.abs().max(v.abs()).powi(6)).max(0.0).powf(0.4);
                    let dome = 0.75 + 0.25 * (1.0 - 0.5 * (u * u + v * v)).max(0.0).sqrt();
                    (u.abs().powi(10) + v.abs().powi(10) <= 1.0, edge * dome * 0.6)
                }
                Shape::Chip => {
                    let k = chip.len();
                    let ang = v.atan2(u);
                    let idx = (ang + std::f32::consts::PI) / std::f32::consts::TAU * k as f32;
                    let i0 = (idx.floor() as usize) % k;
                    let t = idx - idx.floor();
                    let radius = chip[i0] * (1.0 - t) + chip[(i0 + 1) % k] * t;
                    let r = (u * u + v * v).sqrt();
                    let d = (1.0 - r / radius).clamp(0.0, 1.0);
                    (r <= radius, (1.0 - (1.0 - d) * (1.0 - d)).sqrt())
                }
            };
            if inside {
                mask[i] = 1.0;
                height[i] = ht;
            }
        }
    }
    if shape == Shape::Chip {
        // Soften the creases of a tumbled chip.
        blur(&mut height, w, h, w.max(h) as f32 / 14.0);
        let mut top = 1e-6f32;
        for (hv, m) in height.iter_mut().zip(&mask) {
            *hv *= m;
            top = top.max(*hv);
        }
        for hv in &mut height {
            *hv /= top;
        }
    }
    (mask, height)
}

/// Surface direction at every pixel.
fn normals(spec: &BeadSpec, w: usize, h: usize, rng: &mut Rng) -> (Vec<f32>, Vec<[f32; 3]>) {
    if spec.shape == Shape::FacetRondelle {
        // Rows of flat facets running round the strand: a straight girdle band in
        // the middle and slanted crown facets either side, offset like a zigzag.
        let step = std::f32::consts::PI / 9.0;
        let mut mask = vec![0.0f32; w * h];
        let mut n = vec![[0.0f32, 0.0, 1.0]; w * h];
        for y in 0..h {
            let v = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            for x in 0..w {
                let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                let r = (1.0 - u.abs().powi(4)).max(0.0).powf(0.25);
                if v.abs() > r {
                    continue;
                }
                let i = y * w + x;
                mask[i] = 1.0;
                let z = (r * r - v * v).max(0.0).sqrt();
                let phi = v.atan2(z);
                // Girdle in the middle; crown facets on each side lean outwards, their
                // rows slanting and offset by half a row so the edges zigzag.
                let side = u.abs() - 0.18;
                let (tilt, shift) = if side < 0.0 {
                    (0.0, 0.0)
                } else {
                    (0.35 * u.signum(), 0.5 * step + side * step * 0.9 * u.signum())
                };
                let q = ((phi + shift) / step).floor() * step + 0.5 * step - shift;
                let (sy, cz) = (q.sin(), q.cos());
                let len = (tilt * tilt + sy * sy + cz * cz).sqrt();
                n[i] = [tilt / len, sy / len, cz / len];
            }
        }
        return (mask, n);
    }
    if matches!(spec.shape, Shape::Faceted | Shape::FacetOval) {
        // Three rings of flat facets; each facet has one direction.
        let mut mask = vec![0.0f32; w * h];
        let mut n = vec![[0.0f32, 0.0, 1.0]; w * h];
        for y in 0..h {
            let v = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            for x in 0..w {
                let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                let r2 = u * u + v * v;
                if r2 > 1.0 {
                    continue;
                }
                let i = y * w + x;
                mask[i] = 1.0;
                let ang = v.atan2(u);
                let ring = (r2.sqrt() * 3.0).floor().min(2.0);
                let sectors = 8.0 + 4.0 * ring;
                let pi = std::f32::consts::PI;
                let sect = ((ang + pi) / (2.0 * pi) * sectors).floor() + 0.5;
                let sect = sect / sectors * 2.0 * pi - pi;
                let rr = (ring + 0.5) / 3.2;
                let (nx, ny) = (sect.cos() * rr, sect.sin() * rr);
                n[i] = [nx, ny, (1.0 - nx * nx - ny * ny).max(0.0).sqrt()];
            }
        }
        return (mask, n);
    }
    let (mask, mut height) = shape_field(spec.shape, w, h, rng);
    for (hv, m) in height.iter_mut().zip(&mask) {
        *hv *= m;
    }
    blur(&mut height, w, h, SS as f32 * 0.8);
    let k = w.min(h) as f32 * 0.5;
    let at = |x: usize, y: usize| height[y * w + x];
    let mut n = vec![[0.0f32, 0.0, 1.0]; w * h];
    for y in 0..h {
        for x in 0..w {
            let gx = if w < 2 {
                0.0
            } else if x == 0 {
                at(1, y) - at(0, y)
            } else if x == w - 1 {
                at(x, y) - at(x - 1, y)
            } else {
                (at(x + 1, y) - at(x - 1, y)) * 0.5
            } * k;
            let gy = if h < 2 {
                0.0
            } else if y == 0 {
                at(x, 1) - at(x, 0)
            } else if y == h - 1 {
                at(x, y) - at(x, y - 1)
            } else {
                (at(x, y + 1) - at(x, y - 1)) * 0.5
            } * k;
            let len = (gx * gx + gy * gy + 1.0).sqrt();
            n[y * w + x] = [-gx / len, -gy / len, 1.0 / len];
        }
    }
    (mask, n)
}

// ------------------------------------------------------------------ materials

struct Surface {
    albedo: Vec<[f32; 3]>,
    shine: f32,
    spec_k: f32,
    glass: f32,
}

fn material(spec: &BeadSpec, w: usize, h: usize, n: &[[f32; 3]], rng: &mut Rng, photo: Option<&Rgba>) -> Surface {
    let c0 = rgbf(spec.colour);
    let c1 = rgbf(spec.colour2);
    let wf = w as f32;
    let len = w * h;
    let flat = |shine, spec_k, glass, albedo| Surface {
        albedo,
        shine,
        spec_k,
        glass,
    };
    match spec.material {
        Material::Glass => flat(90.0, 0.95, 0.55, vec![c0; len]),
        Material::Stone => {
            let f = noise(w, h, 3.0, 4, rng);
            let grain = noise(w, h, 25.0, 2, rng);
            let albedo = (0..len)
                .map(|i| {
                    let t = ((f[i] - 0.5) * 2.2 + 0.5).clamp(0.0, 1.0);
                    let g = (grain[i] - 0.5) * 0.18;
                    let c = mix(c0, c1, t);
                    [
                        (c[0] + g).clamp(0.0, 1.0),
                        (c[1] + g).clamp(0.0, 1.0),
                        (c[2] + g).clamp(0.0, 1.0),
                    ]
                })
                .collect();
            flat(30.0, 0.35, 0.0, albedo)
        }
        Material::Agate => {
            let warp = noise(w, h, 2.5, 4, rng);
            let albedo = (0..len)
                .map(|i| {
                    let (x, y) = ((i % w) as f32, (i / w) as f32);
                    let b = 0.5 + 0.5 * ((x * 0.6 + y) / (wf / 9.0) + warp[i] * 6.0).sin();
                    mix(c0, c1, b.powf(1.5))
                })
                .collect();
            flat(60.0, 0.6, 0.15, albedo)
        }
        Material::Veined => {
            let v1 = noise(w, h, 2.0, 5, rng);
            let v2 = noise(w, h, 3.5, 5, rng);
            let f = noise(w, h, 4.0, 4, rng);
            let albedo = (0..len)
                .map(|i| {
                    let a = (-((v1[i] - 0.5) / 0.022).powi(2)).exp();
                    let b = 0.8 * (-((v2[i] - 0.45) / 0.016).powi(2)).exp();
                    let vein = a.max(b) * 0.85;
                    let base = scale(c0, 1.0 - 0.05 * f[i]);
                    mix(base, c1, vein)
                })
                .collect();
            flat(30.0, 0.35, 0.0, albedo)
        }
        Material::Speckled => {
            let f = noise(w, h, 3.5, 4, rng);
            let mut spots: Vec<f32> = noise(w, h, 10.0, 2, rng)
                .into_iter()
                .map(|s| if s > 0.78 { 1.0 } else { 0.0 })
                .collect();
            blur(&mut spots, w, h, 1.5);
            let albedo = (0..len)
                .map(|i| mix(scale(c0, 1.0 - 0.15 * f[i]), c1, spots[i]))
                .collect();
            flat(25.0, 0.3, 0.0, albedo)
        }
        Material::Lava => {
            let f = noise(w, h, 4.0, 4, rng);
            let mut pits: Vec<f32> = noise(w, h, 14.0, 2, rng)
                .into_iter()
                .map(|s| if s > 0.62 { 1.0 } else { 0.0 })
                .collect();
            blur(&mut pits, w, h, 1.2);
            let albedo = (0..len)
                .map(|i| scale(c0, (0.75 + 0.5 * f[i]) * (1.0 - 0.8 * pits[i])))
                .collect();
            flat(8.0, 0.08, 0.0, albedo)
        }
        Material::Wood => {
            let warp = noise(w, h, 2.0, 4, rng);
            let albedo = (0..len)
                .map(|i| {
                    let x = (i % w) as f32;
                    let r = 0.5 + 0.5 * (x / (wf / 14.0) + warp[i] * 3.0).sin();
                    mix(c0, c1, r * r)
                })
                .collect();
            flat(15.0, 0.2, 0.0, albedo)
        }
        Material::Pearl => {
            // Soft lustre: a faint colour shift with the viewing angle.
            let albedo = n
                .iter()
                .map(|nv| {
                    let nz = nv[2];
                    [
                        c0[0] * (1.0 - 0.06 * nz),
                        c0[1] * (0.97 - 0.02 * (1.0 - nz)),
                        c0[2] * (0.95 + 0.04 * (1.0 - nz)),
                    ]
                })
                .collect();
            flat(12.0, 0.55, 0.0, albedo)
        }
        Material::Metal => {
            // Reflects a bright sky above and darker ground below.
            let albedo = n
                .iter()
                .map(|nv| {
                    let up = (0.5 + 0.9 * -nv[1]).clamp(0.0, 1.0);
                    scale(c0, 0.5 + 0.6 * up.powf(1.5))
                })
                .collect();
            flat(120.0, 1.0, 0.0, albedo)
        }
        Material::Photo => {
            let Some(tex) =
                photo.filter(|t| t.width > 0 && t.height > 0 && t.pixels.len() >= (t.width * t.height * 4) as usize)
            else {
                // No picture chosen yet: a plain mottled stone in the main colour.
                let f = noise(w, h, 3.0, 4, rng);
                return flat(30.0, 0.35, 0.0, f.iter().map(|&t| scale(c0, 0.85 + 0.3 * t)).collect());
            };
            // The picture's width shows `texture_mm` of stone, so small beads show
            // less of it, like real beads cut from the same rock.
            let k = tex.width as f32 / (spec.texture_mm.max(1.0) as f32 * (PX_PER_MM as f32 * SS as f32));
            // A different part of the picture for every pattern number.
            let bulge = 0.35 * w.min(h) as f32;
            // Use a part of the picture that fits inside it, so no edge (seam) shows.
            // If the bead needs more than the whole picture, it is mirrored instead.
            let pick = |extent: f32, size: f32, r: f32| {
                if extent < size {
                    bulge * k + r * (size - extent)
                } else {
                    r * size
                }
            };
            let ox = pick((w as f32 + 2.0 * bulge) * k, tex.width as f32, rand01(rng));
            let oy = pick((h as f32 + 2.0 * bulge) * k, tex.height as f32, rand01(rng));
            let albedo = (0..len)
                .map(|i| {
                    let (x, y) = ((i % w) as f32, (i / w) as f32);
                    // Push the picture outwards near the edges, as if wrapped round the bead.
                    let nv = n[i];
                    sample_wrap(tex, ox + (x + nv[0] * bulge) * k, oy + (y + nv[1] * bulge) * k)
                })
                .collect();
            flat(50.0, 0.45, 0.0, albedo)
        }
    }
}

/// Bilinear sample of a picture that repeats mirrored in both directions (no seams).
fn sample_wrap(t: &Rgba, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (t.width as i64, t.height as i64);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mirror = |i: i64, n: i64| {
        let p = i.rem_euclid(2 * n);
        if p < n {
            p
        } else {
            2 * n - 1 - p
        }
    };
    let px = |xi: i64, yi: i64| {
        let i = ((mirror(yi, h) * w + mirror(xi, w)) * 4) as usize;
        [
            t.pixels[i] as f32 / 255.0,
            t.pixels[i + 1] as f32 / 255.0,
            t.pixels[i + 2] as f32 / 255.0,
        ]
    };
    let (xi, yi) = (x0 as i64, y0 as i64);
    let top = mix(px(xi, yi), px(xi + 1, yi), fx);
    let bottom = mix(px(xi, yi + 1), px(xi + 1, yi + 1), fx);
    mix(top, bottom, fy)
}

// ------------------------------------------------------------------ drawing

/// Draws one bead picture at 300 dpi with a transparent background.
pub fn render(spec: &BeadSpec) -> Rgba {
    render_with(spec, None)
}

/// Like `render`, with the picture used by the Photo texture material.
pub fn render_with(spec: &BeadSpec, photo: Option<&Rgba>) -> Rgba {
    let (out_w, out_h) = spec.pixel_size();
    let (w, h) = (out_w as usize * SS, out_h as usize * SS);
    let mut rng = Rng::new(spec.seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xB5AD_4ECE_DA1C_E2A9);
    let (mask, n) = normals(spec, w, h, &mut rng);
    let surf = material(spec, w, h, &n, &mut rng, photo);

    let l = {
        let v = [-0.45f32, -0.65, 0.62];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / len, v[1] / len, v[2] / len]
    };
    let hv = {
        let v = [l[0], l[1], l[2] + 1.0];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / len, v[1] / len, v[2] / len]
    };

    let mut acc = vec![[0.0f32; 4]; out_w as usize * out_h as usize];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if mask[i] == 0.0 {
                continue;
            }
            let nv = n[i];
            let a = surf.albedo[i];
            let diff = (nv[0] * l[0] + nv[1] * l[1] + nv[2] * l[2]).clamp(0.0, 1.0);
            let nh = (nv[0] * hv[0] + nv[1] * hv[1] + nv[2] * hv[2]).clamp(0.0, 1.0);
            let spec_v = nh.powf(surf.shine) * surf.spec_k;
            let soft = nh.powi(6) * 0.12;
            let mut c = scale(a, 0.38 + 0.72 * diff);
            if surf.glass > 0.0 {
                // Light passing through glass glows on the side away from the light.
                let glow = (-(nv[0] * l[0] + nv[1] * l[1])).clamp(0.0, 1.0);
                let k = surf.glass * 0.65 * glow;
                c = [
                    c[0] * (1.0 - surf.glass * 0.3) + a[0] * k,
                    c[1] * (1.0 - surf.glass * 0.3) + a[1] * k,
                    c[2] * (1.0 - surf.glass * 0.3) + a[2] * k,
                ];
            }
            // Slightly darker rim, like a photo's edge falloff.
            let rim = 0.75 + 0.25 * (nv[2] * 1.6).clamp(0.0, 1.0);
            let o = (y / SS) * out_w as usize + x / SS;
            for (ch, cv) in c.iter().enumerate() {
                acc[o][ch] += ((cv + spec_v + soft) * rim).clamp(0.0, 1.0);
            }
            acc[o][3] += 1.0;
        }
    }

    let per = (SS * SS) as f32;
    let mut pixels = Vec::with_capacity(acc.len() * 4);
    for px in &acc {
        if px[3] == 0.0 {
            pixels.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let to8 = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        pixels.extend_from_slice(&[
            to8(px[0] / px[3]),
            to8(px[1] / px[3]),
            to8(px[2] / px[3]),
            to8(px[3] / per),
        ]);
    }
    Rgba {
        width: out_w,
        height: out_h,
        pixels,
    }
}

// ------------------------------------------------------------------ sample library

/// One bead of the generated sample library.
#[derive(Clone, Debug)]
pub struct LibraryBead {
    /// Sub-folder, e.g. "Gemstones".
    pub folder: &'static str,
    /// File name without ".png".
    pub name: String,
    pub spec: BeadSpec,
}

/// Shapes and sizes (width along the strand, height) made for each material.
fn library_sizes(m: Material) -> &'static [(Shape, f64, f64)] {
    use Shape::*;
    match m {
        Material::Glass => &[
            (Round, 4.0, 4.0),
            (Round, 6.0, 6.0),
            (Round, 8.0, 8.0),
            (Faceted, 4.0, 4.0),
            (Faceted, 6.0, 6.0),
            (Faceted, 8.0, 8.0),
            (Bicone, 4.0, 4.0),
            (Bicone, 6.0, 6.0),
            (Rondelle, 4.0, 6.0),
            (Rondelle, 5.0, 8.0),
            (FacetRondelle, 4.0, 6.0),
            (FacetRondelle, 5.0, 8.0),
        ],
        Material::Stone | Material::Agate | Material::Veined | Material::Speckled => &[
            (Round, 4.0, 4.0),
            (Round, 6.0, 6.0),
            (Round, 8.0, 8.0),
            (Round, 10.0, 10.0),
            (Oval, 12.0, 8.0),
            (Rondelle, 4.0, 6.0),
            (Rondelle, 5.0, 8.0),
            (FacetRondelle, 5.0, 8.0),
            (FacetOval, 12.0, 8.0),
            (Chip, 10.0, 7.0),
        ],
        Material::Photo => &[],
        Material::Lava => &[
            (Round, 6.0, 6.0),
            (Round, 8.0, 8.0),
            (Round, 10.0, 10.0),
            (Round, 12.0, 12.0),
        ],
        Material::Wood => &[
            (Round, 6.0, 6.0),
            (Round, 8.0, 8.0),
            (Round, 10.0, 10.0),
            (Oval, 10.0, 8.0),
            (Barrel, 10.0, 8.0),
            (Tube, 10.0, 5.0),
        ],
        Material::Pearl => &[
            (Round, 4.0, 4.0),
            (Round, 6.0, 6.0),
            (Round, 8.0, 8.0),
            (Round, 10.0, 10.0),
            (Oval, 8.0, 6.0),
        ],
        Material::Metal => &[
            (Round, 2.0, 2.0),
            (Round, 3.0, 3.0),
            (Round, 4.0, 4.0),
            (Round, 6.0, 6.0),
            (Cube, 3.0, 3.0),
            (Cube, 4.0, 4.0),
            (Tube, 6.0, 3.0),
            (Tube, 10.0, 4.0),
            (Bicone, 4.0, 4.0),
            (Rondelle, 2.0, 4.0),
            (Heishi, 1.0, 4.0),
            (Heishi, 1.0, 6.0),
            (Barrel, 6.0, 5.0),
        ],
    }
}

/// A few hundred varied beads: every preset colour in the usual shapes and sizes.
pub fn sample_library() -> Vec<LibraryBead> {
    let mut out = Vec::new();
    for (pi, preset) in PRESETS.iter().enumerate() {
        for (si, &(shape, w, h)) in library_sizes(preset.material).iter().enumerate() {
            let spec = BeadSpec {
                shape,
                material: preset.material,
                width_mm: w,
                height_mm: h,
                colour: preset.colour,
                colour2: preset.colour2,
                seed: (pi * 100 + si) as u64 + 7,
                texture_mm: 20.0,
            };
            out.push(LibraryBead {
                folder: preset.material.folder(),
                name: spec.file_name(preset.name),
                spec,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(img: &Rgba, x: u32, y: u32) -> u8 {
        img.pixels[((y * img.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn sizes_match_the_photographed_beads() {
        let mut s = BeadSpec::default();
        assert_eq!(render(&s).width as i32, mm_to_px(8.0)); // 8 mm = 94-95 px
        s.width_mm = 4.0;
        s.height_mm = 4.0;
        let img = render(&s);
        assert_eq!((img.width, img.height), (47, 47));
        assert_eq!(img.pixels.len(), 47 * 47 * 4);
    }

    #[test]
    fn every_shape_and_material_draws_a_solid_centre_and_clear_corners() {
        for &shape in &Shape::ALL {
            for &material in &Material::ALL {
                let (w, h) = shape.default_size();
                let s = BeadSpec {
                    shape,
                    material,
                    width_mm: w,
                    height_mm: h,
                    ..BeadSpec::default()
                };
                let img = render(&s);
                assert_eq!(
                    alpha(&img, img.width / 2, img.height / 2),
                    255,
                    "{shape:?} {material:?}"
                );
                assert_eq!(alpha(&img, 0, 0), 0, "{shape:?} {material:?}");
                // Not all one colour: it is shaded.
                let c = |x: u32, y: u32| img.pixels[((y * img.width + x) * 4) as usize];
                let (cx, cy) = (img.width / 2, img.height / 2);
                assert!(
                    c(cx - img.width / 4, cy - img.height / 6) != c(cx + img.width / 4, cy + img.height / 6)
                        || material == Material::Pearl,
                    "{shape:?} {material:?} looks flat"
                );
            }
        }
    }

    #[test]
    fn same_seed_same_picture_new_seed_new_pattern() {
        let s = BeadSpec::default();
        assert_eq!(render(&s).pixels, render(&s).pixels);
        let t = BeadSpec { seed: 2, ..s };
        assert_ne!(render(&s).pixels, render(&t).pixels);
    }

    #[test]
    fn names_follow_the_bead_naming_style() {
        let s = BeadSpec::default();
        assert_eq!(s.file_name("Red Jasper!"), "RedJasper_8mmRD");
        let r = BeadSpec {
            shape: Shape::Rondelle,
            width_mm: 5.0,
            height_mm: 8.0,
            ..s
        };
        assert_eq!(r.file_name("Cobalt"), "Cobalt_8x5mmRondelle");
        let t = BeadSpec {
            shape: Shape::Tube,
            width_mm: 2.5,
            height_mm: 2.5,
            ..s
        };
        assert_eq!(t.file_name(""), "Bead_2.5mmTube");
    }

    #[test]
    fn photo_texture_is_wrapped_onto_the_bead() {
        // A red/blue checkerboard picture.
        let mut pixels = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                pixels.extend_from_slice(if (x + y) % 2 == 0 {
                    &[200, 20, 20, 255]
                } else {
                    &[20, 20, 200, 255]
                });
            }
        }
        let tex = Rgba {
            width: 8,
            height: 8,
            pixels,
        };
        let s = BeadSpec {
            material: Material::Photo,
            texture_mm: 4.0,
            ..BeadSpec::default()
        };
        let img = render_with(&s, Some(&tex));
        let (mut reddish, mut bluish) = (0, 0);
        for px in img.pixels.chunks(4).filter(|p| p[3] == 255) {
            if px[0] as i32 > px[2] as i32 + 40 {
                reddish += 1;
            } else if px[2] as i32 > px[0] as i32 + 40 {
                bluish += 1;
            }
        }
        assert!(reddish > 500 && bluish > 500, "{reddish} {bluish}");
        // Without a picture it still draws a bead.
        assert_eq!(alpha(&render(&s), 47, 47), 255);
    }

    #[test]
    fn library_is_large_with_unique_names() {
        let lib = sample_library();
        assert!(lib.len() > 250, "{}", lib.len());
        let mut names: Vec<&str> = lib.iter().map(|b| b.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lib.len());
    }
}
