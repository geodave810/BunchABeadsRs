//! Saving and opening designs ("bookmarks").
//!
//! New files are plain text in the same INI style as the settings file, and
//! store beads **by name** (not by list position), so they keep working when
//! beads are added or folders are rearranged:
//!
//! ```text
//! ; Bunch-a-Beads design file
//! [Design]
//! version = 2
//! mode = mirrored
//! length_inches = 18
//! pattern1 = RedJasper_6mmRD | SmokeyQuartz4mmRD | | Angelite6mmRD
//! ...
//! ```
//!
//! 2013 bookmark files ("Bead Design File VersionPro 1.10" / "1.12") are read too.

use crate::catalog::BeadCatalog;
use crate::design::{Design, DesignMode, Parts};
use crate::manual::{ManualDesign, ManualItem};
use crate::pattern::{Pattern, MAX_PATTERNS, PATTERN_SIZE};
use crate::settings::Background;

pub const HEADER: &str = "; Bunch-a-Beads design file";
pub const EXTENSION: &str = "txt"; // same as 2013, so old and new bookmarks share one folder

/// A design as stored on disk: everything by name.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedDesign {
    pub patterns: [[Option<String>; PATTERN_SIZE]; MAX_PATTERNS],
    pub mode: DesignMode,
    /// Pattern (0-4) used by the 1-pattern modes.
    pub active_pattern: usize,
    pub length_inches: f64,
    pub left_adjust: i32,
    pub right_adjust: i32,
    pub bottom_adjust: i32,
    pub clasp: Option<String>,
    pub use_clasp: bool,
    pub crimp: Option<String>,
    pub crimps_per_end: u32,
    pub background: Option<Background>,
    pub pendant: Option<String>,
    pub use_pendant: bool,
    /// The manual edit design (Edit Patterns tab), with beads by name.
    pub manual: Vec<SavedItem>,
    /// 2013 manual-design entries that couldn't be read (single beads stored by
    /// list position instead of by file name).
    pub unreadable_manual: usize,
}

/// One manual-design item as stored on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedItem {
    /// Pattern number 0-4 (written as P1-P5, with "m" for mirrored).
    Pattern { pattern: usize, mirrored: bool },
    Bead(String),
}

impl SavedItem {
    fn to_token(&self) -> String {
        match self {
            SavedItem::Pattern { pattern, mirrored } => format!("P{}{}", pattern + 1, if *mirrored { "m" } else { "" }),
            SavedItem::Bead(name) => format!("B:{name}"),
        }
    }

    fn from_token(t: &str) -> Option<SavedItem> {
        let t = t.trim();
        if let Some(name) = t.strip_prefix("B:").or_else(|| t.strip_prefix("b:")) {
            return non_empty(name).map(SavedItem::Bead);
        }
        let rest = t.strip_prefix('P').or_else(|| t.strip_prefix('p'))?;
        let (num, mirrored) = match rest.strip_suffix(['m', 'M']) {
            Some(n) => (n, true),
            None => (rest, false),
        };
        let n: usize = num.parse().ok()?;
        (1..=MAX_PATTERNS).contains(&n).then_some(SavedItem::Pattern { pattern: n - 1, mirrored })
    }
}

impl Default for SavedDesign {
    fn default() -> Self {
        SavedDesign {
            patterns: Default::default(),
            mode: DesignMode::OnePatternLinear,
            active_pattern: 0,
            length_inches: 18.0,
            left_adjust: 0,
            right_adjust: 0,
            bottom_adjust: 0,
            clasp: None,
            use_clasp: true,
            crimp: None,
            crimps_per_end: 2,
            background: None,
            pendant: None,
            use_pendant: false,
            manual: Vec::new(),
            unreadable_manual: 0,
        }
    }
}

fn mode_name(m: DesignMode) -> &'static str {
    match m {
        DesignMode::OnePatternLinear => "linear",
        DesignMode::OnePatternMirrored => "mirrored",
        DesignMode::TwoPattern => "two patterns",
        DesignMode::Manual => "manual",
    }
}

fn parse_mode(s: &str) -> Option<DesignMode> {
    match s.trim().to_lowercase().replace(['_', '-'], " ").as_str() {
        "linear" => Some(DesignMode::OnePatternLinear),
        "mirrored" | "mirror" => Some(DesignMode::OnePatternMirrored),
        "two patterns" | "two pattern" | "2 patterns" => Some(DesignMode::TwoPattern),
        "manual" | "manual edit" => Some(DesignMode::Manual),
        _ => None,
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// "C:\a\b\Name.png" -> "Name"; also strips a clasp's "_both".
fn name_from_path(v: &str) -> Option<String> {
    let file = v.trim().rsplit(['\\', '/']).next()?;
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let stem = if stem.to_lowercase().ends_with("_both") { &stem[..stem.len() - 5] } else { stem };
    non_empty(stem)
}

impl SavedDesign {
    /// Captures the current design, turning list positions into names.
    pub fn from_design(d: &Design, parts: &Parts, background: Background) -> Self {
        let mut patterns: [[Option<String>; PATTERN_SIZE]; MAX_PATTERNS] = Default::default();
        for (p, pat) in d.patterns.iter().enumerate() {
            for (s, slot) in pat.slots.iter().enumerate() {
                patterns[p][s] = slot.and_then(|b| parts.beads.get(b)).map(|b| b.name.clone());
            }
        }
        SavedDesign {
            patterns,
            mode: d.mode,
            active_pattern: d.active_pattern.min(MAX_PATTERNS - 1),
            length_inches: d.target_length_inches,
            left_adjust: d.left_adjust,
            right_adjust: d.right_adjust,
            bottom_adjust: d.bottom_adjust,
            clasp: d.clasp.and_then(|c| parts.clasps.get(c)).map(|c| c.name.clone()),
            use_clasp: d.use_clasp,
            crimp: d.crimp.and_then(|c| parts.crimps.get(c)).map(|c| c.name.clone()),
            crimps_per_end: d.crimps_per_end,
            background: Some(background),
            pendant: d.pendant.and_then(|p| parts.pendants.get(p)).map(|p| p.name.clone()),
            use_pendant: d.use_pendant,
            manual: d
                .manual
                .items
                .iter()
                .filter_map(|item| match *item {
                    ManualItem::Pattern { pattern, mirrored } => Some(SavedItem::Pattern { pattern, mirrored }),
                    ManualItem::Bead(b) => parts.beads.get(b).map(|b| SavedItem::Bead(b.name.clone())),
                })
                .collect(),
            ..SavedDesign::default()
        }
    }

    /// Builds a design from the saved names. Beads that can't be found are left
    /// out and returned by name, so the user can be told. Clasp/crimp keep the
    /// current choice from `base` if the saved one isn't available.
    pub fn to_design(&self, parts: &Parts, base: &Design) -> (Design, Vec<String>) {
        let mut missing: Vec<String> = Vec::new();
        let mut patterns = [Pattern::default(); MAX_PATTERNS];
        for (p, pat) in self.patterns.iter().enumerate() {
            for (s, name) in pat.iter().enumerate() {
                let Some(name) = name else { continue };
                match parts.beads.find_by_name(name) {
                    Some(i) => patterns[p].set(s, i),
                    None => {
                        if !missing.contains(name) {
                            missing.push(name.clone());
                        }
                    }
                }
            }
        }
        let mut manual = ManualDesign::default();
        for item in &self.manual {
            match item {
                SavedItem::Pattern { pattern, mirrored } => {
                    manual.push(ManualItem::Pattern { pattern: *pattern, mirrored: *mirrored });
                }
                SavedItem::Bead(name) => match parts.beads.find_by_name(name) {
                    Some(i) => {
                        manual.push(ManualItem::Bead(i));
                    }
                    None => {
                        if !missing.contains(name) {
                            missing.push(name.clone());
                        }
                    }
                },
            }
        }
        let find_clasp = |n: &String| parts.clasps.iter().position(|c| c.name.eq_ignore_ascii_case(n));
        let design = Design {
            patterns,
            mode: self.mode,
            active_pattern: self.active_pattern.min(MAX_PATTERNS - 1),
            target_length_inches: self.length_inches.clamp(1.0, 36.0),
            left_adjust: self.left_adjust,
            right_adjust: self.right_adjust,
            bottom_adjust: self.bottom_adjust,
            clasp: self.clasp.as_ref().and_then(find_clasp).or(base.clasp),
            use_clasp: self.use_clasp,
            crimp: self.crimp.as_ref().and_then(|n| parts.crimps.find_by_name(n)).or(base.crimp),
            crimps_per_end: self.crimps_per_end.min(2),
            pendant: self.pendant.as_ref().and_then(|n| parts.pendants.find_by_name(n)).or(base.pendant),
            use_pendant: self.use_pendant,
            manual,
        };
        (design, missing)
    }

    /// The names of the beads in pattern 1 (and 2), for naming files.
    fn first_bead_names(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for name in self.patterns.iter().flatten().flatten() {
            if !out.contains(&name.as_str()) {
                out.push(name);
            }
        }
        out
    }

    /// File name like the 2013 Bookmark button: the first 5 letters of the first
    /// 3 different beads, the number of beads in pattern 1, then `stamp`
    /// (e.g. "261116_1405"). Example: "RedJaSmokeAngel_6_261116_1405.txt".
    pub fn auto_file_name(&self, stamp: &str) -> String {
        let prefix: String = self
            .first_bead_names()
            .iter()
            .take(3)
            .map(|n| n.chars().filter(|c| c.is_alphanumeric()).take(5).collect::<String>())
            .collect();
        let count = self.patterns[0].iter().flatten().count();
        let prefix = if prefix.is_empty() { "Design".to_string() } else { prefix };
        format!("{prefix}_{count}_{stamp}.{EXTENSION}")
    }

    /// Writes the new (version 2) format.
    pub fn to_text(&self) -> String {
        let b = |v: bool| if v { "true" } else { "false" };
        let opt = |o: &Option<String>| o.clone().unwrap_or_default();
        let mut t = format!(
            "{HEADER}\n\
             ; Plain text. Beads are stored by name; empty pattern slots are left blank.\n\
             \n\
             [Design]\n\
             version = 2\n\
             ; linear, mirrored, two patterns or manual\n\
             mode = {}\n\
             ; the pattern (1-5) drawn by the linear and mirrored modes\n\
             active_pattern = {}\n\
             length_inches = {}\n\
             left_adjust = {}\n\
             right_adjust = {}\n\
             bottom_adjust = {}\n\
             clasp = {}\n\
             use_clasp = {}\n\
             crimp = {}\n\
             crimps_per_end = {}\n\
             background = {}\n\
             pendant = {}\n\
             use_pendant = {}\n\
             \n\
             [Patterns]\n",
            mode_name(self.mode),
            self.active_pattern + 1,
            self.length_inches,
            self.left_adjust,
            self.right_adjust,
            self.bottom_adjust,
            opt(&self.clasp),
            b(self.use_clasp),
            opt(&self.crimp),
            self.crimps_per_end,
            self.background.map(|bg| bg.label()).unwrap_or_default(),
            opt(&self.pendant),
            b(self.use_pendant),
        );
        for (i, pat) in self.patterns.iter().enumerate() {
            let last = pat.iter().rposition(Option::is_some).map_or(0, |p| p + 1);
            let slots: Vec<&str> = pat[..last].iter().map(|s| s.as_deref().unwrap_or("")).collect();
            t += &format!("pattern{} = {}\n", i + 1, slots.join(" | "));
        }
        if !self.manual.is_empty() {
            t += "\n[Manual]\n; Edit Patterns design, in order: P1-P5 = pattern (m = mirrored), B:name = one bead\n";
            for item in &self.manual {
                t += &format!("item = {}\n", item.to_token());
            }
        }
        t
    }

    /// Reads either format. Errors are messages suitable for the user.
    pub fn parse(text: &str) -> Result<SavedDesign, String> {
        let first = text.lines().next().unwrap_or("").trim().trim_start_matches('\u{feff}');
        if first.starts_with("Bead Design File VersionPro") {
            Ok(Self::parse_2013(text))
        } else if first.starts_with("Bead Design File Version") {
            Err(format!("\"{first}\" files (from the 2012 shareware version) can't be opened yet."))
        } else if text.lines().any(|l| l.trim().eq_ignore_ascii_case("[Patterns]")) {
            Ok(Self::parse_v2(text))
        } else {
            Err("This doesn't look like a Bunch-a-Beads design file.".into())
        }
    }

    fn parse_v2(text: &str) -> SavedDesign {
        let mut d = SavedDesign::default();
        let mut section = String::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_lowercase();
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim());
            match (section.as_str(), k.as_str()) {
                ("design", "mode") => d.mode = parse_mode(v).unwrap_or(d.mode),
                ("design", "active_pattern") => {
                    d.active_pattern = v.parse::<usize>().map_or(0, |n| n.clamp(1, MAX_PATTERNS) - 1)
                }
                ("design", "length_inches") => d.length_inches = v.parse().unwrap_or(d.length_inches),
                ("design", "left_adjust") => d.left_adjust = v.parse().unwrap_or(0),
                ("design", "right_adjust") => d.right_adjust = v.parse().unwrap_or(0),
                ("design", "bottom_adjust") => d.bottom_adjust = v.parse().unwrap_or(0),
                ("design", "clasp") => d.clasp = non_empty(v),
                ("design", "use_clasp") => d.use_clasp = parse_bool(v).unwrap_or(d.use_clasp),
                ("design", "crimp") => d.crimp = non_empty(v),
                ("design", "crimps_per_end") => d.crimps_per_end = v.parse::<u32>().map_or(2, |n| n.min(2)),
                ("design", "background") => d.background = crate::settings::parse_background(v),
                ("design", "pendant") => d.pendant = non_empty(v),
                ("design", "use_pendant") => d.use_pendant = parse_bool(v).unwrap_or(false),
                ("patterns", key) if key.starts_with("pattern") => {
                    let Ok(n) = key["pattern".len()..].parse::<usize>() else { continue };
                    if !(1..=MAX_PATTERNS).contains(&n) {
                        continue;
                    }
                    for (slot, name) in v.split('|').take(PATTERN_SIZE).enumerate() {
                        d.patterns[n - 1][slot] = non_empty(name);
                    }
                }
                ("manual", "item") => d.manual.extend(SavedItem::from_token(v)),
                _ => {}
            }
        }
        d
    }

    /// 2013 layout (see FillInAryOutput_New in the VB code): header, folder,
    /// list index, then 5 patterns x 12 slots x (bead path, width, height),
    /// then a fixed list of settings, then the manual-edit list, then "EOF".
    fn parse_2013(text: &str) -> SavedDesign {
        let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
        let at = |i: usize| lines.get(i).copied().unwrap_or("").trim();
        let is_true = |i: usize| at(i).eq_ignore_ascii_case("true");
        let mut d = SavedDesign::default();

        let mut idx = 3;
        for p in 0..MAX_PATTERNS {
            for s in 0..PATTERN_SIZE {
                d.patterns[p][s] = name_from_path(at(idx));
                idx += 3;
            }
        }
        let base = idx; // 183
        d.clasp = name_from_path(at(base));
        d.crimp = name_from_path(at(base + 1));
        if let Ok(len) = at(base + 2).parse::<f64>() {
            if len > 0.0 {
                d.length_inches = len.clamp(1.0, 36.0);
            }
        }
        // Selected pattern: optCurPat1, optCurPat2 (base+6, +7), optCurPat3-5 (base+34..36).
        d.active_pattern = [base + 6, base + 7, base + 34, base + 35, base + 36]
            .iter()
            .position(|&i| is_true(i))
            .unwrap_or(0);
        let two_patterns = is_true(base + 9);
        let mirrored = is_true(base + 15);
        d.mode = if two_patterns {
            DesignMode::TwoPattern
        } else if mirrored {
            DesignMode::OnePatternMirrored
        } else {
            DesignMode::OnePatternLinear
        };
        d.left_adjust = at(base + 17).parse().unwrap_or(0);
        d.right_adjust = at(base + 18).parse().unwrap_or(0);
        d.bottom_adjust = at(base + 19).parse().unwrap_or(0);
        d.crimps_per_end = if is_true(base + 20) {
            0
        } else if is_true(base + 21) {
            1
        } else {
            2
        };
        d.background = if is_true(base + 24) {
            Some(Background::Black)
        } else if is_true(base + 25) {
            Some(Background::LightGreen)
        } else if is_true(base + 26) {
            Some(Background::LightBlue)
        } else if is_true(base + 23) {
            Some(Background::White)
        } else {
            None
        };
        if !at(base + 29).is_empty() {
            d.use_clasp = is_true(base + 29);
        }
        d.use_pendant = is_true(base + 31);
        d.pendant = name_from_path(at(base + 32));
        if is_true(base + 33) {
            d.mode = DesignMode::Manual;
        }
        // Manual design entries until "EOF": a bead's file path, or a pattern
        // number (negative = mirrored). "0" marks the end of the list.
        let mut i = base + 37;
        while i < lines.len() && at(i) != "EOF" {
            let v = at(i);
            if v.to_lowercase().ends_with(".png") {
                d.manual.extend(name_from_path(v).map(SavedItem::Bead));
            } else if let Ok(n) = v.parse::<i32>() {
                match n.unsigned_abs() as usize {
                    0 => {}
                    p @ 1..=MAX_PATTERNS => d.manual.push(SavedItem::Pattern { pattern: p - 1, mirrored: n < 0 }),
                    _ => d.unreadable_manual += 1, // a mirrored single bead, stored by list position
                }
            }
            i += 1;
        }
        d
    }
}

/// Convenience for callers that only have a bead catalog (e.g. tests).
pub fn bead_names(cat: &BeadCatalog, pattern: &Pattern) -> Vec<String> {
    pattern.beads().iter().filter_map(|&b| cat.get(b)).map(|b| b.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{BeadAdjust, BeadInfo, ClaspInfo, ImageInfo};

    fn bead(name: &str) -> BeadInfo {
        BeadInfo {
            name: name.into(),
            folder: "Natural".into(),
            image_path: None,
            width_px: 71,
            height_px: 71,
            adjust: BeadAdjust::default(),
        }
    }

    fn parts_data() -> (BeadCatalog, BeadCatalog, Vec<ClaspInfo>) {
        let mut beads = BeadCatalog::new();
        for n in ["Angelite6mmRD", "RedJasper_6mmRD", "SmokeyQuartz4mmRD"] {
            beads.push(bead(n));
        }
        let mut crimps = BeadCatalog::new();
        crimps.push(bead("Crimp1x2"));
        crimps.push(bead("Crimp2x2"));
        let img = ImageInfo { path: None, width_px: 237, height_px: 296 };
        let clasps = vec![
            ClaspInfo { name: "CopperToggle15mm".into(), both: img.clone(), ..Default::default() },
            ClaspInfo { name: "CopperToggleHeart14mm".into(), both: img, ..Default::default() },
        ];
        (beads, crimps, clasps)
    }

    #[test]
    fn round_trip_by_name() {
        let (beads, crimps, clasps) = parts_data();
        let mut pendants = BeadCatalog::new();
        pendants.push(bead("Agate_27x50mmPendant"));
        let parts = Parts { beads: &beads, crimps: &crimps, clasps: &clasps, pendants: &pendants };
        let mut d = Design::default();
        d.patterns[0].set(0, 1);
        d.patterns[0].set(1, 2);
        d.patterns[0].set(3, 0); // a blank slot at 2
        d.patterns[1].set(0, 0);
        d.mode = DesignMode::OnePatternMirrored;
        d.active_pattern = 1;
        d.target_length_inches = 16.5;
        d.left_adjust = 1;
        d.right_adjust = -2;
        d.clasp = Some(1);
        d.crimp = Some(1);
        d.crimps_per_end = 1;
        d.pendant = Some(0);
        d.use_pendant = true;
        let saved = SavedDesign::from_design(&d, &parts, Background::LightBlue);
        assert!(saved.to_text().contains("pendant = Agate_27x50mmPendant"));
        let text = saved.to_text();
        assert!(text.contains("pattern1 = RedJasper_6mmRD | SmokeyQuartz4mmRD |  | Angelite6mmRD"));

        let back = SavedDesign::parse(&text).unwrap();
        assert_eq!(back, saved);

        // Open with the beads in a different order: names still match.
        let mut reordered = BeadCatalog::new();
        for n in ["SmokeyQuartz4mmRD", "Angelite6mmRD", "RedJasper_6mmRD"] {
            reordered.push(bead(n));
        }
        let parts2 = Parts { beads: &reordered, crimps: &crimps, clasps: &clasps, pendants: &pendants };
        let (d2, missing) = back.to_design(&parts2, &Design::default());
        assert!(missing.is_empty());
        assert_eq!(bead_names(&reordered, &d2.patterns[0]), vec!["RedJasper_6mmRD", "SmokeyQuartz4mmRD", "Angelite6mmRD"]);
        assert_eq!(d2.patterns[0].get(2), None);
        assert_eq!((d2.mode, d2.left_adjust, d2.right_adjust, d2.crimps_per_end), (d.mode, 1, -2, 1));
        assert_eq!(d2.active_pattern, 1);
        assert_eq!((d2.clasp, d2.crimp), (Some(1), Some(1)));
        assert_eq!((d2.pendant, d2.use_pendant), (Some(0), true));
    }

    #[test]
    fn missing_beads_are_reported() {
        let (beads, crimps, clasps) = parts_data();
        let mut pendants = BeadCatalog::new();
        pendants.push(bead("Agate_27x50mmPendant"));
        let parts = Parts { beads: &beads, crimps: &crimps, clasps: &clasps, pendants: &pendants };
        let text = "; Bunch-a-Beads design file\n[Patterns]\npattern1 = Angelite6mmRD | Unobtainium8mmRD\n";
        let (d, missing) = SavedDesign::parse(text).unwrap().to_design(&parts, &Design::default());
        assert_eq!(missing, vec!["Unobtainium8mmRD".to_string()]);
        assert_eq!(d.patterns[0].bead_count(), 1);
    }

    /// Builds a file exactly the way the 2013 FillInAryOutput_New wrote it.
    fn legacy_file(pattern1: &[&str], mirrored: bool, two: bool) -> String {
        legacy_file_with_manual(pattern1, mirrored, two, &[])
    }

    fn legacy_file_with_manual(pattern1: &[&str], mirrored: bool, two: bool, manual: &[&str]) -> String {
        let mut l: Vec<String> = vec![
            "Bead Design File VersionPro 1.12".into(),
            r"C:\Bunch-a-BeadsPatternDesignPro\Beads\Natural".into(),
            "5".into(),
        ];
        for p in 0..5 {
            for s in 0..12 {
                match (p, pattern1.get(s)) {
                    (0, Some(n)) if !n.is_empty() => {
                        l.push(format!(r"C:\Bunch-a-BeadsPatternDesignPro\Beads\Natural\{n}.png"));
                        l.push("71".into());
                        l.push("71".into());
                    }
                    _ => {
                        l.push(String::new());
                        l.push("0".into());
                        l.push("0".into());
                    }
                }
            }
        }
        let t = |b: bool| if b { "True" } else { "False" }.to_string();
        l.extend([
            r"C:\Bunch-a-BeadsPatternDesignPro\Clasps\Copper\CopperToggle15mm_both.png".into(),
            r"C:\Bunch-a-BeadsPatternDesignPro\Crimps\Crimp2x2.png".into(),
            "17".into(),  // length
            "100".into(), // magnify
            "3".into(),
            "7".into(),
            t(true),  // optCurPat1
            t(false), // optCurPat2
            t(!two),  // optDifPat1
            t(two),   // optDifPat2
            t(false), // chkAutoPat
            t(true),  // chkToolTips
            String::new(),
            String::new(),
            t(!mirrored), // optLinear
            t(mirrored),  // optMirror
            t(true),      // chkMatList
            "1".into(),   // left add/sub
            "-1".into(),  // right add/sub
            "2".into(),   // bottom add/sub
            t(false),     // crimp0
            t(true),      // crimp1
            t(false),     // crimp2
            t(false),     // white
            t(false),     // black
            t(false),     // light green
            t(true),      // light blue
            t(false),     // auto sym
            t(true),      // auto rand
            t(true),      // clasp
            r"C:\Bunch-a-BeadsPatternDesignPro\Clasps\Copper".into(),
            t(false), // pendant
            String::new(),
            t(!manual.is_empty()), // optDifPat3 (manual)
            t(false),
            t(false),
            t(false),
        ]);
        l.extend(manual.iter().map(|s| s.to_string()));
        l.extend(["0".to_string(), "EOF".to_string()]); // end of manual list, end of file
        l.join("\r\n") + "\r\n"
    }

    #[test]
    fn imports_2013_bookmarks() {
        let text = legacy_file(&["RedJasper_6mmRD", "SmokeyQuartz4mmRD", "", "Angelite6mmRD"], true, false);
        let s = SavedDesign::parse(&text).unwrap();
        assert_eq!(s.patterns[0][0].as_deref(), Some("RedJasper_6mmRD"));
        assert_eq!(s.patterns[0][2], None);
        assert_eq!(s.patterns[0][3].as_deref(), Some("Angelite6mmRD"));
        assert!(s.patterns[1].iter().all(Option::is_none));
        assert_eq!(s.mode, DesignMode::OnePatternMirrored);
        assert_eq!(s.active_pattern, 0); // optCurPat1 was checked
        assert_eq!(s.length_inches, 17.0);
        assert_eq!((s.left_adjust, s.right_adjust, s.bottom_adjust), (1, -1, 2));
        assert_eq!(s.clasp.as_deref(), Some("CopperToggle15mm"));
        assert_eq!(s.crimp.as_deref(), Some("Crimp2x2"));
        assert_eq!(s.crimps_per_end, 1);
        assert_eq!(s.background, Some(Background::LightBlue));
        assert!(s.use_clasp && s.manual.is_empty() && s.unreadable_manual == 0);

        let two = SavedDesign::parse(&legacy_file(&["Angelite6mmRD"], false, true)).unwrap();
        assert_eq!(two.mode, DesignMode::TwoPattern);

        let (beads, crimps, clasps) = parts_data();
        let mut pendants = BeadCatalog::new();
        pendants.push(bead("Agate_27x50mmPendant"));
        let parts = Parts { beads: &beads, crimps: &crimps, clasps: &clasps, pendants: &pendants };
        let (d, missing) = s.to_design(&parts, &Design::default());
        assert!(missing.is_empty());
        assert_eq!(d.patterns[0].bead_count(), 3);
        assert_eq!((d.clasp, d.crimp), (Some(0), Some(1)));
    }

    #[test]
    fn imports_2013_manual_designs() {
        let text = legacy_file_with_manual(
            &["RedJasper_6mmRD", "SmokeyQuartz4mmRD"],
            false,
            false,
            &["1", "1", r"C:\Bunch-a-BeadsPatternDesignPro\Beads\Natural\Angelite6mmRD.png", "-1", "-105"],
        );
        let s = SavedDesign::parse(&text).unwrap();
        assert_eq!(s.mode, DesignMode::Manual);
        assert_eq!(
            s.manual,
            vec![
                SavedItem::Pattern { pattern: 0, mirrored: false },
                SavedItem::Pattern { pattern: 0, mirrored: false },
                SavedItem::Bead("Angelite6mmRD".into()),
                SavedItem::Pattern { pattern: 0, mirrored: true },
            ]
        );
        assert_eq!(s.unreadable_manual, 1);

        let (beads, crimps, clasps) = parts_data();
        let mut pendants = BeadCatalog::new();
        pendants.push(bead("Agate_27x50mmPendant"));
        let parts = Parts { beads: &beads, crimps: &crimps, clasps: &clasps, pendants: &pendants };
        let (d, missing) = s.to_design(&parts, &Design::default());
        assert!(missing.is_empty());
        // RedJasper=1, Smokey=2, Angelite=0 in parts_data()
        assert_eq!(crate::design::build_strand(&d, &beads), vec![1, 2, 1, 2, 0, 2, 1]);

        // ...and it survives a save in the new format.
        let again = SavedDesign::parse(&SavedDesign::from_design(&d, &parts, Background::White).to_text()).unwrap();
        assert_eq!(again.mode, DesignMode::Manual);
        assert_eq!(again.manual, s.manual);
    }

    #[test]
    fn rejects_other_files_and_names_files_like_2013() {
        assert!(SavedDesign::parse("hello").is_err());
        assert!(SavedDesign::parse("Bead Design File Version 0.72\nx").is_err());
        let mut s = SavedDesign::default();
        s.patterns[0][0] = Some("RedJasper_6mmRD".into());
        s.patterns[0][1] = Some("SmokeyQuartz4mmRD".into());
        s.patterns[0][2] = Some("RedJasper_6mmRD".into());
        s.patterns[0][3] = Some("Angelite6mmRD".into());
        assert_eq!(s.auto_file_name("261116_1405"), "RedJaSmokeAngel_4_261116_1405.txt");
        assert_eq!(SavedDesign::default().auto_file_name("x"), "Design_0_x.txt");
    }
}
