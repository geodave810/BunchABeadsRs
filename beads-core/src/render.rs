//! Fit-to-area maths shared by the screen and by "Save Image", plus a small
//! software renderer that composes a design into an RGBA picture.

use crate::design::{Piece, Placed};

/// Scale and offset that fit every piece into a `width` x `height` area with a margin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: f64,
    pub off_x: f64,
    pub off_y: f64,
}

/// Bounding box (min_x, min_y, max_x, max_y) of all pieces, generous enough for any rotation.
pub fn bounds(placed: &[Placed]) -> Option<(f64, f64, f64, f64)> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in placed {
        let r = p.width.max(p.height) / 2.0 + p.x_shift.abs().max(p.y_offset.abs());
        min_x = min_x.min(p.cx - r);
        max_x = max_x.max(p.cx + r);
        min_y = min_y.min(p.cy - r);
        max_y = max_y.max(p.cy + r);
    }
    (max_x > min_x && max_y > min_y).then_some((min_x, min_y, max_x, max_y))
}

impl Fit {
    pub fn compute(placed: &[Placed], width: f64, height: f64, margin: f64) -> Option<Fit> {
        let (min_x, min_y, max_x, max_y) = bounds(placed)?;
        let scale = ((width - 2.0 * margin) / (max_x - min_x)).min((height - 2.0 * margin) / (max_y - min_y));
        if scale <= 0.0 {
            return None;
        }
        Some(Fit {
            scale,
            off_x: (width - (max_x - min_x) * scale) / 2.0 - min_x * scale,
            off_y: (height - (max_y - min_y) * scale) / 2.0 - min_y * scale,
        })
    }

    /// Fits a fixed design-space box (min_x, min_y, max_x, max_y) instead of the
    /// pieces' own bounds, so the view doesn't jump while something is dragged.
    pub fn for_box(bounds: (f64, f64, f64, f64), width: f64, height: f64, margin: f64) -> Option<Fit> {
        let (min_x, min_y, max_x, max_y) = bounds;
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        let scale = ((width - 2.0 * margin) / (max_x - min_x)).min((height - 2.0 * margin) / (max_y - min_y));
        (scale > 0.0).then(|| Fit {
            scale,
            off_x: (width - (max_x - min_x) * scale) / 2.0 - min_x * scale,
            off_y: (height - (max_y - min_y) * scale) / 2.0 - min_y * scale,
        })
    }

    /// Output coordinates -> design coordinates.
    pub fn unapply(&self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.off_x) / self.scale, (y - self.off_y) / self.scale)
    }

    /// Design coordinates -> output coordinates.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.off_x + x * self.scale, self.off_y + y * self.scale)
    }

    /// Height / width of the design's bounding box, for sizing saved images.
    pub fn aspect(placed: &[Placed]) -> f64 {
        bounds(placed).map_or(1.0, |(x0, y0, x1, y1)| (y1 - y0) / (x1 - x0))
    }
}

/// A plain RGBA picture (8 bits per channel, not premultiplied).
#[derive(Clone, Debug)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            pixels.extend_from_slice(&rgba);
        }
        Rgba { width, height, pixels }
    }

    fn sample(&self, x: f64, y: f64) -> [f64; 4] {
        // Bilinear sample; outside the picture is transparent.
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let mut acc = [0.0; 4];
        for (dx, dy, w) in [
            (0.0, 0.0, (1.0 - fx) * (1.0 - fy)),
            (1.0, 0.0, fx * (1.0 - fy)),
            (0.0, 1.0, (1.0 - fx) * fy),
            (1.0, 1.0, fx * fy),
        ] {
            let (px, py) = (x0 + dx, y0 + dy);
            if w == 0.0 || px < 0.0 || py < 0.0 || px >= self.width as f64 || py >= self.height as f64 {
                continue;
            }
            let i = ((py as u32 * self.width + px as u32) * 4) as usize;
            let a = self.pixels[i + 3] as f64 / 255.0;
            for (c, value) in acc.iter_mut().take(3).enumerate() {
                *value += self.pixels[i + c] as f64 * a * w; // premultiply while averaging
            }
            acc[3] += a * w;
        }
        acc
    }
}

/// Draws the placed pieces onto `canvas`, fitting them to its size.
/// `image_of(piece)` supplies each piece's picture.
pub fn compose<'a>(
    canvas: &mut Rgba,
    placed: &[Placed],
    margin: f64,
    mut image_of: impl FnMut(Piece) -> Option<&'a Rgba>,
) {
    let Some(fit) = Fit::compute(placed, canvas.width as f64, canvas.height as f64, margin) else {
        return;
    };
    for p in placed {
        let Some(img) = image_of(p.piece) else { continue };
        let (cx, cy) = fit.apply(p.cx, p.cy);
        let (sin, cos) = p.angle_deg.to_radians().sin_cos();
        // Scale from picture pixels to output pixels (pictures may differ from the listed size).
        let (sx_img, sy_img) = (
            fit.scale * p.width / img.width as f64,
            fit.scale * p.height / img.height as f64,
        );
        let (hw, hh) = (img.width as f64 / 2.0, img.height as f64 / 2.0);
        let (shift_x, shift_y) = (p.x_shift * fit.scale, p.y_offset * fit.scale);
        let reach = (p.width.hypot(p.height) / 2.0) * fit.scale + shift_x.abs() + shift_y.abs() + 2.0;
        let x_range = ((cx - reach).max(0.0) as u32)..((cx + reach).min(canvas.width as f64) as u32);
        let y_range = ((cy - reach).max(0.0) as u32)..((cy + reach).min(canvas.height as f64) as u32);
        for y in y_range {
            for x in x_range.clone() {
                // Output pixel centre -> piece picture coordinates (inverse rotate, unshift, unscale).
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                let (rx, ry) = (dx * cos + dy * sin - shift_x, -dx * sin + dy * cos - shift_y);
                let src = img.sample(rx / sx_img + hw - 0.5, ry / sy_img + hh - 0.5);
                let a = src[3];
                if a <= 0.0 {
                    continue;
                }
                let i = ((y * canvas.width + x) * 4) as usize;
                let dst = &mut canvas.pixels[i..i + 4];
                for c in 0..3 {
                    dst[c] = (src[c] + dst[c] as f64 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                }
                dst[3] = ((a + dst[3] as f64 / 255.0 * (1.0 - a)) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{BeadAdjust, BeadCatalog, BeadInfo};
    use crate::design::{layout_circular, Design, Parts};

    #[test]
    fn compose_draws_inside_the_canvas() {
        let mut cat = BeadCatalog::new();
        cat.push(BeadInfo {
            name: "Red".into(),
            folder: String::new(),
            image_path: None,
            width_px: 10,
            height_px: 10,
            adjust: BeadAdjust::default(),
        });
        let none = BeadCatalog::new();
        let parts = Parts { beads: &cat, crimps: &none, clasps: &[], pendants: &none };
        let red = Rgba::filled(10, 10, [255, 0, 0, 255]);
        let placed = layout_circular(&[0; 20], &Design::default(), &parts);

        let mut canvas = Rgba::filled(200, 200, [255, 255, 255, 255]);
        compose(&mut canvas, &placed, 5.0, |_| Some(&red));

        let px = |x: u32, y: u32| &canvas.pixels[((y * 200 + x) * 4) as usize..][..4];
        assert_eq!(px(100, 100), &[255, 255, 255, 255]); // centre of the ring stays background
        let red_count = canvas.pixels.chunks(4).filter(|p| p[0] > 200 && p[1] < 60).count();
        assert!(red_count > 2000, "ring should be drawn, got {red_count} red pixels");
        let aspect = Fit::aspect(&placed);
        assert!(aspect > 0.95 && aspect < 1.05);
    }
}
