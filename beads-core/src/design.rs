//! Turning patterns into a full necklace (beads, crimps and clasp), and laying it out for drawing.

use crate::catalog::{px_to_inches, BeadCatalog, ClaspInfo, PX_PER_INCH};
use crate::manual::ManualDesign;
use crate::pattern::{Pattern, MAX_PATTERNS};
use std::f64::consts::PI;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesignMode {
    /// The selected pattern (`Design::active_pattern`) repeated end to end.
    #[default]
    OnePatternLinear,
    /// Left side forward, right side reversed.
    OnePatternMirrored,
    /// Pattern 1 on the left and right (mirrored), pattern 2 at the bottom.
    TwoPattern,
    /// Built item by item on the Edit Patterns tab.
    Manual,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    pub patterns: [Pattern; MAX_PATTERNS],
    pub mode: DesignMode,
    /// The pattern (0-4) used by the 1-pattern modes, as in 2013: the one
    /// selected under "Editing" on the Main tab. The 2-pattern mode always uses
    /// patterns 1 and 2.
    pub active_pattern: usize,
    /// Wanted length in inches (1-36, default 18 like the original).
    pub target_length_inches: f64,
    /// The old "Add / Sub" buttons: positive adds beads, negative removes beads.
    /// Left is the whole strand in 1-pattern linear mode; Right is used in the
    /// mirrored and 2-pattern modes; Bottom only in 2-pattern mode.
    pub left_adjust: i32,
    pub right_adjust: i32,
    pub bottom_adjust: i32,
    /// Chosen clasp (index into the clasp list) and whether to show it.
    pub clasp: Option<usize>,
    pub use_clasp: bool,
    /// Chosen crimp (index into the crimp catalog) and how many at each end (0-2).
    pub crimp: Option<usize>,
    pub crimps_per_end: u32,
    /// Chosen pendant (index into the pendant catalog) and whether to show it.
    pub pendant: Option<usize>,
    pub use_pendant: bool,
    /// The manual edit design (used when `mode` is `Manual`).
    pub manual: ManualDesign,
}

impl Default for Design {
    fn default() -> Self {
        Design {
            patterns: [Pattern::default(); MAX_PATTERNS],
            mode: DesignMode::default(),
            active_pattern: 0,
            target_length_inches: 18.0,
            left_adjust: 0,
            right_adjust: 0,
            bottom_adjust: 0,
            clasp: None,
            use_clasp: true, // 2013 default
            crimp: None,
            crimps_per_end: 2, // 2013 default
            pendant: None,
            use_pendant: false,
            manual: ManualDesign::default(),
        }
    }
}

/// Which part of a clasp is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClaspView {
    Both,
    Left,
    Right,
}

/// One thing on the necklace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Piece {
    Bead(usize),
    Crimp(usize),
    Clasp(usize, ClaspView),
    Pendant(usize),
}

/// Where a pendant hangs from the strand when its picture has no BeadGuide
/// hang point: this fraction of its height down from the top of the picture.
pub const DEFAULT_PENDANT_HANG: f64 = 0.10;

/// How far a pendant's picture is moved down from "centred on the strand", in
/// picture pixels: the BeadGuide line-4 value if set, else the default hang point.
pub fn pendant_drop_px(height_px: f64, adjust_y: i32) -> f64 {
    if adjust_y != 0 {
        adjust_y as f64
    } else {
        height_px * (0.5 - DEFAULT_PENDANT_HANG)
    }
}

// BeadGuide files (2013) give the horizontal shift for a program that drew every
// picture from its top-left corner. This program draws pictures centred on their
// position, so the shift is converted to "how far the picture's centre moves".

/// A bead's picture started at the left edge of its slot on the strand, moved by
/// `x_shift`; its slot is `strand_width` wide. Returns the centre's offset from the slot centre.
pub fn bead_centre_shift(width_px: f64, strand_width_px: f64, x_shift: i32) -> f64 {
    x_shift as f64 + (width_px - strand_width_px) / 2.0
}

/// A pendant's picture started `x_shift` pixels from the hang point (usually about
/// minus half its width). Returns the centre's offset from the hang point; with no
/// shift set the pendant is simply centred.
pub fn pendant_centre_shift(width_px: f64, x_shift: i32) -> f64 {
    if x_shift == 0 {
        0.0
    } else {
        x_shift as f64 + width_px / 2.0
    }
}

/// Everything a design can be made of: beads, crimps, clasps and pendants.
#[derive(Clone, Copy)]
pub struct Parts<'a> {
    pub beads: &'a BeadCatalog,
    pub crimps: &'a BeadCatalog,
    pub clasps: &'a [ClaspInfo],
    pub pendants: &'a BeadCatalog,
}

impl<'a> Parts<'a> {
    /// Picture file, pixel size and BeadGuide offsets of a piece.
    pub fn info(&self, piece: Piece) -> Option<PieceInfo<'a>> {
        match piece {
            Piece::Bead(i) => self.beads.get(i).map(|b| PieceInfo {
                path: b.image_path.as_deref(),
                width: b.width_px as f64,
                height: b.height_px as f64,
                strand_width: b.strand_width_px(),
                x_shift: bead_centre_shift(b.width_px as f64, b.strand_width_px(), b.adjust.x_shift_px),
                y_offset: b.adjust.y_offset_px as f64,
            }),
            Piece::Crimp(i) => self.crimps.get(i).map(|b| PieceInfo {
                path: b.image_path.as_deref(),
                width: b.width_px as f64,
                height: b.height_px as f64,
                strand_width: b.width_px as f64,
                x_shift: 0.0,
                y_offset: 0.0,
            }),
            Piece::Pendant(i) => self.pendants.get(i).map(|b| PieceInfo {
                path: b.image_path.as_deref(),
                width: b.width_px as f64,
                height: b.height_px as f64,
                strand_width: 0.0, // hangs from the strand; doesn't add length
                x_shift: pendant_centre_shift(b.width_px as f64, b.adjust.x_shift_px),
                y_offset: pendant_drop_px(b.height_px as f64, b.adjust.y_offset_px),
            }),
            Piece::Clasp(i, view) => {
                let c = self.clasps.get(i)?;
                let img = match view {
                    ClaspView::Both => Some(&c.both),
                    ClaspView::Left => c.left.as_ref(),
                    ClaspView::Right => c.right.as_ref(),
                }?;
                Some(PieceInfo {
                    path: img.path.as_deref(),
                    width: img.width_px as f64,
                    height: img.height_px as f64,
                    strand_width: img.width_px as f64,
                    x_shift: 0.0,
                    y_offset: 0.0,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceInfo<'a> {
    pub path: Option<&'a Path>,
    pub width: f64,
    pub height: f64,
    /// Room the piece takes along the strand.
    pub strand_width: f64,
    pub x_shift: f64,
    pub y_offset: f64,
}

/// A positioned piece, ready for any renderer. Units are 300-dpi pixels;
/// `angle_deg` is the rotation (0 = upright).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub piece: Piece,
    pub cx: f64,
    pub cy: f64,
    pub angle_deg: f64,
    pub width: f64,
    pub height: f64,
    pub x_shift: f64,
    pub y_offset: f64,
}

fn pattern_width(beads: &[usize], cat: &BeadCatalog) -> f64 {
    beads.iter().map(|&b| cat.width_of(b)).sum()
}

/// Repeat `beads` to roughly `target_px`, then add/remove `adjust` beads.
fn fill(beads: &[usize], target_px: f64, adjust: i32, cat: &BeadCatalog) -> Vec<usize> {
    let pw = pattern_width(beads, cat);
    if beads.is_empty() || pw <= 0.0 {
        return Vec::new();
    }
    let repeats = ((target_px / pw).round() as usize).max(1);
    let mut out: Vec<usize> = beads.iter().copied().cycle().take(beads.len() * repeats).collect();
    apply_adjust(&mut out, beads, adjust);
    out
}

fn apply_adjust(out: &mut Vec<usize>, beads: &[usize], adjust: i32) {
    if adjust > 0 && !beads.is_empty() {
        out.extend(beads.iter().copied().cycle().take(adjust as usize));
    } else if adjust < 0 {
        let n = (adjust.unsigned_abs() as usize).min(out.len());
        out.drain(..n);
    }
}

/// Expand a design into the ordered bead list on the strand (beads only).
pub fn build_strand(d: &Design, cat: &BeadCatalog) -> Vec<usize> {
    let target_px = d.target_length_inches.clamp(1.0, 36.0) * PX_PER_INCH;
    let one = d.patterns[d.active_pattern.min(MAX_PATTERNS - 1)].beads();
    let p1 = d.patterns[0].beads();
    match d.mode {
        DesignMode::OnePatternLinear => fill(&one, target_px, d.left_adjust, cat),
        DesignMode::OnePatternMirrored => {
            let left = fill(&one, target_px / 2.0, d.left_adjust, cat);
            let right = fill(&one, target_px / 2.0, d.right_adjust, cat);
            mirror_around(&left, &[], &right)
        }
        DesignMode::TwoPattern => {
            // Bottom: pattern 2 once (plus bottom adjust); the sides share the rest.
            let p2 = d.patterns[1].beads();
            let mut bottom = p2.clone();
            apply_adjust(&mut bottom, &p2, d.bottom_adjust);
            let side_px = ((target_px - pattern_width(&bottom, cat)) / 2.0).max(0.0);
            let left = fill(&p1, side_px, d.left_adjust, cat);
            let right = fill(&p1, side_px, d.right_adjust, cat);
            mirror_around(&left, &bottom, &right)
        }
        DesignMode::Manual => d.manual.strand(&d.patterns),
    }
}

/// Left side as-is, then the middle, then the right side reversed (so it reads
/// outward from the middle, mirroring the left side).
fn mirror_around(left: &[usize], middle: &[usize], right: &[usize]) -> Vec<usize> {
    let mut s = left.to_vec();
    s.extend_from_slice(middle);
    s.extend(right.iter().rev());
    s
}

/// The clasp actually used (if any) and the crimp pieces at each end.
fn finishing(d: &Design, parts: &Parts) -> (Option<usize>, Vec<Piece>) {
    let clasp = if d.use_clasp {
        d.clasp.filter(|&c| c < parts.clasps.len())
    } else {
        None
    };
    let crimps = match d.crimp.filter(|&c| c < parts.crimps.len()) {
        Some(c) => vec![Piece::Crimp(c); d.crimps_per_end.min(2) as usize],
        None => Vec::new(),
    };
    (clasp, crimps)
}

/// The strand from one end to the other: crimps, beads, crimps (clasp not included).
fn strand_pieces(strand: &[usize], crimps_each_end: &[Piece]) -> Vec<Piece> {
    let mut v = crimps_each_end.to_vec();
    v.extend(strand.iter().map(|&b| Piece::Bead(b)));
    v.extend_from_slice(crimps_each_end);
    v
}

fn strand_width(pieces: &[Piece], parts: &Parts) -> f64 {
    pieces
        .iter()
        .filter_map(|&p| parts.info(p))
        .map(|i| i.strand_width)
        .sum()
}

/// Common finished lengths (name, inches), for the Length menu.
pub const LENGTH_PRESETS: [(&str, f64); 9] = [
    ("Bracelet, small", 6.5),
    ("Bracelet, medium", 7.0),
    ("Bracelet, large", 7.5),
    ("Anklet", 9.0),
    ("Choker", 16.0),
    ("Princess", 18.0),
    ("Matinee", 22.0),
    ("Opera", 30.0),
    ("Rope", 36.0),
];

/// Extra stringing wire to cut beyond the finished length: about 2 inches at
/// each end for going through the crimps and back.
pub const WIRE_EXTRA_INCHES: f64 = 4.0;

/// How much beading wire to cut for a necklace of `finished_inches`.
pub fn wire_length_inches(finished_inches: f64) -> f64 {
    finished_inches + WIRE_EXTRA_INCHES
}

/// Finished length in inches: beads + crimps + clasp.
pub fn length_inches(strand: &[usize], d: &Design, parts: &Parts) -> f64 {
    let (clasp, crimps) = finishing(d, parts);
    let clasp_w = clasp.map_or(0.0, |c| parts.clasps[c].width_px());
    px_to_inches(strand_width(&strand_pieces(strand, &crimps), parts) + clasp_w)
}

pub(crate) fn placed(piece: Piece, parts: &Parts, cx: f64, cy: f64, angle_deg: f64) -> Option<Placed> {
    let i = parts.info(piece)?;
    Some(Placed {
        piece,
        cx,
        cy,
        angle_deg,
        width: i.width,
        height: i.height,
        x_shift: i.x_shift,
        y_offset: i.y_offset,
    })
}

/// The necklace opened out, end to end: left clasp half, crimps, beads, crimps,
/// right clasp half (as in the linear, arc and hanging views).
pub(crate) fn open_pieces(strand: &[usize], d: &Design, parts: &Parts) -> Vec<Piece> {
    let (clasp, crimps) = finishing(d, parts);
    let mut pieces = Vec::new();
    if let Some(c) = clasp {
        pieces.push(Piece::Clasp(c, ClaspView::Left));
    }
    pieces.extend(strand_pieces(strand, &crimps));
    if let Some(c) = clasp {
        pieces.push(Piece::Clasp(c, ClaspView::Right));
    }
    pieces
}

pub(crate) fn pendant_piece(d: &Design, parts: &Parts) -> Option<Piece> {
    d.pendant.filter(|&p| d.use_pendant && p < parts.pendants.len()).map(Piece::Pendant)
}

/// Lay the necklace out on a circle centred at (0,0), y pointing down.
/// The clasp sits upright at the top; the strand runs counter-clockwise from it
/// (down the left side first, as in 2013). Each piece's top faces the centre.
pub fn layout_circular(strand: &[usize], d: &Design, parts: &Parts) -> Vec<Placed> {
    let (clasp, crimps) = finishing(d, parts);
    let pieces = strand_pieces(strand, &crimps);
    let clasp_w = clasp.map_or(0.0, |c| parts.clasps[c].width_px());
    let total = strand_width(&pieces, parts) + clasp_w;
    if strand.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let radius = total / (2.0 * PI);
    let mut out = Vec::with_capacity(pieces.len() + 1);
    if let Some(c) = clasp {
        out.extend(placed(Piece::Clasp(c, ClaspView::Both), parts, 0.0, -radius, 0.0));
    }
    let mut along = clasp_w / 2.0;
    for p in pieces {
        let Some(info) = parts.info(p) else { continue };
        let theta = 270.0 - (along + info.strand_width / 2.0) / total * 360.0;
        along += info.strand_width;
        let rad = theta.to_radians();
        out.extend(placed(p, parts, radius * rad.cos(), radius * rad.sin(), theta - 90.0));
    }
    // The pendant hangs upright from the bottom of the circle (drawn last, on top).
    if let Some(p) = pendant_piece(d, parts) {
        out.extend(placed(p, parts, 0.0, radius, 0.0));
    }
    out
}

/// Lay the necklace out in a straight line from x = 0, centred on y = 0:
/// left clasp half, crimps, beads, crimps, right clasp half.
pub fn layout_linear(strand: &[usize], d: &Design, parts: &Parts) -> Vec<Placed> {
    if strand.is_empty() {
        return Vec::new();
    }
    let pieces = open_pieces(strand, d, parts);
    let mut x = 0.0;
    let mut out = Vec::with_capacity(pieces.len());
    for p in pieces {
        let Some(info) = parts.info(p) else { continue }; // e.g. a clasp without _left/_right pictures
        out.extend(placed(p, parts, x + info.strand_width / 2.0, 0.0, 0.0));
        x += info.strand_width;
    }
    // The pendant hangs from the middle of the strand.
    if let Some(p) = pendant_piece(d, parts) {
        out.extend(placed(p, parts, x / 2.0, 0.0, 0.0));
    }
    out
}

/// Material list: (count, name) for each bead in order of first appearance,
/// then the crimps and the clasp.
pub fn material_list(strand: &[usize], d: &Design, parts: &Parts) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for &b in strand {
        let Some(info) = parts.beads.get(b) else { continue };
        match out.iter_mut().find(|(_, n)| *n == info.name) {
            Some(e) => e.0 += 1,
            None => out.push((1, info.name.clone())),
        }
    }
    let (clasp, crimps) = finishing(d, parts);
    if let (Some(Piece::Crimp(c)), false) = (crimps.first(), strand.is_empty()) {
        out.push((
            crimps.len() * 2,
            format!("{} (crimp)", parts.crimps.get(*c).map_or("", |b| b.name.as_str())),
        ));
    }
    if let (Some(c), false) = (clasp, strand.is_empty()) {
        out.push((1, format!("{} (clasp)", parts.clasps[c].name)));
    }
    if let (Some(Piece::Pendant(p)), false) = (pendant_piece(d, parts), strand.is_empty()) {
        out.push((1, format!("{} (pendant)", parts.pendants.get(p).map_or("", |b| b.name.as_str()))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{BeadAdjust, BeadInfo, ImageInfo};

    fn bead(name: &str, w: u32, h: u32) -> BeadInfo {
        BeadInfo {
            name: name.into(),
            folder: "t".into(),
            image_path: None,
            width_px: w,
            height_px: h,
            adjust: BeadAdjust::default(),
        }
    }

    fn catalog() -> BeadCatalog {
        let mut c = BeadCatalog::new();
        c.push(bead("Amazonite6mmRD", 71, 71)); // 0
        c.push(bead("Onyx4mmRD", 47, 47)); // 1
        let mut pe = bead("Peridot8mmTD", 95, 140); // 2
        pe.adjust.string_width_px = 80;
        c.push(pe);
        c
    }

    fn crimps() -> BeadCatalog {
        let mut c = BeadCatalog::new();
        c.push(bead("GoldCrimp", 35, 24));
        c
    }

    fn clasps() -> Vec<ClaspInfo> {
        let img = |w, h| ImageInfo {
            path: None,
            width_px: w,
            height_px: h,
        };
        vec![ClaspInfo {
            name: "Toggle".into(),
            folder: "Copper".into(),
            both: img(237, 120),
            left: Some(img(120, 120)),
            right: Some(img(125, 60)),
        }]
    }

    #[test]
    fn strand_modes() {
        let cat = catalog();
        let mut d = Design::default();
        d.patterns[0].set(0, 0);
        d.patterns[0].set(1, 1); // 118 px per repeat; 18 in = 5400 px -> 46 repeats
        assert_eq!(build_strand(&d, &cat).len(), 92);
        d.left_adjust = 1;
        assert_eq!(build_strand(&d, &cat).len(), 93);
        d.left_adjust = 0;

        d.mode = DesignMode::OnePatternMirrored;
        let s = build_strand(&d, &cat);
        assert_eq!(s.len(), 92);
        assert_eq!(s.first(), s.last());
        d.right_adjust = -1; // one bead fewer on the right side only
        let s = build_strand(&d, &cat);
        assert_eq!(s.len(), 91);
        assert_eq!(s[0], 0);
        assert_eq!(*s.last().unwrap(), 1);
        d.right_adjust = 0;

        d.mode = DesignMode::TwoPattern;
        d.patterns[1].set(0, 2);
        let s = build_strand(&d, &cat);
        assert_eq!(s.iter().filter(|&&b| b == 2).count(), 1);

        // 1-pattern modes draw the selected pattern; 2-pattern mode ignores the selection.
        d.patterns[3].set(0, 2); // pattern 4 = one teardrop
        d.active_pattern = 3;
        assert_eq!(build_strand(&d, &cat).iter().filter(|&&b| b == 2).count(), 1); // still 2-pattern
        d.mode = DesignMode::OnePatternLinear;
        let s = build_strand(&d, &cat);
        assert!(!s.is_empty() && s.iter().all(|&b| b == 2));
    }

    #[test]
    fn clasp_and_crimps_add_length_and_pieces() {
        let (cat, cr, cl) = (catalog(), crimps(), clasps());
        let none = BeadCatalog::new();
        let parts = Parts { beads: &cat, crimps: &cr, clasps: &cl, pendants: &none };
        let strand = vec![0, 1, 2, 1, 0];
        let beads_px = 71.0 + 47.0 + 80.0 + 47.0 + 71.0;

        let mut d = Design::default(); // clasp on, 2 crimps per end, but nothing chosen yet
        assert!((length_inches(&strand, &d, &parts) - beads_px / 300.0).abs() < 1e-9);

        d.clasp = Some(0);
        d.crimp = Some(0);
        let want = (beads_px + 237.0 + 4.0 * 35.0) / 300.0;
        assert!((length_inches(&strand, &d, &parts) - want).abs() < 1e-9);

        let circ = layout_circular(&strand, &d, &parts);
        assert_eq!(circ.len(), 1 + 4 + 5);
        assert_eq!(circ[0].piece, Piece::Clasp(0, ClaspView::Both));
        assert!(circ[0].cx.abs() < 1e-9 && circ[0].cy < 0.0); // clasp at the top
        assert_eq!(circ[1].piece, Piece::Crimp(0));
        let first_bead = circ.iter().find(|p| matches!(p.piece, Piece::Bead(_))).unwrap();
        assert!(first_bead.cx < 0.0, "strand starts down the left side");
        let r = circ[0].cy.abs();
        for p in &circ {
            assert!((p.cx.hypot(p.cy) - r).abs() < 1e-9);
        }
        let bottom = circ.iter().max_by(|a, b| a.cy.total_cmp(&b.cy)).unwrap();
        assert!(bottom.angle_deg.rem_euclid(360.0) < 20.0 || bottom.angle_deg.rem_euclid(360.0) > 340.0); // upright at the bottom

        let lin = layout_linear(&strand, &d, &parts);
        assert_eq!(lin.first().unwrap().piece, Piece::Clasp(0, ClaspView::Left));
        assert_eq!(lin.last().unwrap().piece, Piece::Clasp(0, ClaspView::Right));
        assert_eq!(lin.len(), 2 + 4 + 5);

        let m = material_list(&strand, &d, &parts);
        assert_eq!(m[0], (2, "Amazonite6mmRD".to_string()));
        assert_eq!(m[3], (4, "GoldCrimp (crimp)".to_string()));
        assert_eq!(m[4], (1, "Toggle (clasp)".to_string()));

        d.use_clasp = false;
        d.crimps_per_end = 0;
        assert_eq!(layout_circular(&strand, &d, &parts).len(), 5);
        assert_eq!(material_list(&strand, &d, &parts).len(), 3);
    }

    #[test]
    fn pendant_hangs_from_the_middle_without_adding_length() {
        let (cat, cr, cl) = (catalog(), crimps(), clasps());
        let mut pendants = BeadCatalog::new();
        pendants.push(bead("Agate_27x50mmPendant", 319, 591)); // no BeadGuide file
        let mut tuned = bead("Angelite_31x60mmPendant", 366, 709);
        tuned.adjust.y_offset_px = 300;
        pendants.push(tuned);
        let parts = Parts { beads: &cat, crimps: &cr, clasps: &cl, pendants: &pendants };
        let strand = vec![0, 1, 0, 1, 0, 1];
        let mut d = Design { clasp: Some(0), crimp: Some(0), pendant: Some(0), ..Design::default() };
        let before = length_inches(&strand, &d, &parts);

        assert!(!layout_circular(&strand, &d, &parts).iter().any(|p| matches!(p.piece, Piece::Pendant(_))));
        d.use_pendant = true;
        assert_eq!(length_inches(&strand, &d, &parts), before);

        let circ = layout_circular(&strand, &d, &parts);
        let pend = circ.last().unwrap();
        assert_eq!(pend.piece, Piece::Pendant(0));
        let r = circ[0].cy.abs(); // the clasp sits at (0, -r)
        assert!(pend.cx.abs() < 1e-9 && (pend.cy - r).abs() < 1e-9 && pend.angle_deg == 0.0);
        assert!((pend.y_offset - 591.0 * 0.4).abs() < 1e-9); // hangs from 10% below its top

        d.pendant = Some(1);
        let lin = layout_linear(&strand, &d, &parts);
        let pend = lin.last().unwrap();
        let total: f64 = lin
            .iter()
            .filter(|p| !matches!(p.piece, Piece::Pendant(_)))
            .map(|p| parts.info(p.piece).unwrap().strand_width)
            .sum();
        assert!((pend.cx - total / 2.0).abs() < 1e-9);
        assert_eq!(pend.y_offset, 300.0); // BeadGuide line 4 wins

        let m = material_list(&strand, &d, &parts);
        assert_eq!(m.last().unwrap(), &(1, "Angelite_31x60mmPendant (pendant)".to_string()));
    }

    #[test]
    fn real_2013_pendant_files_hang_centred_from_the_bail() {
        use crate::catalog::BeadAdjust;
        // David's Agate_27x50mmPendant.txt: "0 -156 Pixel Width and Shift Adjustment",
        // "592 Pixel Height", "262 Pixel Offset from Top". 27 mm = 319 px wide.
        let text = "Agate_27x50mmPendant.png\r\n0 -156 Pixel Width and Shift Adjustment\r\n592 Pixel Height\r\n262 Pixel Offset from Top\r\n";
        let mut agate = bead("Agate_27x50mmPendant", 319, 592);
        agate.adjust = BeadAdjust::parse(text);
        let mut pendants = BeadCatalog::new();
        pendants.push(agate);
        let (cat, cr, cl) = (catalog(), crimps(), clasps());
        let parts = Parts { beads: &cat, crimps: &cr, clasps: &cl, pendants: &pendants };
        let info = parts.info(Piece::Pendant(0)).unwrap();
        assert!((info.x_shift - 3.5).abs() < 1e-9, "picture centre within a few px of the hang point");
        assert_eq!(info.y_offset, 262.0);
        assert_eq!(592.0 / 2.0 - info.y_offset, 34.0); // the strand passes 34 px below its top (the bail)

        // Beads: a teardrop 95 px wide taking 80 px of strand, no shift, starts at its slot's left edge.
        assert_eq!(bead_centre_shift(95.0, 80.0, 0), 7.5);
        assert_eq!(bead_centre_shift(71.0, 71.0, 0), 0.0);
        assert_eq!(pendant_centre_shift(319.0, 0), 0.0);
    }

    #[test]
    fn manual_mode_uses_the_items_exactly() {
        use crate::manual::ManualItem;
        let cat = catalog();
        let mut d = Design::default();
        d.patterns[0].set(0, 0);
        d.patterns[0].set(1, 1);
        d.mode = DesignMode::Manual;
        assert!(build_strand(&d, &cat).is_empty());
        d.manual.push(ManualItem::Pattern { pattern: 0, mirrored: false });
        d.manual.push(ManualItem::Bead(2));
        d.manual.push(ManualItem::Pattern { pattern: 0, mirrored: true });
        d.target_length_inches = 30.0; // ignored in manual mode
        d.left_adjust = 5; // ignored too
        assert_eq!(build_strand(&d, &cat), vec![0, 1, 2, 1, 0]);
    }
}
