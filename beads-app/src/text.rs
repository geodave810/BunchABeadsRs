//! Draws text into saved pictures (the material list inside the circle), using
//! the font that comes with egui, so no font files are needed.

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use beads_core::render::Rgba;
use eframe::egui;

pub struct TextPainter {
    font: FontArc,
}

impl TextPainter {
    /// Uses egui's built-in proportional font.
    pub fn new() -> Option<Self> {
        let defs = egui::FontDefinitions::default();
        let data = defs
            .font_data
            .get("Ubuntu-Light")
            .or_else(|| defs.font_data.values().next())?;
        let font = FontArc::try_from_vec(data.font.to_vec()).ok()?;
        Some(TextPainter { font })
    }

    /// Width of `text` in pixels at size `px`.
    pub fn width(&self, text: &str, px: f32) -> f32 {
        let scaled = self.font.as_scaled(PxScale::from(px));
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                w += scaled.kern(p, id);
            }
            w += scaled.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Draws one line with its top-left corner at (x, y).
    pub fn draw(&self, canvas: &mut Rgba, x: f32, y: f32, text: &str, px: f32, colour: [u8; 3]) {
        let scaled = self.font.as_scaled(PxScale::from(px));
        let baseline = y + scaled.ascent();
        let mut caret = x;
        let mut prev = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(PxScale::from(px), point(caret, baseline));
            caret += scaled.h_advance(id);
            prev = Some(id);
            let Some(outline) = self.font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px_x = bounds.min.x as i64 + gx as i64;
                let px_y = bounds.min.y as i64 + gy as i64;
                if px_x < 0 || px_y < 0 || px_x >= canvas.width as i64 || px_y >= canvas.height as i64 {
                    return;
                }
                let i = ((px_y as u32 * canvas.width + px_x as u32) * 4) as usize;
                let a = coverage.clamp(0.0, 1.0);
                for ch in 0..3 {
                    let d = canvas.pixels[i + ch] as f32;
                    canvas.pixels[i + ch] = (colour[ch] as f32 * a + d * (1.0 - a)).round() as u8;
                }
            });
        }
    }
}
