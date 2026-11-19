//! Program settings, saved as a plain-text INI file (`BunchABeads.ini`).
//!
//! The format is standard INI: `[Section]` headers and `key = value` lines.
//! Lines starting with `;` or `#` are comments, and unknown keys are ignored,
//! so the file can be edited by hand and new settings can be added later
//! without breaking older files. The 2013 `BaBPro.ini` layout is also read,
//! so old settings can be imported once.

/// Background colour of the design display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    White,
    Black,
    LightBlue,
    LightGreen,
    Custom([u8; 3]),
}

impl Background {
    pub const CHOICES: [Background; 4] = [
        Background::White,
        Background::Black,
        Background::LightBlue,
        Background::LightGreen,
    ];

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Background::White => [255, 255, 255],
            Background::Black => [0, 0, 0],
            Background::LightBlue => [173, 216, 230],
            Background::LightGreen => [144, 238, 144],
            Background::Custom(c) => c,
        }
    }

    pub fn label(self) -> String {
        match self {
            Background::White => "White".into(),
            Background::Black => "Black".into(),
            Background::LightBlue => "Light Blue".into(),
            Background::LightGreen => "Light Green".into(),
            Background::Custom([r, g, b]) => format!("#{r:02X}{g:02X}{b:02X}"),
        }
    }

    fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_lowercase().replace([' ', '_', '-'], "");
        match t.as_str() {
            "white" => Some(Background::White),
            "black" => Some(Background::Black),
            "lightblue" => Some(Background::LightBlue),
            "lightgreen" => Some(Background::LightGreen),
            _ => {
                let hex = t.strip_prefix('#')?;
                if hex.len() != 6 {
                    return None;
                }
                let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                Some(Background::Custom([byte(0)?, byte(2)?, byte(4)?]))
            }
        }
    }
}

/// Reads a background name ("White", "Light Blue", "#FFF8E7", ...).
pub fn parse_background(s: &str) -> Option<Background> {
    Background::parse(s)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // [User]
    pub user_name: String,
    pub email: String,
    // [Folders]  (empty = not chosen yet)
    /// Optional main folder holding Beads, Clasps, Crimps, Pendants, ... sub-folders.
    pub main_folder: String,
    pub bead_folder: String,
    /// Sub-folder shown in the bead list; empty = all sub-folders.
    pub current_bead_folder: String,
    pub clasp_folder: String,
    pub crimp_folder: String,
    pub pendant_folder: String,
    pub bookmarks_folder: String,
    pub images_folder: String,
    pub automatic_designs_folder: String,
    /// Stone pictures for the Bead Maker's Photo texture.
    pub textures_folder: String,
    // [Design]
    pub clasp: String,
    pub use_clasp: bool,
    pub crimp: String,
    pub crimps_per_end: u32,
    pub length_inches: f64,
    pub background: Background,
    pub show_grid: bool,
    /// Size of everything on screen, in percent; 0 = fit the screen automatically.
    pub screen_scale: u32,
    pub auto_refresh: bool,
    /// Draw the material list inside the circular design (screen and saved images).
    pub materials_on_picture: bool,
    // [Automatic]
    pub auto_min_beads: usize,
    pub auto_max_beads: usize,
    pub auto_symmetrical: bool,
    /// Colour scheme for automatic designs (see `colour::ColourScheme::key`).
    pub auto_colours: String,
    /// Currency symbol shown with prices.
    pub currency: String,
    // [Arc]
    /// true = Hanging view (drag the hands), false = 2013 arc view.
    pub arc_hanging: bool,
    /// Arc view: how much of a circle, in degrees (10-360).
    pub arc_angle: f64,
    /// Hanging view: where the hands hold the clasp ends, as fractions of the
    /// necklace length (x right, y down) from the middle.
    pub hand_left: (f64, f64),
    pub hand_right: (f64, f64),
}

/// Where the hands start in the Hanging view.
pub const DEFAULT_HAND_LEFT: (f64, f64) = (-0.3, 0.0);
pub const DEFAULT_HAND_RIGHT: (f64, f64) = (0.3, 0.0);

fn parse_pair(v: &str) -> Option<(f64, f64)> {
    let (a, b) = v.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            user_name: String::new(),
            email: String::new(),
            main_folder: String::new(),
            bead_folder: String::new(),
            current_bead_folder: String::new(),
            clasp_folder: String::new(),
            crimp_folder: String::new(),
            pendant_folder: String::new(),
            bookmarks_folder: String::new(),
            images_folder: String::new(),
            automatic_designs_folder: String::new(),
            textures_folder: String::new(),
            clasp: String::new(),
            use_clasp: true,
            crimp: String::new(),
            crimps_per_end: 2,
            length_inches: 18.0,
            background: Background::White,
            show_grid: true,
            screen_scale: 0,
            auto_refresh: false,
            materials_on_picture: true, // 2013 default
            auto_min_beads: 3,
            auto_max_beads: 7,
            auto_symmetrical: false,
            auto_colours: "any".into(),
            currency: "$".into(),
            arc_hanging: true,
            arc_angle: 200.0,
            hand_left: DEFAULT_HAND_LEFT,
            hand_right: DEFAULT_HAND_RIGHT,
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// File name without folders and without `_both` / extension, for clasp/crimp names
/// stored as full paths in the 2013 file.
fn name_from_path(v: &str) -> String {
    let file = v.rsplit(['\\', '/']).next().unwrap_or(v);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let lower = stem.to_lowercase();
    if lower.ends_with("_both") {
        stem[..stem.len() - 5].to_string()
    } else {
        stem.to_string()
    }
}

impl Settings {
    /// Reads settings text in either the new format or the 2013 `BaBPro.ini` layout.
    pub fn from_text(text: &str) -> Settings {
        if Self::is_legacy(text) {
            Self::from_legacy(text)
        } else {
            Self::from_ini(text)
        }
    }

    /// True if `text` looks like the 2013 layout ("[Key]" with the value on the next line).
    pub fn is_legacy(text: &str) -> bool {
        text.lines().any(|l| {
            let l = l.trim();
            l.eq_ignore_ascii_case("[Bead Folder]") || l.eq_ignore_ascii_case("[EOF]")
        })
    }

    fn from_ini(text: &str) -> Settings {
        let mut s = Settings::default();
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
            s.set(&section, &k, v);
        }
        s
    }

    fn set(&mut self, section: &str, key: &str, v: &str) {
        let text = || v.to_string();
        match (section, key) {
            ("user", "name") => self.user_name = text(),
            ("user", "email") => self.email = text(),
            ("folders", "main") => self.main_folder = text(),
            ("folders", "beads") => self.bead_folder = text(),
            ("folders", "current_beads") => self.current_bead_folder = text(),
            ("folders", "clasps") => self.clasp_folder = text(),
            ("folders", "crimps") => self.crimp_folder = text(),
            ("folders", "pendants") => self.pendant_folder = text(),
            ("folders", "bookmarks") => self.bookmarks_folder = text(),
            ("folders", "images") => self.images_folder = text(),
            ("folders", "automatic_designs") => self.automatic_designs_folder = text(),
            ("folders", "textures") => self.textures_folder = text(),
            ("design", "clasp") => self.clasp = text(),
            ("design", "use_clasp") => self.use_clasp = parse_bool(v).unwrap_or(self.use_clasp),
            ("design", "crimp") => self.crimp = text(),
            ("design", "crimps_per_end") => {
                self.crimps_per_end = v.parse::<u32>().map_or(self.crimps_per_end, |n| n.min(2))
            }
            ("design", "length_inches") => {
                self.length_inches = v.parse::<f64>().map_or(self.length_inches, |n| n.clamp(1.0, 36.0))
            }
            ("design", "background") => self.background = Background::parse(v).unwrap_or(self.background),
            ("design", "show_grid") => self.show_grid = parse_bool(v).unwrap_or(self.show_grid),
            ("screen", "scale") => {
                self.screen_scale = if v.eq_ignore_ascii_case("auto") {
                    0
                } else {
                    v.trim_end_matches('%')
                        .trim()
                        .parse::<u32>()
                        .map_or(self.screen_scale, |n| n.clamp(50, 200))
                }
            }
            ("design", "auto_refresh") => self.auto_refresh = parse_bool(v).unwrap_or(self.auto_refresh),
            ("design", "materials_on_picture") => {
                self.materials_on_picture = parse_bool(v).unwrap_or(self.materials_on_picture)
            }
            ("automatic", "min_beads") => {
                self.auto_min_beads = v.parse::<usize>().map_or(self.auto_min_beads, |n| n.clamp(1, 12))
            }
            ("automatic", "max_beads") => {
                self.auto_max_beads = v.parse::<usize>().map_or(self.auto_max_beads, |n| n.clamp(1, 12))
            }
            ("automatic", "symmetrical") => self.auto_symmetrical = parse_bool(v).unwrap_or(self.auto_symmetrical),
            ("automatic", "colours") => self.auto_colours = v.trim().to_string(),
            ("design", "currency") => self.currency = v.trim().to_string(),
            ("arc", "style") => self.arc_hanging = !v.trim().eq_ignore_ascii_case("arc"),
            ("arc", "angle") => self.arc_angle = v.parse::<f64>().map_or(self.arc_angle, |a| a.clamp(10.0, 360.0)),
            ("arc", "left_hand") => self.hand_left = parse_pair(v).unwrap_or(self.hand_left),
            ("arc", "right_hand") => self.hand_right = parse_pair(v).unwrap_or(self.hand_right),
            _ => {} // unknown keys are ignored
        }
    }

    /// Reads the 2013 layout: "[Key]" on one line, its value on the next, up to "[EOF]".
    pub fn from_legacy(text: &str) -> Settings {
        let mut s = Settings::default();
        let mut key: Option<String> = None;
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let k = line[1..line.len() - 1].trim().to_lowercase();
                if k == "eof" {
                    break;
                }
                key = Some(k);
                continue;
            }
            let Some(k) = key.take() else { continue };
            let v = line.to_string();
            match k.as_str() {
                "username" => s.user_name = v,
                "email address" => s.email = v,
                "bead folder" => s.bead_folder = v,
                "current bead folder" => s.current_bead_folder = v,
                "clasp folder" => s.clasp_folder = v,
                "crimp folder" => s.crimp_folder = v,
                "bookmarks folder" => s.bookmarks_folder = v,
                "images folder" => s.images_folder = v,
                "automaticdesigns folder" => s.automatic_designs_folder = v,
                "clasp" => s.clasp = name_from_path(&v),
                "crimp" => s.crimp = name_from_path(&v),
                _ => {} // e.g. the old registration number, no longer used
            }
        }
        // 2013 "Current Bead Folder" was a full path; the list filter only needs the sub-folder name,
        // and it means "all" when it is the main folder itself.
        if !s.current_bead_folder.is_empty() {
            let trimmed = s.current_bead_folder.trim_end_matches(['\\', '/']);
            if trimmed.eq_ignore_ascii_case(s.bead_folder.trim_end_matches(['\\', '/'])) {
                s.current_bead_folder.clear();
            } else {
                s.current_bead_folder = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).to_string();
            }
        }
        s
    }

    /// Writes the settings in the new INI format.
    pub fn to_ini(&self) -> String {
        let b = |v: bool| if v { "true" } else { "false" };
        format!(
            "; Bunch-a-Beads Pattern Designer settings\n\
             ; Plain text: you can edit this file while the program is closed.\n\
             ; Empty values mean \"not chosen yet\". Lines starting with ; are comments.\n\
             \n\
             [User]\n\
             name = {}\n\
             email = {}\n\
             \n\
             [Folders]\n\
             ; Optional main folder that holds Beads, Clasps, Crimps, Pendants, Bookmarks, Images\n\
             main = {}\n\
             beads = {}\n\
             ; Sub-folder shown in the bead list (empty = all sub-folders)\n\
             current_beads = {}\n\
             clasps = {}\n\
             crimps = {}\n\
             pendants = {}\n\
             bookmarks = {}\n\
             images = {}\n\
             automatic_designs = {}\n\
             textures = {}\n\
             \n\
             [Design]\n\
             ; Clasp and crimp are picture names without folder or extension\n\
             clasp = {}\n\
             use_clasp = {}\n\
             crimp = {}\n\
             ; 0, 1 or 2 crimp beads at each end\n\
             crimps_per_end = {}\n\
             length_inches = {}\n\
             ; White, Black, Light Blue, Light Green, or a colour like #FFF8E7\n\
             background = {}\n\
             show_grid = {}\n\
             auto_refresh = {}\n\
             materials_on_picture = {}\n\
             ; shown in front of prices in the material list and Inventory tab\n\
             currency = {}\n\
             \n\
             [Automatic]\n\
             min_beads = {}\n\
             max_beads = {}\n\
             symmetrical = {}\n\
             ; any, one colour, matching, contrasting, with neutrals, light to dark\n\
             colours = {}\n\
             \n\
             [Arc]\n\
             ; hanging (drag the hands) or arc (part of a circle, as in 2013)\n\
             style = {}\n\
             angle = {}\n\
             ; hand positions as fractions of the necklace length: x, y (y down)\n\
             left_hand = {}, {}\n\
             right_hand = {}, {}\n\
             \n\
             [Screen]\n\
             ; auto (fit the screen) or a size in percent, 50 to 200\n\
             scale = {}\n",
            self.user_name,
            self.email,
            self.main_folder,
            self.bead_folder,
            self.current_bead_folder,
            self.clasp_folder,
            self.crimp_folder,
            self.pendant_folder,
            self.bookmarks_folder,
            self.images_folder,
            self.automatic_designs_folder,
            self.textures_folder,
            self.clasp,
            b(self.use_clasp),
            self.crimp,
            self.crimps_per_end,
            self.length_inches,
            self.background.label(),
            b(self.show_grid),
            b(self.auto_refresh),
            b(self.materials_on_picture),
            self.currency,
            self.auto_min_beads,
            self.auto_max_beads,
            b(self.auto_symmetrical),
            self.auto_colours,
            if self.arc_hanging { "hanging" } else { "arc" },
            self.arc_angle,
            self.hand_left.0,
            self.hand_left.1,
            self.hand_right.0,
            self.hand_right.1,
            if self.screen_scale == 0 {
                "auto".to_string()
            } else {
                self.screen_scale.to_string()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = "; Initialization File for Bunch_a_BeadsPatternDesignPro\r\n[UserName]\r\nDavid Bunch\r\n\
        [Email Address]\r\nsomeone@example.com\r\n[Registration Number]\r\nXXXX-1234\r\n\
        [Bead Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Beads\r\n\
        [Current Bead Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Beads\\Natural\r\n\
        [Clasp Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Clasps\r\n\
        [Current Clasp Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Clasps\\Copper\r\n\
        [Crimp Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Crimps\r\n\
        [BookMarks Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\BookMarks\r\n\
        [Images Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Images\r\n\
        [AutoMaticDesigns Folder]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\AutoMaticDesigns\r\n\
        [Clasp]\r\nC:\\Bunch-a-BeadsPatternDesignPro\\Clasps\\Copper\\CopperToggle_both.png\r\n\
        [Crimp]\r\n\r\n[EOF]\r\n";

    #[test]
    fn imports_the_2013_file() {
        assert!(Settings::is_legacy(LEGACY));
        let s = Settings::from_text(LEGACY);
        assert_eq!(s.user_name, "David Bunch");
        assert_eq!(s.bead_folder, "C:\\Bunch-a-BeadsPatternDesignPro\\Beads");
        assert_eq!(s.current_bead_folder, "Natural");
        assert_eq!(s.clasp_folder, "C:\\Bunch-a-BeadsPatternDesignPro\\Clasps");
        assert_eq!(
            s.automatic_designs_folder,
            "C:\\Bunch-a-BeadsPatternDesignPro\\AutoMaticDesigns"
        );
        assert_eq!(s.clasp, "CopperToggle");
        assert_eq!(s.crimp, "");
        assert_eq!(s.crimps_per_end, 2); // defaults kept for things the old file didn't store
    }

    #[test]
    fn new_format_round_trips() {
        let mut s = Settings::from_legacy(LEGACY);
        s.background = Background::Custom([0xFF, 0xF8, 0xE7]);
        s.use_clasp = false;
        s.crimps_per_end = 1;
        s.length_inches = 16.5;
        s.main_folder = "D:\\BeadParts".into();
        s.pendant_folder = "D:\\BeadParts\\Pendants".into();
        s.auto_symmetrical = true;
        s.materials_on_picture = false;
        s.auto_colours = "matching".into();
        s.currency = "€".into();
        s.arc_hanging = false;
        s.arc_angle = 230.0;
        s.hand_left = (-0.25, 0.125);
        s.screen_scale = 80;
        let text = s.to_ini();
        assert!(!Settings::is_legacy(&text));
        assert_eq!(Settings::from_text(&text), s);
        s.screen_scale = 0;
        assert!(s.to_ini().contains("scale = auto"));
        assert_eq!(Settings::from_text(&s.to_ini()), s);
    }

    #[test]
    fn tolerant_parsing() {
        let s = Settings::from_text("[design]\nBackground = light blue\nlength_inches = 99\nnonsense\nunknown = 3\n");
        assert_eq!(s.background, Background::LightBlue);
        assert_eq!(s.length_inches, 36.0);
        assert_eq!(Settings::from_text(""), Settings::default());
    }
}
