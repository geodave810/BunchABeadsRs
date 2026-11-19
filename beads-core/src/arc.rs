//! Arc Design: the necklace opened at the clasp and held up by both ends.
//!
//! * **Arc** (as in 2013): the strand follows part of a circle; `angle_deg`
//!   says how much of a full circle (10-360 degrees).
//! * **Hanging**: the two clasp ends are held at chosen points and the strand
//!   drapes between them the way a real chain does (a catenary curve), keeping
//!   its true length. Moving the hands closer makes it hang deeper.
//!
//! Coordinates are 300-dpi pixels with y pointing down, like the other layouts.

use crate::design::{open_pieces, pendant_piece, placed, Design, Parts, Piece, Placed};
use std::f64::consts::PI;

/// Length of the opened necklace (clasp halves, crimps and beads) in pixels.
pub fn open_length_px(strand: &[usize], d: &Design, parts: &Parts) -> f64 {
    open_pieces(strand, d, parts)
        .into_iter()
        .filter_map(|p| parts.info(p))
        .map(|i| i.strand_width)
        .sum()
}

/// Places every piece along a path. `path(s)` gives (x, y, angle in degrees)
/// at distance `s` from the start of the strand. The pendant hangs straight
/// down from the middle of the beads (not counting the clasp halves, which can
/// differ in width, e.g. a toggle's bar and ring), as on the circular view.
fn place_along(strand: &[usize], d: &Design, parts: &Parts, path: impl Fn(f64) -> (f64, f64, f64)) -> Vec<Placed> {
    if strand.is_empty() {
        return Vec::new();
    }
    let pieces = open_pieces(strand, d, parts);
    let mut out = Vec::with_capacity(pieces.len() + 1);
    let mut s = 0.0;
    let (mut beads_start, mut beads_end) = (None, 0.0);
    for p in pieces {
        let Some(info) = parts.info(p) else { continue };
        let is_clasp = matches!(p, Piece::Clasp(..));
        if !is_clasp && beads_start.is_none() {
            beads_start = Some(s);
        }
        let (x, y, angle) = path(s + info.strand_width / 2.0);
        out.extend(placed(p, parts, x, y, angle));
        s += info.strand_width;
        if !is_clasp {
            beads_end = s;
        }
    }
    if let Some(p) = pendant_piece(d, parts) {
        let middle = beads_start.map_or(s / 2.0, |start| (start + beads_end) / 2.0);
        let (x, y, _) = path(middle);
        out.extend(placed(p, parts, x, y, 0.0));
    }
    out
}

/// 2013 Arc Design: the strand on part of a circle, open at the top, centred
/// on the bottom of the circle. Circle centre is (0, 0).
pub fn layout_arc(strand: &[usize], d: &Design, parts: &Parts, angle_deg: f64) -> Vec<Placed> {
    let total = open_length_px(strand, d, parts);
    if total <= 0.0 {
        return Vec::new();
    }
    let theta = angle_deg.clamp(1.0, 360.0).to_radians();
    let radius = total / theta;
    place_along(strand, d, parts, |s| {
        // From the left end (90 deg + half the arc) round to the right end.
        let phi = PI / 2.0 + theta / 2.0 - s / radius;
        (radius * phi.cos(), radius * phi.sin(), phi.to_degrees() - 90.0)
    })
}

/// A chain of length `length` hanging between two points (y down).
#[derive(Clone, Copy, Debug)]
pub struct Catenary {
    left: (f64, f64),
    right: (f64, f64),
    length: f64,
    /// None = pulled straight (the ends are as far apart as the chain is long).
    shape: Option<(f64, f64, f64, f64)>, // (a, x0, c, arc length from vertex to the left end)
}

impl Catenary {
    pub fn new(left: (f64, f64), right: (f64, f64), length: f64) -> Self {
        let (mut left, mut right) = (left, right);
        if left.0 > right.0 {
            std::mem::swap(&mut left, &mut right);
        }
        let dx = (right.0 - left.0).max(1e-6);
        // Work with y pointing up so the curve opens upward: Y = a cosh((x - x0)/a) + c.
        let dy_up = -(right.1 - left.1);
        let span = (dx * dx + dy_up * dy_up).sqrt();
        if length <= span * 1.000_001 {
            return Catenary {
                left,
                right,
                length,
                shape: None,
            };
        }
        // Solve sqrt(L^2 - dY^2) = 2a sinh(dx / 2a) for a (left side falls as a grows).
        let target = (length * length - dy_up * dy_up).sqrt();
        let f = |a: f64| 2.0 * a * (dx / (2.0 * a)).sinh() - target;
        let (mut lo, mut hi) = (dx / 1400.0, dx * 1e9); // sinh stays finite above lo
        for _ in 0..200 {
            let mid = (lo * hi).sqrt(); // bisect on a log scale
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let a = (lo * hi).sqrt();
        let x0 = (left.0 + right.0) / 2.0 - a * (dy_up / length).clamp(-0.999_999, 0.999_999).atanh();
        let c = -left.1 - a * ((left.0 - x0) / a).cosh();
        let s_left = a * ((left.0 - x0) / a).sinh();
        Catenary {
            left,
            right,
            length,
            shape: Some((a, x0, c, s_left)),
        }
    }

    /// Point (x, y) and tangent angle (degrees, 0 = level) at distance `s` from the left end.
    pub fn at(&self, s: f64) -> (f64, f64, f64) {
        let s = s.clamp(0.0, self.length);
        match self.shape {
            Some((a, x0, c, s_left)) => {
                let u = (s + s_left) / a; // sinh((x - x0)/a)
                let x = x0 + a * u.asinh();
                let y_up = a * ((x - x0) / a).cosh() + c;
                // Tangent in y-down screen terms: (1, -u) normalised; angle = atan(-u).
                (x, -y_up, (-u).atan().to_degrees())
            }
            None => {
                let (dx, dy) = (self.right.0 - self.left.0, self.right.1 - self.left.1);
                let span = (dx * dx + dy * dy).sqrt().max(1e-9);
                let t = (s / span).min(1.0);
                (self.left.0 + dx * t, self.left.1 + dy * t, dy.atan2(dx).to_degrees())
            }
        }
    }

    /// The lowest point of the chain (largest y).
    pub fn lowest_y(&self) -> f64 {
        match self.shape {
            Some((a, x0, c, _)) if x0 > self.left.0 && x0 < self.right.0 => -(a + c),
            _ => self.left.1.max(self.right.1),
        }
    }
}

/// Hanging view: the clasp ends are held at `left` and `right`, given as
/// fractions of the necklace's opened length (so they don't change when the
/// design gets longer). Returns the pieces in pixels.
pub fn layout_hanging(strand: &[usize], d: &Design, parts: &Parts, left: (f64, f64), right: (f64, f64)) -> Vec<Placed> {
    let total = open_length_px(strand, d, parts);
    if total <= 0.0 {
        return Vec::new();
    }
    let (l, r) = ((left.0 * total, left.1 * total), (right.0 * total, right.1 * total));
    let chain = Catenary::new(l, r, total);
    // If the hands were given right-to-left, walk the strand from the right instead.
    let flipped = left.0 > right.0;
    place_along(strand, d, parts, |s| {
        let (x, y, a) = chain.at(if flipped { total - s } else { s });
        (x, y, if flipped { a + 180.0 } else { a })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{BeadAdjust, BeadCatalog, BeadInfo};

    fn cat() -> BeadCatalog {
        let mut c = BeadCatalog::new();
        c.push(BeadInfo {
            name: "Round6mm".into(),
            folder: String::new(),
            image_path: None,
            width_px: 71,
            height_px: 71,
            adjust: BeadAdjust::default(),
        });
        c
    }

    #[test]
    fn arc_is_symmetric_and_level_at_the_bottom() {
        let beads = cat();
        let none = BeadCatalog::new();
        let parts = Parts {
            beads: &beads,
            crimps: &none,
            clasps: &[],
            pendants: &none,
        };
        let strand = vec![0; 41];
        let d = Design::default();
        let total = open_length_px(&strand, &d, &parts);
        assert_eq!(total, 41.0 * 71.0);
        for angle in [60.0, 180.0, 300.0] {
            let placed = layout_arc(&strand, &d, &parts, angle);
            let r = total / f64::to_radians(angle);
            for p in &placed {
                assert!((p.cx.hypot(p.cy) - r).abs() < 1e-6);
            }
            let (first, last, mid) = (placed[0], placed[40], placed[20]);
            assert!((first.cx + last.cx).abs() < 1e-6 && (first.cy - last.cy).abs() < 1e-6);
            assert!(mid.cx.abs() < 1e-6 && mid.angle_deg.abs() < 1e-6 && mid.cy > 0.0);
        }
    }

    #[test]
    fn pendant_hangs_from_the_middle_of_the_beads_with_an_uneven_clasp() {
        use crate::catalog::{ClaspInfo, ImageInfo};
        let beads = cat();
        let none = BeadCatalog::new();
        let img = |w| ImageInfo {
            path: None,
            width_px: w,
            height_px: 50,
        };
        // A toggle: the bar half is much wider than the ring half.
        let clasps = vec![ClaspInfo {
            name: "Toggle".into(),
            folder: String::new(),
            both: img(160),
            left: Some(img(120)),
            right: Some(img(40)),
        }];
        let parts = Parts {
            beads: &beads,
            crimps: &none,
            clasps: &clasps,
            pendants: &beads,
        };
        let strand = vec![0; 41];
        let d = Design {
            clasp: Some(0),
            use_clasp: true,
            pendant: Some(0),
            use_pendant: true,
            ..Design::default()
        };
        for placed in [
            layout_arc(&strand, &d, &parts, 180.0),
            layout_hanging(&strand, &d, &parts, (-0.3, 0.0), (0.3, 0.0)),
        ] {
            // Pieces: left clasp half, 41 beads, right clasp half, then the pendant.
            let (middle_bead, pendant) = (placed[21], placed[placed.len() - 1]);
            assert!(
                (pendant.cx - middle_bead.cx).abs() < 1e-6,
                "{} vs {}",
                pendant.cx,
                middle_bead.cx
            );
        }
    }

    #[test]
    fn catenary_meets_both_hands_and_keeps_its_length() {
        for (l, r) in [
            ((-300.0, 0.0), (300.0, 0.0)),
            ((-200.0, -150.0), (350.0, 100.0)),
            ((-50.0, 0.0), (50.0, 0.0)),
        ] {
            let chain = Catenary::new(l, r, 1000.0);
            let (x0, y0, _) = chain.at(0.0);
            let (x1, y1, _) = chain.at(1000.0);
            assert!(
                (x0 - l.0).abs() < 1e-3 && (y0 - l.1).abs() < 1e-3,
                "starts at the left hand"
            );
            assert!(
                (x1 - r.0).abs() < 1e-3 && (y1 - r.1).abs() < 1e-3,
                "ends at the right hand: {x1},{y1}"
            );
            // Walking along it in small steps measures the same length.
            let mut len = 0.0;
            let mut prev = chain.at(0.0);
            for i in 1..=2000 {
                let p = chain.at(i as f64 * 0.5);
                len += ((p.0 - prev.0).powi(2) + (p.1 - prev.1).powi(2)).sqrt();
                prev = p;
            }
            assert!((len - 1000.0).abs() < 0.5, "length {len}");
            assert!(chain.lowest_y() > l.1.max(r.1), "it sags below the hands");
        }
        // Hands closer together = deeper.
        let wide = Catenary::new((-400.0, 0.0), (400.0, 0.0), 1000.0).lowest_y();
        let narrow = Catenary::new((-100.0, 0.0), (100.0, 0.0), 1000.0).lowest_y();
        assert!(narrow > wide);
        // Pulled tight: a straight line.
        let tight = Catenary::new((0.0, 0.0), (600.0, 800.0), 1000.0);
        let (x, y, _) = tight.at(500.0);
        assert!((x - 300.0).abs() < 1e-6 && (y - 400.0).abs() < 1e-6);
    }

    #[test]
    fn hanging_layout_is_level_in_the_middle_when_hands_are_level() {
        let beads = cat();
        let none = BeadCatalog::new();
        let parts = Parts {
            beads: &beads,
            crimps: &none,
            clasps: &[],
            pendants: &none,
        };
        let strand = vec![0; 41];
        let d = Design::default();
        let placed = layout_hanging(&strand, &d, &parts, (-0.3, 0.0), (0.3, 0.0));
        let mid = placed[20];
        assert!(mid.cx.abs() < 1e-6 && mid.angle_deg.abs() < 1e-6);
        assert!(mid.cy > placed[0].cy, "the middle hangs lowest");
        assert!(
            placed[0].angle_deg > 0.0 && placed[40].angle_deg < 0.0,
            "ends slope down toward the middle"
        );
    }
}
