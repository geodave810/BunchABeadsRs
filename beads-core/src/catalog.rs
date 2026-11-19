//! Bead images: sizes, BeadGuide adjustment files, and the list of available beads.

/// Bead photos are scaled at 300 dpi: 1 mm = 300 / 25.4 = 11.811 px
/// (4 mm = 47 px, 6 mm = 71 px, 8 mm = 94.5 px; the 2013 photos use 95).
pub const DPI: f64 = 300.0;
pub const PX_PER_MM: f64 = DPI / 25.4;
pub const PX_PER_INCH: f64 = DPI;

pub fn mm_to_px(mm: f64) -> i32 {
    (mm * PX_PER_MM + 0.5) as i32
}
pub fn px_to_mm(px: f64) -> f64 {
    px / PX_PER_MM
}
pub fn px_to_inches(px: f64) -> f64 {
    px / PX_PER_INCH
}

/// Reads width and height from the start of a PNG file (needs the first 24 bytes).
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIG {
        return None;
    }
    let be = |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    Some((be(16), be(20)))
}

/// Optional per-bead adjustment (a .txt file next to the .png, made by the old
/// BeadGuide utility). Used for teardrops and other non-symmetrical beads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BeadAdjust {
    /// Width the bead takes up on the strand (0 = use the image width).
    pub string_width_px: i32,
    /// Horizontal shift of the picture relative to the strand.
    pub x_shift_px: i32,
    /// Vertical offset of the picture relative to the strand.
    pub y_offset_px: i32,
}

impl BeadAdjust {
    /// Legacy layout (from the VB `chkForPngTxtFile` routine):
    /// line 2 = "<stringWidth> <xShift> ...", line 4 = "<yOffset> ...".
    pub fn parse(text: &str) -> Self {
        let ints = |line: &str| -> Vec<i32> { line.split_whitespace().map_while(|t| t.parse::<i32>().ok()).collect() };
        let mut a = BeadAdjust::default();
        for (n, line) in text.lines().enumerate().take(4) {
            let v = ints(line);
            match n + 1 {
                2 => {
                    if let Some(&w) = v.first() {
                        a.string_width_px = w;
                    }
                    if let Some(&s) = v.get(1) {
                        a.x_shift_px = s;
                    }
                }
                4 => {
                    if let Some(&y) = v.first() {
                        a.y_offset_px = y;
                    }
                }
                _ => {}
            }
        }
        a
    }

    /// Writes these values in the 2013 BeadGuide layout, which `parse` (and the
    /// 2013 program) reads:
    /// ```text
    /// Agate_27x50mmPendant.png
    /// 0 -156 Pixel Width and Shift Adjustment
    /// 592 Pixel Height
    /// 262 Pixel Offset from Top
    /// ```
    /// Lines after the fourth in an existing file are kept.
    pub fn to_text(&self, existing: &str, file_name: &str, image_height: u32) -> String {
        let extra: Vec<&str> = existing.lines().skip(4).collect();
        let mut out = format!(
            "{file_name}\r\n{} {} Pixel Width and Shift Adjustment\r\n{image_height} Pixel Height\r\n{} Pixel Offset from Top\r\n",
            self.string_width_px, self.x_shift_px, self.y_offset_px
        );
        for line in extra {
            out += line.trim_end_matches('\r');
            out += "\r\n";
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeadInfo {
    /// File name without extension, e.g. "GoldenTigerEye6mmRD".
    pub name: String,
    /// Sub-folder it came from, e.g. "Natural".
    pub folder: String,
    /// Where the picture lives (desktop). Web/Android versions may leave this empty
    /// and supply image bytes another way.
    pub image_path: Option<std::path::PathBuf>,
    pub width_px: u32,
    pub height_px: u32,
    pub adjust: BeadAdjust,
}

impl BeadInfo {
    /// Width this bead occupies along the strand, in 300-dpi pixels.
    pub fn strand_width_px(&self) -> f64 {
        if self.adjust.string_width_px > 0 {
            self.adjust.string_width_px as f64
        } else {
            self.width_px as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BeadCatalog {
    beads: Vec<BeadInfo>,
}

impl BeadCatalog {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.beads.clear();
    }
    pub fn len(&self) -> usize {
        self.beads.len()
    }
    pub fn is_empty(&self) -> bool {
        self.beads.is_empty()
    }
    pub fn get(&self, i: usize) -> Option<&BeadInfo> {
        self.beads.get(i)
    }
    pub fn get_mut(&mut self, i: usize) -> Option<&mut BeadInfo> {
        self.beads.get_mut(i)
    }
    pub fn all(&self) -> &[BeadInfo] {
        &self.beads
    }
    pub fn push(&mut self, bead: BeadInfo) {
        self.beads.push(bead);
    }

    /// Sorts beads by name (case-insensitive), like the 2013 bead list.
    pub fn sort_by_name(&mut self) {
        self.beads.sort_by_key(|b| b.name.to_lowercase());
    }

    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        let want = name.to_lowercase();
        self.beads.iter().position(|b| b.name.to_lowercase() == want)
    }

    /// Strand width of bead `i`, or 0 if `i` is out of range.
    pub fn width_of(&self, i: usize) -> f64 {
        self.get(i).map_or(0.0, |b| b.strand_width_px())
    }
}

/// A picture and its pixel size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageInfo {
    pub path: Option<std::path::PathBuf>,
    pub width_px: u32,
    pub height_px: u32,
}

/// A clasp comes as up to three pictures, named like the 2013 program:
/// `Name_both.png` (whole clasp, used at the top of the circular view) and
/// `Name_left.png` / `Name_right.png` (the two halves, used at the ends of the linear view).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClaspInfo {
    /// The part before `_both`, e.g. "CopperToggle".
    pub name: String,
    pub folder: String,
    pub both: ImageInfo,
    pub left: Option<ImageInfo>,
    pub right: Option<ImageInfo>,
}

impl ClaspInfo {
    /// Width the clasp adds to the necklace (the whole clasp).
    pub fn width_px(&self) -> f64 {
        self.both.width_px as f64
    }
}

// ---------------------------------------------------------------------------
// Reading bead folders. The folders come through `Files`, so the same code reads
// the real disk on a computer and the browser's own file store on the web.
pub mod disk {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Where bead pictures come from.
    pub trait Files {
        /// Everything directly inside `dir`: (path, is it a folder).
        fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)>;
        /// The first `n` bytes of a file (enough for a PNG's size).
        fn head(&self, path: &Path, n: usize) -> Option<Vec<u8>>;
        fn read_text(&self, path: &Path) -> Option<String>;
    }

    /// The computer's own disk.
    #[cfg(not(target_arch = "wasm32"))]
    pub struct DiskFiles;

    #[cfg(not(target_arch = "wasm32"))]
    impl Files for DiskFiles {
        fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
            std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| {
                            let p = e.path();
                            let d = p.is_dir();
                            (p, d)
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        fn head(&self, path: &Path, n: usize) -> Option<Vec<u8>> {
            use std::io::Read;
            let mut buf = vec![0u8; n];
            std::fs::File::open(path).ok()?.read_exact(&mut buf).ok()?;
            Some(buf)
        }
        fn read_text(&self, path: &Path) -> Option<String> {
            std::fs::read_to_string(path).ok()
        }
    }

    impl BeadCatalog {
        /// Adds every *.png under `folder` (including sub-folders). Returns how many were added.
        pub fn add_folder(&mut self, files: &dyn Files, folder: &Path) -> usize {
            let before = self.beads.len();
            let mut stack = vec![folder.to_path_buf()];
            while let Some(dir) = stack.pop() {
                for (path, is_dir) in files.list(&dir) {
                    if is_dir {
                        stack.push(path);
                    } else if is_png(&path) {
                        if let Some(b) = read_bead(files, &path) {
                            self.beads.push(b);
                        }
                    }
                }
            }
            self.sort_by_name();
            self.beads.len() - before
        }

        /// Adds every *.png directly inside `folder` (no sub-folders), as used for crimps.
        pub fn add_folder_flat(&mut self, files: &dyn Files, folder: &Path) -> usize {
            let before = self.beads.len();
            for (path, is_dir) in files.list(folder) {
                if !is_dir && is_png(&path) {
                    if let Some(b) = read_bead(files, &path) {
                        self.beads.push(b);
                    }
                }
            }
            self.sort_by_name();
            self.beads.len() - before
        }

        /// Immediate sub-folders of `root` (the old "bead folders" list).
        pub fn sub_folders(files: &dyn Files, root: &Path) -> Vec<PathBuf> {
            let mut out: Vec<PathBuf> = files.list(root).into_iter().filter(|e| e.1).map(|e| e.0).collect();
            out.sort();
            out
        }
    }

    /// Finds every clasp (`*_both.png`) under `folder`, including sub-folders, sorted by name.
    pub fn scan_clasps(files: &dyn Files, folder: &Path) -> Vec<ClaspInfo> {
        let mut out = Vec::new();
        let mut stack = vec![folder.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for (path, is_dir) in files.list(&dir) {
                if is_dir {
                    stack.push(path);
                    continue;
                }
                if !is_png(&path) {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let Some(name) = strip_suffix_ignore_case(&stem, "_both") else {
                    continue;
                };
                let Some(both) = read_image(files, &path) else { continue };
                let sibling = |suffix: &str| {
                    let p = dir.join(format!("{name}{suffix}.png"));
                    read_image(files, &p)
                };
                out.push(ClaspInfo {
                    name: name.to_string(),
                    folder: dir
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    both,
                    left: sibling("_left"),
                    right: sibling("_right"),
                });
            }
        }
        out.sort_by_key(|c| (c.folder.to_lowercase(), c.name.to_lowercase()));
        out
    }

    fn strip_suffix_ignore_case<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
        let cut = s.len().checked_sub(suffix.len())?;
        (s.is_char_boundary(cut) && s[cut..].eq_ignore_ascii_case(suffix)).then(|| &s[..cut])
    }

    fn read_image(files: &dyn Files, path: &Path) -> Option<ImageInfo> {
        let header = files.head(path, 24)?;
        let (w, h) = png_size(&header)?;
        Some(ImageInfo {
            path: Some(path.to_path_buf()),
            width_px: w,
            height_px: h,
        })
    }

    fn is_png(p: &Path) -> bool {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    }

    fn read_bead(files: &dyn Files, path: &Path) -> Option<BeadInfo> {
        let header = files.head(path, 24)?;
        let (w, h) = png_size(&header)?;
        let adjust = files
            .read_text(&path.with_extension("txt"))
            .map(|t| BeadAdjust::parse(&t))
            .unwrap_or_default();
        Some(BeadInfo {
            name: path.file_stem()?.to_string_lossy().into_owned(),
            folder: path
                .parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
            image_path: Some(path.to_path_buf()),
            width_px: w,
            height_px: h,
            adjust,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mm_conversions_match_the_2013_table() {
        assert_eq!(mm_to_px(4.0), 47);
        assert_eq!(mm_to_px(6.0), 71);
        assert_eq!(mm_to_px(10.0), 118);
    }

    #[test]
    fn adjust_file_parses() {
        let a = BeadAdjust::parse("header\n80 -3 x\nfoo\n12 y\n");
        assert_eq!(
            a,
            BeadAdjust {
                string_width_px: 80,
                x_shift_px: -3,
                y_offset_px: 12
            }
        );
        assert_eq!(BeadAdjust::parse(""), BeadAdjust::default());
    }

    #[test]
    fn adjust_file_writes_back_in_the_2013_layout() {
        let a = BeadAdjust {
            string_width_px: 0,
            x_shift_px: -156,
            y_offset_px: 262,
        };
        let text = a.to_text("", "Agate_27x50mmPendant.png", 592);
        assert_eq!(
            text,
            "Agate_27x50mmPendant.png\r\n0 -156 Pixel Width and Shift Adjustment\r\n592 Pixel Height\r\n262 Pixel Offset from Top\r\n"
        );
        assert_eq!(BeadAdjust::parse(&text), a);
        // Anything after line 4 of an existing file is kept.
        let updated = a.to_text("a\r\nb\r\nc\r\nd\r\nnotes\r\n", "x.png", 592);
        assert!(updated.ends_with("262 Pixel Offset from Top\r\nnotes\r\n"));
    }

    #[test]
    fn png_header() {
        let mut b = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        b.extend_from_slice(&71u32.to_be_bytes());
        b.extend_from_slice(&95u32.to_be_bytes());
        assert_eq!(png_size(&b), Some((71, 95)));
        assert_eq!(png_size(b"not a png at all, sorry!"), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn scans_folders_with_adjust_files() {
        let png = |w: u32, h: u32| {
            let mut b = vec![
                0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
            ];
            b.extend_from_slice(&w.to_be_bytes());
            b.extend_from_slice(&h.to_be_bytes());
            b
        };
        let root = std::env::temp_dir().join(format!("bab_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Natural")).unwrap();
        std::fs::create_dir_all(root.join("Glass")).unwrap();
        std::fs::write(root.join("Natural/Onyx4mmRD.png"), png(47, 47)).unwrap();
        std::fs::write(root.join("Glass/Peridot8mmTD.PNG"), png(95, 140)).unwrap();
        std::fs::write(root.join("Glass/Peridot8mmTD.txt"), "h\n80 -3\nx\n12\n").unwrap();
        std::fs::write(root.join("Glass/readme.txt"), "not a bead").unwrap();

        let mut cat = BeadCatalog::new();
        assert_eq!(cat.add_folder(&disk::DiskFiles, &root), 2);
        assert_eq!(BeadCatalog::sub_folders(&disk::DiskFiles, &root).len(), 2);
        let pe = cat.find_by_name("peridot8mmtd").unwrap();
        assert_eq!(cat.width_of(pe), 80.0);
        assert_eq!(cat.get(pe).unwrap().folder, "Glass");
        assert_eq!(cat.get(0).unwrap().name, "Onyx4mmRD"); // sorted by name

        // Clasps: *_both.png with optional _left / _right halves.
        std::fs::create_dir_all(root.join("Clasps/Copper")).unwrap();
        std::fs::write(root.join("Clasps/Copper/Toggle_both.png"), png(237, 120)).unwrap();
        std::fs::write(root.join("Clasps/Copper/Toggle_left.png"), png(120, 120)).unwrap();
        std::fs::write(root.join("Clasps/Copper/Toggle_right.png"), png(125, 60)).unwrap();
        std::fs::write(root.join("Clasps/Copper/Lobster_Both.png"), png(150, 80)).unwrap();
        let clasps = disk::scan_clasps(&disk::DiskFiles, &root.join("Clasps"));
        assert_eq!(clasps.len(), 2);
        assert_eq!(clasps[0].name, "Lobster");
        assert!(clasps[0].left.is_none());
        assert_eq!(clasps[1].name, "Toggle");
        assert_eq!(clasps[1].folder, "Copper");
        assert_eq!(clasps[1].width_px(), 237.0);
        assert_eq!(clasps[1].right.as_ref().unwrap().width_px, 125);

        // Crimps: flat folder, no sub-folders.
        let mut crimps = BeadCatalog::new();
        assert_eq!(crimps.add_folder_flat(&disk::DiskFiles, &root.join("Glass")), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
