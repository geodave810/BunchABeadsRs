//! Bunch-a-Beads core: everything about beads, patterns and necklace layout,
//! with no GUI code and no dependencies. The desktop app, a future web app and
//! a future Android app all use this crate unchanged.

pub mod arc;
pub mod beadmaker;
pub mod catalog;
pub mod colour;
pub mod design;
pub mod design_file;
pub mod inventory;
pub mod manual;
pub mod memfs;
pub mod pattern;
pub mod pdf;
pub mod render;
pub mod rng;
pub mod settings;

pub use catalog::{BeadAdjust, BeadCatalog, BeadInfo, ClaspInfo, ImageInfo};
pub use design::{ClaspView, Design, DesignMode, Parts, Piece, Placed};
pub use pattern::{Pattern, MAX_PATTERNS, PATTERN_SIZE};
pub use rng::Rng;
pub use design_file::SavedDesign;
pub use manual::{ManualDesign, ManualItem};
pub use settings::{Background, Settings};
