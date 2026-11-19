//! Loads pictures (beads, crimps, clasps) on demand: a GPU texture for the
//! screen and the raw RGBA pixels for "Save Image". Keyed by file path.

use beads_core::render::Rgba;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub texture: egui::TextureHandle,
    pub rgba: Rgba,
}

#[derive(Default)]
pub struct Images {
    // None = tried and failed (missing or broken file), so we don't retry every frame.
    cache: HashMap<PathBuf, Option<Loaded>>,
}

impl Images {
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<&Loaded> {
        if !self.cache.contains_key(path) {
            let loaded = load(ctx, path);
            self.cache.insert(path.to_path_buf(), loaded);
        }
        self.cache.get(path).and_then(|o| o.as_ref())
    }

    /// Already-loaded pixels only (call `get` first).
    pub fn pixels(&self, path: &Path) -> Option<&Rgba> {
        self.cache.get(path).and_then(|o| o.as_ref()).map(|l| &l.rgba)
    }
}

fn load(ctx: &egui::Context, path: &Path) -> Option<Loaded> {
    let bytes = crate::platform::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
    let texture = ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR);
    Some(Loaded {
        texture,
        rgba: Rgba {
            width: w,
            height: h,
            pixels,
        },
    })
}
