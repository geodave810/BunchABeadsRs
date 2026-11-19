//! Bead colours, worked out from the bead photos themselves, and colour
//! schemes for automatic designs.

use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColourFamily {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
    Brown,
    Black,
    Grey,
    White,
}

impl ColourFamily {
    pub const ALL: [ColourFamily; 12] = [
        ColourFamily::Red,
        ColourFamily::Orange,
        ColourFamily::Yellow,
        ColourFamily::Green,
        ColourFamily::Teal,
        ColourFamily::Blue,
        ColourFamily::Purple,
        ColourFamily::Pink,
        ColourFamily::Brown,
        ColourFamily::Black,
        ColourFamily::Grey,
        ColourFamily::White,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColourFamily::Red => "Red",
            ColourFamily::Orange => "Orange",
            ColourFamily::Yellow => "Yellow",
            ColourFamily::Green => "Green",
            ColourFamily::Teal => "Teal",
            ColourFamily::Blue => "Blue",
            ColourFamily::Purple => "Purple",
            ColourFamily::Pink => "Pink",
            ColourFamily::Brown => "Brown",
            ColourFamily::Black => "Black",
            ColourFamily::Grey => "Grey",
            ColourFamily::White => "White",
        }
    }

    /// Black, grey, white and brown go with anything.
    pub fn is_neutral(self) -> bool {
        matches!(self, ColourFamily::Black | ColourFamily::Grey | ColourFamily::White | ColourFamily::Brown)
    }
}

/// A bead's typical colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeadColour {
    pub rgb: [u8; 3],
    /// 0-360 degrees.
    pub hue: f64,
    /// 0-1.
    pub saturation: f64,
    /// 0-1.
    pub lightness: f64,
    pub family: ColourFamily,
}

fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max - min < 1e-9 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn family_of(h: f64, s: f64, l: f64) -> ColourFamily {
    use ColourFamily::*;
    if l < 0.16 || (l < 0.24 && s < 0.35) {
        return Black;
    }
    if s < 0.13 || (s < 0.2 && l > 0.75) {
        return if l > 0.78 { White } else { Grey };
    }
    if (12.0..55.0).contains(&h) && (l < 0.42 || s < 0.35) {
        return Brown;
    }
    match h {
        h if !(12.0..345.0).contains(&h) => {
            if l > 0.72 {
                Pink
            } else {
                Red
            }
        }
        h if h < 40.0 => Orange,
        h if h < 70.0 => Yellow,
        h if h < 165.0 => Green,
        h if h < 200.0 => Teal,
        h if h < 255.0 => Blue,
        h if h < 290.0 => Purple,
        _ => {
            if l < 0.4 {
                Purple
            } else {
                Pink
            }
        }
    }
}

/// Works out a bead's colour from its picture (RGBA, not premultiplied).
/// Uses the solid pixels near the middle, ignoring shiny highlights and dark
/// shadow edges, so the result is the colour a person would call it.
pub fn analyse(rgba: &[u8], width: u32, height: u32) -> Option<BeadColour> {
    let (w, h) = (width as f64, height as f64);
    let (cx, cy, r) = (w / 2.0, h / 2.0, w.min(h) * 0.42);
    let mut samples: Vec<(f64, f64, f64, f64)> = Vec::new(); // r, g, b, lightness
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let i = ((y * width + x) * 4) as usize;
            if rgba.get(i + 3).copied().unwrap_or(0) < 200 {
                continue;
            }
            let (pr, pg, pb) = (rgba[i] as f64 / 255.0, rgba[i + 1] as f64 / 255.0, rgba[i + 2] as f64 / 255.0);
            let l = (pr.max(pg).max(pb) + pr.min(pg).min(pb)) / 2.0;
            samples.push((pr, pg, pb, l));
        }
    }
    if samples.is_empty() {
        return None;
    }
    // Drop the brightest 10% (highlights) and darkest 10% (edges) when there are enough pixels.
    samples.sort_by(|a, b| a.3.total_cmp(&b.3));
    let cut = if samples.len() >= 20 { samples.len() / 10 } else { 0 };
    let middle = &samples[cut..samples.len() - cut];
    let n = middle.len() as f64;
    let (r, g, b) = middle.iter().fold((0.0, 0.0, 0.0), |a, s| (a.0 + s.0, a.1 + s.1, a.2 + s.2));
    let (r, g, b) = (r / n, g / n, b / n);
    let (hue, saturation, lightness) = to_hsl(r, g, b);
    Some(BeadColour {
        rgb: [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8],
        hue,
        saturation,
        lightness,
        family: family_of(hue, saturation, lightness),
    })
}

/// How automatic designs choose beads by colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColourScheme {
    /// Any beads (the 2013 behaviour).
    #[default]
    Any,
    /// All beads from one colour family.
    OneColour,
    /// Neighbouring shades (hues close together).
    Matching,
    /// Two opposite colours, alternating.
    Contrasting,
    /// One colour with neutrals (black, grey, white, brown).
    WithNeutrals,
    /// One colour family from light to dark.
    LightToDark,
}

impl ColourScheme {
    pub const ALL: [ColourScheme; 6] = [
        ColourScheme::Any,
        ColourScheme::OneColour,
        ColourScheme::Matching,
        ColourScheme::Contrasting,
        ColourScheme::WithNeutrals,
        ColourScheme::LightToDark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColourScheme::Any => "Any colours",
            ColourScheme::OneColour => "One colour",
            ColourScheme::Matching => "Matching shades",
            ColourScheme::Contrasting => "Contrasting colours",
            ColourScheme::WithNeutrals => "Colour + neutrals",
            ColourScheme::LightToDark => "Light to dark",
        }
    }

    /// Name used in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            ColourScheme::Any => "any",
            ColourScheme::OneColour => "one colour",
            ColourScheme::Matching => "matching",
            ColourScheme::Contrasting => "contrasting",
            ColourScheme::WithNeutrals => "with neutrals",
            ColourScheme::LightToDark => "light to dark",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.key().eq_ignore_ascii_case(k.trim()))
    }

    /// Light to dark keeps its order instead of being shuffled.
    pub fn keeps_order(self) -> bool {
        matches!(self, ColourScheme::LightToDark)
    }
}

fn hue_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn take_random(mut v: Vec<usize>, n: usize, rng: &mut Rng) -> Vec<usize> {
    rng.shuffle(&mut v);
    v.truncate(n);
    v
}

/// Chooses up to `n` different beads from `pool` (bead index, colour) following
/// `scheme`. Several starting colours are tried; if none gives at least two
/// beads, any beads are used.
pub fn choose_beads(pool: &[(usize, BeadColour)], n: usize, scheme: ColourScheme, rng: &mut Rng) -> Vec<usize> {
    let n = n.max(1);
    if pool.is_empty() {
        return Vec::new();
    }
    let any = || pool.iter().map(|p| p.0).collect::<Vec<_>>();
    if scheme == ColourScheme::Any {
        return take_random(any(), n, rng);
    }
    // Try colourful starting beads first (in random order), then any bead.
    let mut seeds: Vec<usize> = (0..pool.len()).filter(|&i| !pool[i].1.family.is_neutral()).collect();
    rng.shuffle(&mut seeds);
    let mut rest: Vec<usize> = (0..pool.len()).filter(|&i| pool[i].1.family.is_neutral()).collect();
    rng.shuffle(&mut rest);
    seeds.extend(rest);
    for &s in seeds.iter().take(30) {
        let picked = pick_with_seed(pool, pool[s], n, scheme, rng);
        if picked.len() >= 2 || (n == 1 && !picked.is_empty()) {
            return picked;
        }
    }
    take_random(any(), n, rng)
}

fn pick_with_seed(
    pool: &[(usize, BeadColour)],
    seed: (usize, BeadColour),
    n: usize,
    scheme: ColourScheme,
    rng: &mut Rng,
) -> Vec<usize> {
    match scheme {
        ColourScheme::Any => Vec::new(), // handled by the caller
        ColourScheme::OneColour => {
            let fam: Vec<usize> = pool.iter().filter(|p| p.1.family == seed.1.family).map(|p| p.0).collect();
            take_random(fam, n, rng)
        }
        ColourScheme::Matching => {
            if seed.1.family.is_neutral() {
                return Vec::new();
            }
            let near: Vec<usize> = pool
                .iter()
                .filter(|p| !p.1.family.is_neutral() && hue_gap(p.1.hue, seed.1.hue) <= 35.0)
                .map(|p| p.0)
                .collect();
            take_random(near, n, rng)
        }
        ColourScheme::Contrasting => {
            if seed.1.family.is_neutral() {
                return Vec::new();
            }
            let opposite = (seed.1.hue + 180.0).rem_euclid(360.0);
            let side = |h: f64| -> Vec<usize> {
                pool.iter()
                    .filter(|p| !p.1.family.is_neutral() && hue_gap(p.1.hue, h) <= 30.0)
                    .map(|p| p.0)
                    .collect()
            };
            let a = take_random(side(seed.1.hue), n.div_ceil(2).max(1), rng);
            let b = take_random(side(opposite), (n / 2).max(1), rng);
            if a.is_empty() || b.is_empty() {
                return Vec::new();
            }
            // Alternate: A B A B ...
            let mut out = Vec::new();
            for i in 0..a.len().max(b.len()) {
                out.extend(a.get(i));
                out.extend(b.get(i));
            }
            out.truncate(n.max(2));
            out
        }
        ColourScheme::WithNeutrals => {
            if seed.1.family.is_neutral() {
                return Vec::new();
            }
            let neutrals: Vec<usize> = pool.iter().filter(|p| p.1.family.is_neutral()).map(|p| p.0).collect();
            let mut out = vec![seed.0];
            out.extend(take_random(neutrals, n.saturating_sub(1).max(1), rng));
            out
        }
        ColourScheme::LightToDark => {
            let mut fam: Vec<(usize, f64)> = pool
                .iter()
                .filter(|p| p.1.family == seed.1.family || hue_gap(p.1.hue, seed.1.hue) <= 25.0 && !p.1.family.is_neutral())
                .map(|p| (p.0, p.1.lightness))
                .collect();
            rng.shuffle(&mut fam);
            fam.truncate(n);
            fam.sort_by(|a, b| b.1.total_cmp(&a.1)); // lightest first
            fam.into_iter().map(|f| f.0).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round "bead" of one colour with a white highlight and a dark edge.
    fn bead(rgb: [u8; 3]) -> Vec<u8> {
        let (w, h) = (40u32, 40u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f64 - 19.5, y as f64 - 19.5);
                let d = (dx * dx + dy * dy).sqrt();
                if d > 19.0 {
                    continue; // transparent corners
                }
                let i = ((y * w + x) * 4) as usize;
                let c = if (dx + 8.0).hypot(dy + 8.0) < 3.0 {
                    [255, 255, 255] // highlight
                } else if d > 17.0 {
                    [10, 10, 10] // shadow edge
                } else {
                    rgb
                };
                px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        px
    }

    #[test]
    fn photos_are_classified() {
        let cases = [
            ([200, 30, 30], ColourFamily::Red),
            ([230, 130, 30], ColourFamily::Orange),
            ([230, 210, 40], ColourFamily::Yellow),
            ([40, 160, 60], ColourFamily::Green),
            ([30, 160, 160], ColourFamily::Teal),
            ([40, 70, 200], ColourFamily::Blue),
            ([120, 40, 170], ColourFamily::Purple),
            ([240, 140, 190], ColourFamily::Pink),
            ([110, 70, 40], ColourFamily::Brown),
            ([20, 20, 22], ColourFamily::Black),
            ([128, 128, 130], ColourFamily::Grey),
            ([240, 240, 235], ColourFamily::White),
        ];
        for (rgb, want) in cases {
            let c = analyse(&bead(rgb), 40, 40).unwrap();
            assert_eq!(c.family, want, "{rgb:?} -> {c:?}");
            // The highlight and the shadow edge don't change the colour much.
            for k in 0..3 {
                assert!((c.rgb[k] as i32 - rgb[k] as i32).abs() <= 12, "{rgb:?} -> {:?}", c.rgb);
            }
        }
        assert!(analyse(&[0u8; 16], 2, 2).is_none()); // fully transparent
    }

    fn colour(rgb: [u8; 3]) -> BeadColour {
        let (h, s, l) = to_hsl(rgb[0] as f64 / 255.0, rgb[1] as f64 / 255.0, rgb[2] as f64 / 255.0);
        BeadColour { rgb, hue: h, saturation: s, lightness: l, family: family_of(h, s, l) }
    }

    #[test]
    fn schemes_pick_suitable_beads() {
        let pool: Vec<(usize, BeadColour)> = [
            [200, 30, 30],   // 0 red
            [170, 20, 40],   // 1 red
            [230, 130, 30],  // 2 orange
            [40, 70, 200],   // 3 blue
            [60, 110, 220],  // 4 blue
            [20, 20, 22],    // 5 black
            [240, 240, 235], // 6 white
            [128, 128, 130], // 7 grey
            [250, 170, 170], // 8 light red/pink
            [120, 15, 15],   // 9 dark red
        ]
        .iter()
        .enumerate()
        .map(|(i, &c)| (i, colour(c)))
        .collect();
        let fam = |i: usize| pool[i].1.family;
        let mut rng = Rng::new(7);
        for _ in 0..200 {
            let one = choose_beads(&pool, 3, ColourScheme::OneColour, &mut rng);
            assert!(one.iter().all(|&b| fam(b) == fam(one[0])));

            let near = choose_beads(&pool, 3, ColourScheme::Matching, &mut rng);
            assert!(near.iter().all(|&b| hue_gap(pool[b].1.hue, pool[near[0]].1.hue) <= 70.0));

            let neutral = choose_beads(&pool, 4, ColourScheme::WithNeutrals, &mut rng);
            assert!(neutral[1..].iter().all(|&b| fam(b).is_neutral()));

            let fade = choose_beads(&pool, 3, ColourScheme::LightToDark, &mut rng);
            assert!(fade.windows(2).all(|w| pool[w[0]].1.lightness >= pool[w[1]].1.lightness));

            let mut seen = std::collections::HashSet::new();
            assert!(choose_beads(&pool, 5, ColourScheme::Any, &mut rng).iter().all(|b| seen.insert(*b)));
        }
        // Red vs blue are opposite enough to alternate.
        let two: Vec<(usize, BeadColour)> = [[200, 30, 30], [30, 180, 200]].iter().enumerate().map(|(i, &c)| (i, colour(c))).collect();
        let c = choose_beads(&two, 2, ColourScheme::Contrasting, &mut rng);
        assert_eq!(c.len(), 2);
        assert_ne!(two[c[0]].1.family, two[c[1]].1.family);
    }
}
