//! The desktop window: tabs, the Main tab (laid out like the 2013 program),
//! the Options and Folders tabs, and drawing.

use std::path::{Path, PathBuf};

use crate::platform::{self, AppFiles, Filter, Jobs, Pick};
use beads_core::arc::{layout_arc, layout_hanging, open_length_px};
use beads_core::beadmaker::{self, BeadSpec, Material, Shape, PRESETS};
use beads_core::catalog::{self, disk::scan_clasps};
use beads_core::colour::{self as colours, BeadColour, ColourFamily, ColourScheme};
use beads_core::design::{
    build_strand, layout_circular, layout_linear, length_inches, material_list, wire_length_inches, LENGTH_PRESETS,
    WIRE_EXTRA_INCHES,
};
use beads_core::design_file::{self, SavedDesign};
use beads_core::inventory::{self, CostLine, Inventory, Stock};
use beads_core::manual::{ManualDesign, ManualItem};
use beads_core::pattern::{arrange_random, arrange_symmetric, auto_random, auto_symmetric, move_item};
use beads_core::render::{self, Fit, Rgba};
use beads_core::{
    Background, BeadCatalog, ClaspInfo, Design, DesignMode, Parts, Pattern, Placed, Rng, Settings, MAX_PATTERNS,
    PATTERN_SIZE,
};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::images::Images;
use crate::settings_file;
use crate::text::TextPainter;

// Button colours from the 2013 program, grouped by what they do.
const PATTERN_BLUE: Color32 = Color32::from_rgb(173, 216, 230);
const DISPLAY_GREEN: Color32 = Color32::from_rgb(170, 230, 170);
const AUTO_GOLD: Color32 = Color32::from_rgb(240, 210, 90);
const FILE_TAN: Color32 = Color32::from_rgb(222, 196, 160);
const SELECTED_GOLD: Color32 = Color32::from_rgb(255, 230, 120);

const SLOT_PX: f32 = 64.0;
/// Colours that tell patterns 1-5 apart on the Edit Patterns tab.
const PATTERN_COLOURS: [Color32; MAX_PATTERNS] = [
    Color32::from_rgb(60, 100, 200),
    Color32::from_rgb(50, 150, 60),
    Color32::from_rgb(220, 120, 20),
    Color32::from_rgb(140, 70, 180),
    Color32::from_rgb(20, 150, 150),
];
/// Edit Patterns item blocks: 300-dpi pixels -> screen pixels.
const BLOCK_SCALE: f32 = 0.3;
const BLOCK_H: f32 = 40.0;
const UNDO_LIMIT: usize = 100;
const SLIDE_SECONDS: f64 = 3.0;

/// Borrow the three part lists as one `Parts` without borrowing all of `self`.
macro_rules! parts {
    ($s:ident) => {
        Parts {
            beads: &$s.catalog,
            crimps: &$s.crimps,
            clasps: &$s.clasps,
            pendants: &$s.pendants,
        }
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Main,
    EditPatterns,
    Circular,
    Linear,
    Arc,
    Materials,
    Inventory,
    Options,
    Folders,
    Pendants,
    Bookmarks,
    BeadMaker,
}

const TABS: [(Tab, &str); 12] = [
    (Tab::Main, "Main"),
    (Tab::EditPatterns, "Edit Patterns"),
    (Tab::Circular, "Circular Design"),
    (Tab::Linear, "Linear Design"),
    (Tab::Arc, "Arc Design"),
    (Tab::Materials, "Material List"),
    (Tab::Inventory, "Inventory"),
    (Tab::Options, "Options"),
    (Tab::Folders, "Bead & Clasp Folders"),
    (Tab::Pendants, "Pendants"),
    (Tab::Bookmarks, "Bookmarks"),
    (Tab::BeadMaker, "Bead Maker"),
];

/// The folders listed on the "Bead & Clasp Folders" tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FolderKind {
    Main,
    Beads,
    Clasps,
    Crimps,
    Pendants,
    Bookmarks,
    Images,
    AutomaticDesigns,
    Textures,
}

/// The screen size the program is laid out for (in points at 100%).
const LAYOUT_SIZE: Vec2 = Vec2::new(1280.0, 820.0);
/// "Fit to screen" never makes things smaller than this (text stays readable).
const MIN_AUTO_SCALE: f32 = 0.6;
/// Screens smaller than this (after scaling) scroll instead.
const SCROLL_BELOW: Vec2 = Vec2::new(1100.0, 700.0);
/// The page size to scroll around on a very small screen.
const SMALL_SCREEN_PAGE: Vec2 = Vec2::new(860.0, 640.0);
/// Narrower than this, the Main tab hides its material list panel.
const NARROW_SCREEN: f32 = 1000.0;

/// Picture files the Bead Maker can use as a stone texture.
const PICTURE_TYPES: &[&str] = &["png", "jpg", "jpeg", "bmp", "webp"];

const FOLDERS: [(FolderKind, &str); 9] = [
    (FolderKind::Main, "Main Folder"),
    (FolderKind::Beads, "Beads"),
    (FolderKind::Clasps, "Clasps"),
    (FolderKind::Crimps, "Crimps"),
    (FolderKind::Pendants, "Pendants"),
    (FolderKind::Bookmarks, "Bookmarks"),
    (FolderKind::Images, "Images"),
    (FolderKind::AutomaticDesigns, "Automatic Designs"),
    (FolderKind::Textures, "Textures"),
];

/// Sub-folder names looked for inside the Main Folder (compared ignoring case).
fn main_subfolder_names(kind: FolderKind) -> &'static [&'static str] {
    match kind {
        FolderKind::Main => &[],
        FolderKind::Beads => &["Beads"],
        FolderKind::Clasps => &["Clasps"],
        FolderKind::Crimps => &["Crimps"],
        FolderKind::Pendants => &["Pendants"],
        FolderKind::Bookmarks => &["Bookmarks"],
        FolderKind::Images => &["Images"],
        FolderKind::AutomaticDesigns => &["AutomaticDesigns", "Automatic Designs"],
        FolderKind::Textures => &["Textures"],
    }
}

/// Which Add/Sub row was clicked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Bottom,
}

/// What clicking the design does on the Edit Patterns tab (the 2013 Insert/Replace/... buttons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditTool {
    None,
    InsertPattern(usize),
    InsertBead,
    ReplacePattern(usize),
    ReplaceBead,
    Delete,
    Split,
}

/// One step in the undo history: what the change was, and the whole design
/// as it was just before it.
struct UndoEntry {
    label: String,
    design: Design,
}

/// What to do after the "save changes?" question is answered.
#[derive(Clone, PartialEq)]
enum AfterConfirm {
    Quit,
    New,
    Open(PathBuf),
}

/// Things the user asked for this frame; applied after the UI is drawn.
enum Action {
    ChooseFolder(FolderKind),
    SelectIntoSlot(usize),
    RemoveSlot(usize),
    SlotClicked(usize),
    /// Ctrl+click (or Select mode): select or deselect a pattern bead for shifting.
    SlotToggleSelect(usize),
    /// A pattern bead dragged from one place to another.
    MoveSlot(usize, usize),
    ShiftLeft,
    ShiftRight,
    SelectAll,
    DeselectAll,
    Reorder,
    Swap(usize, usize),
    Duplicate(usize, usize),
    ClearPattern(usize),
    ClearAll,
    PrintPdf,
    ToggleAbout,
    SetLength(f64),
    Undo,
    Redo,
    /// Step back (or forward) this many steps, from the History window.
    UndoSteps(usize),
    RedoSteps(usize),
    ToggleHistory,
    Draw,
    /// A setting that changes the drawing (mode, length): redraw if Auto Refresh is on.
    SettingsChanged,
    /// Clasp, crimp or background changed: save and redraw.
    OptionsChanged,
    AutoPattern,
    SaveImage,
    Adjust(Side, i32),
    EditPattern(usize),
    /// Edit pattern N on the Main tab (from the Edit Patterns tab).
    GoEditPattern(usize),
    BackToEditTab,
    UseManual,
    ClearManual,
    ManCopyMain,
    ManAuto5,
    ManSplitAll,
    ManAddPattern(usize),
    ManRemovePattern(usize),
    ManAddBead,
    ManTool(EditTool),
    ManClickGap(usize),
    ManClickItem(usize),
    ManDelete(usize),
    ManSplit(usize),
    ManFlip(usize),
    ManMove(usize, bool),
    New,
    Open,
    Save,
    SaveAs,
    QuickBookmark,
    OpenFile(PathBuf),
    PreviewBookmark(usize),
    RefreshBookmarks,
    /// Bead Maker: save the bead being made into the Beads folder.
    MakerSave,
    /// Bead Maker: make the whole sample library (in the background).
    MakerLibrary,
    /// Bead Maker: choose the picture for the Photo texture material.
    MakerChoosePhoto,
    /// Bead Maker: use this picture from the Textures folder.
    MakerUsePhoto(PathBuf),
    Quit,
}

pub struct BeadApp {
    settings: Settings,
    settings_path: PathBuf,

    catalog: BeadCatalog,
    crimps: BeadCatalog,
    /// Pendants: from the Pendants folder, or beads with "Pendant" in their name.
    pendants: BeadCatalog,
    pendant_filter: String,
    clasps: Vec<ClaspInfo>,
    bead_subfolders: Vec<String>,
    images: Images,
    list_selection: Option<usize>,
    filter: String,

    design: Design,
    current_pat: usize,
    selected: [bool; PATTERN_SIZE],
    select_mode: bool,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    /// Name of the change about to be recorded (set from the action being applied).
    undo_label: String,
    /// Label of the last step if it can absorb more of the same change (e.g.
    /// dragging the length), so one drag is one undo step.
    merge_label: Option<String>,
    /// The design at the start of this frame, for changes made directly by controls.
    frame_start: Design,
    show_history: bool,
    show_about: bool,
    /// Bead colours worked out from the photos (filled in by a background thread).
    colours: Vec<Option<BeadColour>>,
    colour_rx: Option<Jobs<(u64, usize, Option<BeadColour>)>>,
    colour_generation: u64,
    colours_done: usize,
    /// Bead Maker: the bead being made, its name, the last preset picked, and
    /// the preview picture (redrawn whenever the settings change).
    maker: BeadSpec,
    maker_name: String,
    maker_preset: Option<usize>,
    maker_texture: Option<egui::TextureHandle>,
    maker_drawn: Option<BeadSpec>,
    /// Photo texture: the stone picture wrapped round the bead, and which one the preview used.
    maker_photo_path: Option<PathBuf>,
    maker_photo: Option<Rgba>,
    maker_drawn_photo: Option<PathBuf>,
    /// Pictures in the Textures folder (None = look again).
    maker_texture_list: Option<Vec<PathBuf>>,
    /// The screen scale (zoom) last set, so it is only changed when needed.
    applied_zoom: f32,
    /// Sample library being written in the background: Ok(true) = written,
    /// Ok(false) = already there, Err = could not write.
    maker_rx: Option<Jobs<Result<bool, String>>>,
    maker_total: usize,
    maker_done: usize,
    maker_written: usize,
    maker_errors: Vec<String>,
    colour_filter: Option<ColourFamily>,
    /// What you have and what it costs.
    inventory: Inventory,
    inventory_path: PathBuf,
    inventory_filter: String,
    inventory_only_design: bool,
    /// The material list with stock and prices.
    cost_lines: Vec<CostLine>,
    /// The material list as lines, for the picture and the PDF design sheet.
    material_lines: Vec<String>,
    /// Font for drawing text into saved pictures (loaded the first time it's needed).
    text_painter: Option<TextPainter>,
    /// Arc tab: which hand is being dragged (0 = left, 1 = right).
    arc_drag: Option<usize>,
    /// Arc tab, Hanging view: magnification (1 = the size the 2013 arc view
    /// would show), the view centre (fractions of the necklace length, None =
    /// automatic), and whether the background is being dragged to move the view.
    hang_zoom: f64,
    hang_center: Option<(f64, f64)>,
    arc_pan: bool,
    /// Edit Patterns tab: add/insert patterns mirrored, the active tool, and
    /// which kind of Main design "Copy Main Design" starts from.
    edit_mirrored: bool,
    edit_tool: EditTool,
    copy_from_mode: DesignMode,
    /// The Main tab was opened from Edit Patterns: show a "Back" button.
    came_from_edit_tab: bool,

    tab: Tab,
    slide_show: bool,
    next_slide_at: f64,
    rng: Rng,

    circular: Vec<Placed>,
    /// Linear Design magnify, in percent (100 = whole strand fits the window width).
    linear_zoom: f64,
    linear: Vec<Placed>,
    materials: String,
    status: String,

    /// File the current design was opened from / saved to (None = not saved yet).
    current_file: Option<PathBuf>,
    /// Name shown in the title for an imported 2013 file (saving makes a new file).
    imported_name: Option<String>,
    /// The design has changes that aren't saved.
    dirty: bool,
    confirm: Option<AfterConfirm>,
    allow_close: bool,
    title: String,

    bookmarks: Vec<PathBuf>,
    bookmark_filter: String,
    bookmark_selected: Option<usize>,
    bookmark_preview: Vec<Placed>,
    bookmark_info: String,
    bookmarks_loaded_from: String,
}

impl BeadApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let seed = random_seed();
        let settings_path = settings_file::settings_path();
        let (mut settings, status) = settings_file::load(&settings_path);

        // The first test version remembered the bead folder inside eframe's own storage.
        if settings.bead_folder.is_empty() {
            if let Some(old) = cc.storage.and_then(|s| s.get_string("bead_folder")) {
                settings.bead_folder = old;
            }
        }
        // Still nothing: a "Beads" folder next to the program.
        if settings.bead_folder.is_empty() {
            if let Some(b) = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("Beads")))
            {
                if platform::is_dir(&b) {
                    settings.bead_folder = b.to_string_lossy().into_owned();
                }
            }
        }

        let design = Design {
            target_length_inches: settings.length_inches,
            use_clasp: settings.use_clasp,
            crimps_per_end: settings.crimps_per_end,
            ..Design::default()
        };

        let mut app = BeadApp {
            settings,
            settings_path,
            catalog: BeadCatalog::new(),
            crimps: BeadCatalog::new(),
            pendants: BeadCatalog::new(),
            pendant_filter: String::new(),
            clasps: Vec::new(),
            bead_subfolders: Vec::new(),
            images: Images::default(),
            list_selection: None,
            filter: String::new(),
            design,
            current_pat: 0,
            selected: [false; PATTERN_SIZE],
            select_mode: false,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_label: String::new(),
            merge_label: None,
            frame_start: Design::default(),
            show_history: false,
            show_about: false,
            colours: Vec::new(),
            colour_rx: None,
            colour_generation: 0,
            colours_done: 0,
            maker: BeadSpec::default(),
            maker_name: "RedJasper".into(),
            maker_preset: PRESETS.iter().position(|p| p.name == "RedJasper"),
            maker_texture: None,
            maker_drawn: None,
            maker_photo_path: None,
            maker_photo: None,
            maker_drawn_photo: None,
            maker_texture_list: None,
            applied_zoom: 0.0,
            maker_rx: None,
            maker_total: 0,
            maker_done: 0,
            maker_written: 0,
            maker_errors: Vec::new(),
            colour_filter: None,
            inventory: Inventory::default(),
            inventory_path: PathBuf::new(),
            inventory_filter: String::new(),
            inventory_only_design: false,
            cost_lines: Vec::new(),
            material_lines: Vec::new(),
            text_painter: None,
            arc_drag: None,
            hang_zoom: 1.0,
            hang_center: None,
            arc_pan: false,
            edit_mirrored: false,
            edit_tool: EditTool::None,
            copy_from_mode: DesignMode::OnePatternLinear,
            came_from_edit_tab: false,
            tab: Tab::Main,
            slide_show: false,
            next_slide_at: 0.0,
            rng: Rng::new(seed),
            circular: Vec::new(),
            linear_zoom: 100.0,
            linear: Vec::new(),
            materials: String::new(),
            status,
            current_file: None,
            imported_name: None,
            dirty: false,
            confirm: None,
            allow_close: false,
            title: String::new(),
            bookmarks: Vec::new(),
            bookmark_filter: String::new(),
            bookmark_selected: None,
            bookmark_preview: Vec::new(),
            bookmark_info: String::new(),
            bookmarks_loaded_from: String::new(),
        };
        app.guess_missing_folders();
        app.load_beads();
        app.load_clasps();
        app.load_crimps();
        app.load_pendants();
        app.inventory_path = app.settings_path.with_file_name("Inventory.csv");
        app.inventory = platform::read_to_string(&app.inventory_path)
            .map(|t| Inventory::from_csv(&t))
            .unwrap_or_default();
        app.save_settings();
        app.draw_design();
        if platform::IS_WEB {
            platform::start_sample_downloads(&cc.egui_ctx);
            app.status = "Welcome! Loading the sample bead photos…".into();
        }
        app
    }

    // ----- settings & folders ------------------------------------------------------

    /// If clasp/crimp folders aren't set, look for "Clasps" / "Crimps" next to the Beads folder.
    fn guess_missing_folders(&mut self) {
        let Some(root) = Path::new(&self.settings.bead_folder).parent().map(Path::to_path_buf) else {
            return;
        };
        for (field, name) in [
            (&mut self.settings.clasp_folder, "Clasps"),
            (&mut self.settings.crimp_folder, "Crimps"),
        ] {
            if field.is_empty() && platform::is_dir(&root.join(name)) {
                *field = root.join(name).to_string_lossy().into_owned();
            }
        }
    }

    fn save_settings(&mut self) {
        let s = &mut self.settings;
        s.length_inches = self.design.target_length_inches;
        s.use_clasp = self.design.use_clasp;
        s.crimps_per_end = self.design.crimps_per_end;
        if let Some(c) = self.design.clasp.and_then(|i| self.clasps.get(i)) {
            s.clasp = c.name.clone();
        }
        if let Some(c) = self.design.crimp.and_then(|i| self.crimps.get(i)) {
            s.crimp = c.name.clone();
        }
        if let Err(e) = settings_file::save(&self.settings_path, &self.settings) {
            self.status = format!("Could not save settings to {}: {e}", self.settings_path.display());
        }
    }

    fn folder_mut(&mut self, kind: FolderKind) -> &mut String {
        let s = &mut self.settings;
        match kind {
            FolderKind::Main => &mut s.main_folder,
            FolderKind::Beads => &mut s.bead_folder,
            FolderKind::Clasps => &mut s.clasp_folder,
            FolderKind::Crimps => &mut s.crimp_folder,
            FolderKind::Pendants => &mut s.pendant_folder,
            FolderKind::Bookmarks => &mut s.bookmarks_folder,
            FolderKind::Images => &mut s.images_folder,
            FolderKind::AutomaticDesigns => &mut s.automatic_designs_folder,
            FolderKind::Textures => &mut s.textures_folder,
        }
    }

    fn choose_folder(&mut self, kind: FolderKind) {
        let current = self.folder_mut(kind).clone();
        let Some(folder) = platform::pick_folder(&current) else {
            if platform::IS_WEB {
                self.status = "The web version's folders are built in.".into();
            }
            return;
        };
        *self.folder_mut(kind) = folder.to_string_lossy().into_owned();
        match kind {
            FolderKind::Main => {
                let found = self.use_main_folder(&folder);
                self.settings.current_bead_folder.clear();
                self.load_beads();
                self.load_clasps();
                self.load_crimps();
                self.load_pendants();
                self.status = format!("Main folder {}: found {}.", folder.display(), found.join(", "));
            }
            FolderKind::Beads => {
                self.settings.current_bead_folder.clear();
                self.guess_missing_folders();
                self.load_beads();
                self.load_clasps();
                self.load_crimps();
                self.load_pendants();
            }
            FolderKind::Clasps => self.load_clasps(),
            FolderKind::Crimps => self.load_crimps(),
            FolderKind::Pendants => self.load_pendants(),
            FolderKind::Textures => self.maker_texture_list = None,
            _ => {}
        }
        self.save_settings();
        self.draw_design();
    }

    /// Fills in every folder that exists inside `main` (Beads, Clasps, ...).
    /// Bookmarks and Images are created there if missing. Returns what was found.
    fn use_main_folder(&mut self, main: &Path) -> Vec<String> {
        let subdirs: Vec<PathBuf> = platform::list(main).into_iter().filter(|e| e.1).map(|e| e.0).collect();
        let mut found = Vec::new();
        for (kind, label) in FOLDERS {
            let names = main_subfolder_names(kind);
            if names.is_empty() {
                continue;
            }
            let hit = subdirs.iter().find(|d| {
                d.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| names.iter().any(|want| n.eq_ignore_ascii_case(want)))
            });
            let dir = match (hit, kind) {
                (Some(d), _) => Some(d.clone()),
                (None, FolderKind::Bookmarks | FolderKind::Images | FolderKind::Textures) => {
                    let d = main.join(names[0]);
                    platform::create_dir_all(&d).ok().map(|_| d)
                }
                _ => None,
            };
            if let Some(d) = dir {
                *self.folder_mut(kind) = d.to_string_lossy().into_owned();
                found.push(label.to_string());
            }
        }
        if found.is_empty() {
            found.push("no known sub-folders".into());
        }
        found
    }

    fn load_beads(&mut self) {
        self.catalog.clear();
        self.images.clear();
        self.design.patterns = [Pattern::default(); MAX_PATTERNS];
        self.design.manual.clear(); // bead numbers change when the folder is re-read
        self.design.left_adjust = 0;
        self.design.bottom_adjust = 0;
        self.undo.clear();
        self.redo.clear();
        self.list_selection = None;
        let folder = PathBuf::from(&self.settings.bead_folder);
        if self.settings.bead_folder.is_empty() || !platform::is_dir(&folder) {
            return;
        }
        let n = self.catalog.add_folder(&AppFiles, &folder);
        let mut subs: Vec<String> = self.catalog.all().iter().map(|b| b.folder.clone()).collect();
        subs.sort_by_key(|s| s.to_lowercase());
        subs.dedup();
        self.bead_subfolders = subs;
        if !self.bead_subfolders.contains(&self.settings.current_bead_folder) {
            self.settings.current_bead_folder.clear();
        }
        self.status = format!("{n} beads loaded from {}", folder.display());
        self.start_colour_analysis();
        self.draw_design();
    }

    /// Works out every bead's colour from its photo, in the background so the
    /// program stays responsive. Results arrive in `update()`.
    fn start_colour_analysis(&mut self) {
        self.colour_generation += 1;
        let generation = self.colour_generation;
        self.colours = vec![None; self.catalog.len()];
        self.colours_done = 0;
        let jobs: Vec<(usize, PathBuf)> = self
            .catalog
            .all()
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.image_path.clone().map(|p| (i, p)))
            .collect();
        let jobs = jobs
            .into_iter()
            .map(|(i, path)| -> platform::Job<(u64, usize, Option<BeadColour>)> {
                Box::new(move || {
                    let colour = platform::read(&path)
                        .ok()
                        .and_then(|bytes| image::load_from_memory(&bytes).ok())
                        .and_then(|img| {
                            let rgba = img.to_rgba8();
                            colours::analyse(rgba.as_raw(), rgba.width(), rgba.height())
                        });
                    (generation, i, colour)
                })
            })
            .collect();
        // Starting a new run drops the old one, which stops it.
        self.colour_rx = Some(Jobs::start(jobs));
    }

    /// Collects finished colour results (called every frame).
    fn receive_colours(&mut self, ctx: &egui::Context) {
        let Some(jobs) = &mut self.colour_rx else { return };
        let mut got = false;
        for (generation, i, colour) in jobs.poll() {
            if generation == self.colour_generation {
                if let Some(slot) = self.colours.get_mut(i) {
                    *slot = colour;
                }
                self.colours_done += 1;
                got = true;
            }
        }
        if self.colours_done >= self.colours.len() {
            self.colour_rx = None;
            if got {
                self.status = format!("Colours worked out for {} beads.", self.colours.len());
            }
        } else if platform::IS_WEB {
            ctx.request_repaint(); // the web does the work between frames, so keep going
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn save_inventory(&mut self) {
        if let Some(dir) = self.inventory_path.parent() {
            let _ = platform::create_dir_all(dir);
        }
        if let Err(e) = platform::write(&self.inventory_path, self.inventory.to_csv()) {
            self.status = format!("Could not save {}: {e}", self.inventory_path.display());
        }
    }

    fn load_clasps(&mut self) {
        self.clasps = if self.settings.clasp_folder.is_empty() {
            Vec::new()
        } else {
            scan_clasps(&AppFiles, Path::new(&self.settings.clasp_folder))
        };
        let want = self.settings.clasp.to_lowercase();
        self.design.clasp = self
            .clasps
            .iter()
            .position(|c| c.name.to_lowercase() == want)
            .or(if self.clasps.is_empty() { None } else { Some(0) });
    }

    fn load_crimps(&mut self) {
        self.crimps.clear();
        if !self.settings.crimp_folder.is_empty() {
            self.crimps
                .add_folder_flat(&AppFiles, Path::new(&self.settings.crimp_folder));
        }
        self.design.crimp = self
            .crimps
            .find_by_name(&self.settings.crimp)
            .or(if self.crimps.is_empty() { None } else { Some(0) });
    }

    /// Pendants come from the Pendants folder (and its sub-folders). If none is set,
    /// beads with "pendant" in their name are used, as in the 2013 bead folders.
    fn load_pendants(&mut self) {
        let previous = self
            .design
            .pendant
            .and_then(|i| self.pendants.get(i))
            .map(|p| p.name.clone());
        self.pendants.clear();
        let folder = Path::new(&self.settings.pendant_folder);
        if !self.settings.pendant_folder.is_empty() && platform::is_dir(folder) {
            self.pendants.add_folder(&AppFiles, folder);
        } else {
            for b in self.catalog.all() {
                if b.name.to_lowercase().contains("pendant") {
                    self.pendants.push(b.clone());
                }
            }
            self.pendants.sort_by_name();
        }
        self.design.pendant = previous.and_then(|n| self.pendants.find_by_name(&n));
        if self.design.pendant.is_none() {
            self.design.use_pendant = false;
        }
    }

    /// Changes where the chosen pendant hangs (pixels from the top of its picture).
    fn set_pendant_hang(&mut self, index: usize, hang_from_top: f64) {
        let Some(p) = self.pendants.get_mut(index) else { return };
        let drop = (p.height_px as f64 / 2.0 - hang_from_top).round() as i32;
        // 0 means "not set" in BeadGuide files, so nudge an exact centre by one pixel.
        p.adjust.y_offset_px = if drop == 0 { 1 } else { drop };
        // The 2013 program drew from the picture's left edge: a shift of minus half the
        // width keeps the pendant centred there too, if the file is used by it.
        if p.adjust.x_shift_px == 0 {
            p.adjust.x_shift_px = -((p.width_px as i32 + 1) / 2);
        }
        let (name, adjust) = (p.name.clone(), p.adjust);
        // Keep the same bead in the bead list in step (when pendants come from there).
        if let Some(b) = self.catalog.find_by_name(&name).and_then(|i| self.catalog.get_mut(i)) {
            b.adjust = adjust;
        }
    }

    /// Writes the pendant's hang point to its BeadGuide .txt file (next to the picture).
    fn save_pendant_adjust(&mut self, index: usize) {
        let Some(p) = self.pendants.get(index) else { return };
        let Some(image) = p.image_path.clone() else { return };
        let txt = image.with_extension("txt");
        let existing = platform::read_to_string(&txt).unwrap_or_default();
        let file_name = image
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = p.adjust.to_text(&existing, &file_name, p.height_px);
        self.status = match platform::write(&txt, text) {
            Ok(()) => format!("Saved hang point to {}", txt.display()),
            Err(e) => format!("Could not save {}: {e}", txt.display()),
        };
    }

    /// Re-reads the pendant's .txt file, undoing unsaved hang-point changes.
    fn reload_pendant_adjust(&mut self, index: usize) {
        let Some(p) = self.pendants.get_mut(index) else { return };
        let Some(image) = p.image_path.clone() else { return };
        p.adjust = platform::read_to_string(&image.with_extension("txt"))
            .map(|t| beads_core::BeadAdjust::parse(&t))
            .unwrap_or_default();
        let (name, adjust) = (p.name.clone(), p.adjust);
        if let Some(b) = self.catalog.find_by_name(&name).and_then(|i| self.catalog.get_mut(i)) {
            b.adjust = adjust;
        }
    }

    // ----- editing -----------------------------------------------------------------

    /// Chooses the pattern to edit, which is also the one the 1-pattern modes draw.
    fn select_pattern(&mut self, p: usize) {
        self.current_pat = p.min(MAX_PATTERNS - 1);
        self.selected = [false; PATTERN_SIZE];
        if self.design.active_pattern != self.current_pat {
            self.design.active_pattern = self.current_pat;
            if matches!(
                self.design.mode,
                DesignMode::OnePatternLinear | DesignMode::OnePatternMirrored
            ) {
                self.draw_design();
            }
        }
    }

    fn pattern_mut(&mut self) -> &mut Pattern {
        &mut self.design.patterns[self.current_pat]
    }

    /// Records `before` as the state to return to when the change named in
    /// `undo_label` is undone. With `merge`, a repeat of the same change (like
    /// dragging the length) stays one step.
    fn push_entry(&mut self, before: Design, merge: bool) {
        let label = if self.undo_label.is_empty() {
            "Change".to_string()
        } else {
            self.undo_label.clone()
        };
        if merge && self.redo.is_empty() && self.merge_label.as_deref() == Some(label.as_str()) {
            return;
        }
        self.undo.push(UndoEntry {
            label: label.clone(),
            design: before,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.merge_label = merge.then_some(label);
    }

    /// Records the current design before changing it.
    fn push_undo(&mut self) {
        let before = self.design.clone();
        self.push_entry(before, false);
    }

    fn undo_once(&mut self) -> Option<String> {
        let entry = self.undo.pop()?;
        let now = std::mem::replace(&mut self.design, entry.design);
        self.redo.push(UndoEntry {
            label: entry.label.clone(),
            design: now,
        });
        Some(entry.label)
    }

    fn redo_once(&mut self) -> Option<String> {
        let entry = self.redo.pop()?;
        let now = std::mem::replace(&mut self.design, entry.design);
        self.undo.push(UndoEntry {
            label: entry.label.clone(),
            design: now,
        });
        Some(entry.label)
    }

    /// After undo/redo: follow the restored design's selected pattern and redraw.
    fn after_history_move(&mut self, what: &str, label: Option<String>) {
        self.merge_label = None;
        self.current_pat = self.design.active_pattern;
        self.selected = [false; PATTERN_SIZE];
        self.draw_design();
        if let Some(l) = label {
            self.status = format!("{what}: {l}");
        }
    }

    /// A short name for an action, shown in the History list.
    fn action_label(&self, action: &Action) -> String {
        use Action::*;
        let pat = self.current_pat + 1;
        match action {
            SelectIntoSlot(s) => {
                let bead = self
                    .list_selection
                    .and_then(|b| self.catalog.get(b))
                    .map_or("bead", |b| b.name.as_str());
                format!("Pattern {pat}: {bead} into slot {}", s + 1)
            }
            RemoveSlot(s) => format!("Pattern {pat}: remove slot {}", s + 1),
            MoveSlot(from, to) => format!("Pattern {pat}: move bead {} to {}", from + 1, to + 1),
            ShiftLeft => format!("Pattern {pat}: shift left"),
            ShiftRight => format!("Pattern {pat}: shift right"),
            Reorder => format!("Pattern {pat}: reorder"),
            Swap(a, b) => format!("Swap patterns {} and {}", a + 1, b + 1),
            Duplicate(a, b) => format!("Copy pattern {} to {}", a + 1, b + 1),
            ClearPattern(p) => format!("Clear pattern {}", p + 1),
            ClearAll => "Clear everything".into(),
            SettingsChanged => "Change length or pattern mode".into(),
            OptionsChanged => "Change clasp, crimp or pendant".into(),
            AutoPattern => format!("Automatic pattern {pat}"),
            Adjust(side, d) => format!(
                "{} {}",
                match side {
                    Side::Left => "Left",
                    Side::Right => "Right",
                    Side::Bottom => "Bottom",
                },
                if *d > 0 { "+1" } else { "-1" }
            ),
            UseManual => "Show manual edit design".into(),
            ClearManual => "Manual: clear design".into(),
            ManCopyMain => "Manual: copy main design".into(),
            ManAuto5 => "Automatic patterns 1-5".into(),
            ManSplitAll => "Manual: split all apart".into(),
            ManAddPattern(p) => format!("Manual: add pattern {}", p + 1),
            ManRemovePattern(p) => format!("Manual: remove pattern {}", p + 1),
            ManAddBead => "Manual: add bead".into(),
            ManClickGap(_) => "Manual: insert".into(),
            ManClickItem(_) => match self.edit_tool {
                EditTool::Delete => "Manual: delete".into(),
                EditTool::Split => "Manual: split pattern".into(),
                _ => "Manual: replace".into(),
            },
            ManDelete(_) => "Manual: delete".into(),
            ManSplit(_) => "Manual: split pattern".into(),
            ManFlip(_) => "Manual: flip".into(),
            ManMove(_, left) => format!("Manual: move {}", if *left { "left" } else { "right" }),
            _ => "Change".into(),
        }
    }

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        use Action::*;
        if !matches!(
            action,
            Undo | Redo | UndoSteps(_) | RedoSteps(_) | ToggleHistory | ToggleAbout | PrintPdf
        ) {
            self.undo_label = self.action_label(&action);
        }
        let edited = matches!(
            action,
            SelectIntoSlot(_)
                | RemoveSlot(_)
                | MoveSlot(..)
                | ShiftLeft
                | ShiftRight
                | Reorder
                | Swap(..)
                | Duplicate(..)
                | ClearPattern(_)
                | ClearAll
                | Undo
                | Redo
        );
        if edited
            || matches!(
                action,
                SettingsChanged | OptionsChanged | AutoPattern | Adjust(..) | UndoSteps(_) | RedoSteps(_)
            )
        {
            self.dirty = true;
        }
        match action {
            ChooseFolder(kind) => self.choose_folder(kind),
            SelectIntoSlot(slot) => match self.list_selection {
                Some(bead) => {
                    self.push_undo();
                    self.pattern_mut().set(slot, bead);
                }
                None => self.status = "Pick a bead from the list first.".into(),
            },
            RemoveSlot(slot) => {
                self.push_undo();
                self.pattern_mut().remove(slot);
            }
            SlotClicked(slot) => {
                if self.select_mode {
                    self.selected[slot] = !self.selected[slot];
                } else if let Some(bead) = self.design.patterns[self.current_pat].get(slot) {
                    self.list_selection = Some(bead); // reuse this bead elsewhere
                }
            }
            SlotToggleSelect(slot) => {
                self.selected[slot] = !self.selected[slot];
                if self.selected.iter().any(|&x| x) {
                    self.select_mode = true;
                }
            }
            MoveSlot(from, to) => {
                self.push_undo();
                self.pattern_mut().move_slot(from, to);
                move_item(&mut self.selected, from, to); // the selection moves with the bead
            }
            ShiftLeft | ShiftRight => {
                self.push_undo();
                let right = matches!(action, ShiftRight);
                if self.selected.iter().any(|&x| x) {
                    let mut sel = self.selected;
                    self.pattern_mut().shift_selected(&mut sel, right);
                    self.selected = sel;
                } else {
                    // Nothing selected: move the whole pattern round by one bead.
                    self.pattern_mut().rotate(right);
                }
            }
            SelectAll => {
                self.selected = [true; PATTERN_SIZE];
                self.select_mode = true;
            }
            DeselectAll => {
                self.selected = [false; PATTERN_SIZE];
                self.select_mode = false;
            }
            Reorder => {
                self.push_undo();
                self.pattern_mut().reorder();
            }
            Swap(a, b) => {
                self.push_undo();
                self.design.patterns.swap(a, b);
            }
            Duplicate(from, to) => {
                self.push_undo();
                self.design.patterns[to] = self.design.patterns[from];
            }
            ClearPattern(i) => {
                self.push_undo();
                self.design.patterns[i].clear();
            }
            ClearAll => {
                self.push_undo();
                self.design.patterns = [Pattern::default(); MAX_PATTERNS];
                self.design.manual.clear();
            }
            Undo => {
                let label = self.undo_once();
                self.after_history_move("Undo", label);
            }
            Redo => {
                let label = self.redo_once();
                self.after_history_move("Redo", label);
            }
            UndoSteps(n) => {
                let mut last = None;
                for _ in 0..n {
                    last = self.undo_once().or(last);
                }
                self.after_history_move("Went back to before", last);
            }
            RedoSteps(n) => {
                let mut last = None;
                for _ in 0..n {
                    last = self.redo_once().or(last);
                }
                self.after_history_move("Went forward to after", last);
            }
            ToggleHistory => self.show_history = !self.show_history,
            ToggleAbout => self.show_about = !self.show_about,
            PrintPdf => self.print_pdf(ctx),
            SetLength(inches) => {
                if self.design.target_length_inches != inches {
                    self.undo_label = "Length".into();
                    self.push_undo();
                    self.design.target_length_inches = inches;
                    self.dirty = true;
                    self.draw_design();
                }
            }
            Draw => self.draw_design(),
            SettingsChanged => {
                // The control already changed the design: record how it was at the start of the frame.
                if self.frame_start != self.design {
                    self.undo_label = describe_change(&self.frame_start, &self.design);
                    let before = self.frame_start.clone();
                    self.push_entry(before, true);
                }
                if self.settings.auto_refresh {
                    self.draw_design();
                }
            }
            OptionsChanged => {
                if self.frame_start != self.design {
                    self.undo_label = describe_change(&self.frame_start, &self.design);
                    let before = self.frame_start.clone();
                    self.push_entry(before, true);
                }
                self.save_settings();
                self.draw_design();
            }
            AutoPattern => self.auto_pattern(),
            SaveImage => self.save_image(ctx),
            Adjust(side, delta) => {
                match side {
                    Side::Left => self.design.left_adjust += delta,
                    Side::Right => self.design.right_adjust += delta,
                    Side::Bottom => self.design.bottom_adjust += delta,
                }
                self.draw_design();
            }
            EditPattern(i) => self.select_pattern(i),
            GoEditPattern(p) => self.select_pattern(p),
            BackToEditTab => {
                self.tab = Tab::EditPatterns;
                self.came_from_edit_tab = false;
            }
            UseManual => {
                self.design.mode = DesignMode::Manual;
                self.dirty = true;
                self.draw_design();
            }
            ClearManual => self.manual_edit(|m, _| {
                m.clear();
                Ok(())
            }),
            ManCopyMain => {
                let mut from = self.design.clone();
                from.mode = self.copy_from_mode;
                let strand = build_strand(&from, &self.catalog);
                if strand.is_empty() {
                    self.status = "The Main design is empty: add beads to Pattern 1 first.".into();
                } else {
                    self.manual_edit(|m, pats| {
                        *m = ManualDesign::from_strand(&strand, pats);
                        Ok(())
                    });
                }
            }
            ManAuto5 => {
                if self.catalog.is_empty() {
                    return;
                }
                self.push_undo();
                for p in 0..MAX_PATTERNS {
                    self.design.patterns[p] = self.random_pattern();
                }
                if self.design.manual.is_empty() {
                    for p in 0..MAX_PATTERNS {
                        self.design.manual.push(ManualItem::Pattern {
                            pattern: p,
                            mirrored: false,
                        });
                    }
                }
                self.design.mode = DesignMode::Manual;
                self.dirty = true;
                self.draw_design();
                self.status = "Made 5 new automatic patterns (Undo with Ctrl+Z).".into();
            }
            ManSplitAll => self.manual_edit(|m, pats| {
                m.split_all(pats);
                Ok(())
            }),
            ManAddPattern(p) => {
                let mirrored = self.edit_mirrored;
                self.manual_edit(|m, pats| {
                    if pats[p].is_empty() {
                        return Err(format!("Pattern {} is empty.", p + 1));
                    }
                    if m.push(ManualItem::Pattern { pattern: p, mirrored }) {
                        Ok(())
                    } else {
                        Err("The design is full (500 items).".into())
                    }
                });
            }
            ManRemovePattern(p) => self.manual_edit(|m, _| {
                if m.remove_last_pattern(p) {
                    Ok(())
                } else {
                    Err(format!("Pattern {} isn't used in the design.", p + 1))
                }
            }),
            ManAddBead => match self.list_selection {
                Some(b) => self.manual_edit(|m, _| {
                    if m.push(ManualItem::Bead(b)) {
                        Ok(())
                    } else {
                        Err("The design is full (500 items).".into())
                    }
                }),
                None => self.status = "Pick a bead from the list on the Main tab first.".into(),
            },
            ManTool(tool) => {
                self.edit_tool = if self.edit_tool == tool { EditTool::None } else { tool };
            }
            ManClickGap(at) => {
                let item = match self.edit_tool {
                    EditTool::InsertPattern(p) => Some(ManualItem::Pattern {
                        pattern: p,
                        mirrored: self.edit_mirrored,
                    }),
                    EditTool::InsertBead => self.list_selection.map(ManualItem::Bead),
                    _ => None,
                };
                if let Some(item) = item {
                    self.manual_edit(|m, _| {
                        if m.insert(at, item) {
                            Ok(())
                        } else {
                            Err("The design is full (500 items).".into())
                        }
                    });
                } else if self.edit_tool == EditTool::InsertBead {
                    self.status = "Pick a bead from the list on the Main tab first.".into();
                }
            }
            ManClickItem(i) => {
                let mirrored = self.edit_mirrored;
                let bead = self.list_selection;
                match self.edit_tool {
                    EditTool::ReplacePattern(p) => self.manual_edit(|m, _| {
                        m.replace(i, ManualItem::Pattern { pattern: p, mirrored });
                        Ok(())
                    }),
                    EditTool::ReplaceBead => match bead {
                        Some(b) => self.manual_edit(|m, _| {
                            m.replace(i, ManualItem::Bead(b));
                            Ok(())
                        }),
                        None => self.status = "Pick a bead from the list on the Main tab first.".into(),
                    },
                    EditTool::Delete => self.manual_edit(|m, _| {
                        m.remove(i);
                        Ok(())
                    }),
                    EditTool::Split => self.manual_edit(|m, pats| {
                        m.split(i, pats);
                        Ok(())
                    }),
                    _ => {}
                }
            }
            ManDelete(i) => self.manual_edit(|m, _| {
                m.remove(i);
                Ok(())
            }),
            ManSplit(i) => self.manual_edit(|m, pats| {
                m.split(i, pats);
                Ok(())
            }),
            ManFlip(i) => self.manual_edit(|m, _| {
                m.flip(i);
                Ok(())
            }),
            ManMove(i, left) => self.manual_edit(|m, _| {
                m.move_item(i, left);
                Ok(())
            }),
            New => self.ask_then(AfterConfirm::New, ctx),
            Open => {
                let dir = self.bookmarks_dir(false);
                let filter = Filter("Bead design", &[design_file::EXTENSION]);
                // On the web the file arrives later (see `take_picked` in update).
                if let Some(path) = platform::pick_file(ctx, Pick::Design, filter, dir.as_deref()) {
                    self.ask_then(AfterConfirm::Open(path), ctx);
                }
            }
            OpenFile(path) => self.ask_then(AfterConfirm::Open(path), ctx),
            Save => self.save_design(),
            SaveAs => self.save_as(),
            QuickBookmark => self.quick_bookmark(),
            PreviewBookmark(i) => self.preview_bookmark(i),
            RefreshBookmarks => self.refresh_bookmarks(),
            MakerSave => self.maker_save(),
            MakerLibrary => self.maker_library(),
            MakerChoosePhoto => self.maker_choose_photo(ctx),
            MakerUsePhoto(path) => self.maker_load_photo(path),
            Quit => self.ask_then(AfterConfirm::Quit, ctx),
        }
        if edited && self.settings.auto_refresh {
            self.draw_design();
        }
    }

    // ----- design files (bookmarks) -----------------------------------------------

    fn has_design(&self) -> bool {
        self.design.patterns.iter().any(|p| !p.is_empty())
    }

    /// Runs `next` now, or first asks to save unsaved changes.
    fn ask_then(&mut self, next: AfterConfirm, ctx: &egui::Context) {
        if self.dirty && self.has_design() {
            self.confirm = Some(next);
        } else {
            self.do_after(next, ctx);
        }
    }

    fn do_after(&mut self, next: AfterConfirm, ctx: &egui::Context) {
        match next {
            AfterConfirm::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            AfterConfirm::New => self.new_design(),
            AfterConfirm::Open(path) => self.open_design(&path),
        }
    }

    fn new_design(&mut self) {
        self.undo_label = "New design".into();
        self.push_undo(); // Undo brings the old design back
        let keep = self.design.clone();
        self.design = Design {
            mode: keep.mode,
            target_length_inches: keep.target_length_inches,
            clasp: keep.clasp,
            use_clasp: keep.use_clasp,
            crimp: keep.crimp,
            crimps_per_end: keep.crimps_per_end,
            ..Design::default()
        };
        self.current_file = None;
        self.imported_name = None;
        self.dirty = false;
        self.current_pat = 0;
        self.selected = [false; PATTERN_SIZE];
        self.draw_design();
        self.status = "New design.".into();
    }

    /// The bookmarks folder; with `create`, makes one (next to the Beads folder) if none is set.
    fn bookmarks_dir(&mut self, create: bool) -> Option<PathBuf> {
        let set = PathBuf::from(&self.settings.bookmarks_folder);
        if !self.settings.bookmarks_folder.is_empty() && platform::is_dir(&set) {
            return Some(set);
        }
        if !create {
            return None;
        }
        let base = if !self.settings.main_folder.is_empty() {
            PathBuf::from(&self.settings.main_folder)
        } else {
            Path::new(&self.settings.bead_folder).parent()?.to_path_buf()
        };
        let dir = base.join("Bookmarks");
        platform::create_dir_all(&dir).ok()?;
        self.settings.bookmarks_folder = dir.to_string_lossy().into_owned();
        self.save_settings();
        Some(dir)
    }

    fn write_design(&mut self, path: &Path) {
        let saved = SavedDesign::from_design(&self.design, &parts!(self), self.settings.background);
        match platform::write(path, saved.to_text()) {
            Ok(()) => {
                self.current_file = Some(path.to_path_buf());
                self.imported_name = None;
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
                self.bookmarks_loaded_from.clear(); // refresh the Bookmarks tab list
            }
            Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
        }
    }

    fn save_design(&mut self) {
        match self.current_file.clone() {
            Some(path) => self.write_design(&path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        if !self.has_design() {
            self.status = "Nothing to save yet: add beads to Pattern 1.".into();
            return;
        }
        let saved = SavedDesign::from_design(&self.design, &parts!(self), self.settings.background);
        let name = self
            .current_file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| saved.auto_file_name(&timestamp()));
        let dir = self.bookmarks_dir(true);
        let filter = Filter("Bead design", &[design_file::EXTENSION]);
        if let Some(path) = platform::save_file(&name, filter, dir.as_deref()) {
            self.write_design(&path);
        }
    }

    /// The 2013 "BookMark" button: save straight into the Bookmarks folder with an automatic name.
    fn quick_bookmark(&mut self) {
        if !self.has_design() {
            self.status = "Nothing to bookmark yet: add beads to Pattern 1.".into();
            return;
        }
        let Some(dir) = self.bookmarks_dir(true) else {
            self.status = "Choose a Bookmarks folder on the \"Bead & Clasp Folders\" tab first.".into();
            return;
        };
        let saved = SavedDesign::from_design(&self.design, &parts!(self), self.settings.background);
        let name = saved.auto_file_name(&timestamp());
        let mut path = dir.join(&name);
        let mut n = 2;
        while platform::exists(&path) {
            let stem = name.trim_end_matches(&format!(".{}", design_file::EXTENSION));
            path = dir.join(format!("{stem}_{n}.{}", design_file::EXTENSION));
            n += 1;
        }
        self.write_design(&path);
    }

    fn open_design(&mut self, path: &Path) {
        let text = match platform::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Could not open {}: {e}", path.display());
                return;
            }
        };
        let saved = match SavedDesign::parse(&text) {
            Ok(s) => s,
            Err(msg) => {
                self.status = format!("{}: {msg}", path.display());
                return;
            }
        };
        let (design, missing) = saved.to_design(&parts!(self), &self.design);
        self.undo_label = format!(
            "Open {}",
            path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
        );
        self.push_undo();
        self.design = design;
        if let Some(bg) = saved.background {
            self.settings.background = bg;
        }
        let is_2013 = text.starts_with("Bead Design File");
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_2013 {
            // Keep the original 2013 file untouched: Save will ask for a new name.
            self.current_file = None;
            self.imported_name = Some(file_name.clone());
        } else {
            self.current_file = Some(path.to_path_buf());
            self.imported_name = None;
        }
        self.dirty = false;
        self.current_pat = self.design.active_pattern; // the pattern that was selected when saved
        self.selected = [false; PATTERN_SIZE];
        self.draw_design();

        let mut msg = format!("Opened {file_name}");
        if is_2013 {
            msg += " (2013 bookmark; Save will make a new file)";
        }
        if !missing.is_empty() {
            msg += &format!(". Beads not found: {}", missing.join(", "));
        }
        if saved.unreadable_manual > 0 {
            msg += &format!(
                ". {} single bead(s) of the 2013 manual design were saved by list position and couldn't be read",
                saved.unreadable_manual
            );
        }
        self.status = msg;
    }

    // ----- Bookmarks tab ------------------------------------------------------------

    fn refresh_bookmarks(&mut self) {
        let dir = self.settings.bookmarks_folder.clone();
        self.bookmarks_loaded_from = dir.clone();
        self.bookmarks.clear();
        self.bookmark_selected = None;
        self.bookmark_preview.clear();
        self.bookmark_info.clear();
        if dir.is_empty() {
            return;
        }
        let mut files: Vec<(u64, PathBuf)> = platform::list(Path::new(&dir))
            .into_iter()
            .filter(|(p, is_dir)| {
                !is_dir
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case(design_file::EXTENSION))
            })
            .map(|(p, _)| (platform::modified_millis(&p), p))
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
        self.bookmarks = files.into_iter().map(|(_, p)| p).collect();
    }

    fn preview_bookmark(&mut self, i: usize) {
        self.bookmark_selected = Some(i);
        self.bookmark_preview.clear();
        let Some(path) = self.bookmarks.get(i).cloned() else {
            return;
        };
        let parsed = platform::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| SavedDesign::parse(&t));
        let saved = match parsed {
            Ok(s) => s,
            Err(msg) => {
                self.bookmark_info = msg;
                return;
            }
        };
        let parts = parts!(self);
        let (design, missing) = saved.to_design(&parts, &self.design);
        let strand = build_strand(&design, &self.catalog);
        self.bookmark_preview = layout_circular(&strand, &design, &parts);
        let mode = match design.mode {
            DesignMode::OnePatternLinear => "1 pattern, linear",
            DesignMode::OnePatternMirrored => "1 pattern, mirrored",
            DesignMode::TwoPattern => "2 patterns",
            DesignMode::Manual => "manual edit design",
        };
        let mut info = format!(
            "{mode}, {} beads, {:.2} in",
            strand.len(),
            length_inches(&strand, &design, &parts)
        );
        if !missing.is_empty() {
            info += &format!("\nBeads not found: {}", missing.join(", "));
        }
        if saved.unreadable_manual > 0 {
            info += &format!(
                "\n{} single bead(s) of the 2013 manual design couldn't be read",
                saved.unreadable_manual
            );
        }
        self.bookmark_info = info;
    }

    /// Runs one change to the manual design with undo, switching the Main tab to
    /// the Manual Edit design and redrawing. An `Err` message means nothing changed.
    fn manual_edit(&mut self, change: impl FnOnce(&mut ManualDesign, &[Pattern; MAX_PATTERNS]) -> Result<(), String>) {
        let before = self.design.clone();
        let patterns = self.design.patterns;
        match change(&mut self.design.manual, &patterns) {
            Ok(()) => {
                self.push_entry(before, false);
                if self.design.mode != DesignMode::Manual {
                    self.design.mode = DesignMode::Manual;
                    self.status = "The Main tab now shows the Manual Edit design.".into();
                }
                self.dirty = true;
                self.draw_design();
            }
            Err(msg) => self.status = msg,
        }
    }

    /// One automatic pattern (random or symmetrical, as set on the Main tab).
    fn random_pattern(&mut self) -> Pattern {
        // Like 2013: use the beads in the chosen sub-folder if one is selected.
        let pool: Vec<usize> = (0..self.catalog.len()).filter(|&i| self.in_current_folder(i)).collect();
        let pool = if pool.is_empty() {
            (0..self.catalog.len()).collect()
        } else {
            pool
        };
        let (min, max) = (self.settings.auto_min_beads, self.settings.auto_max_beads);
        // A colour scheme (once the colours are known).
        let scheme = ColourScheme::from_key(&self.settings.auto_colours).unwrap_or_default();
        let coloured: Vec<(usize, BeadColour)> = pool
            .iter()
            .filter_map(|&b| self.colours.get(b).copied().flatten().map(|c| (b, c)))
            .collect();
        if scheme != ColourScheme::Any && coloured.len() >= 2 {
            let cap = if self.settings.auto_symmetrical {
                (PATTERN_SIZE + 2) / 2
            } else {
                PATTERN_SIZE
            };
            let n = self.rng.between(min.clamp(1, cap), max.clamp(1, cap));
            let chosen = colours::choose_beads(&coloured, n, scheme, &mut self.rng);
            return if self.settings.auto_symmetrical {
                arrange_symmetric(&chosen)
            } else {
                arrange_random(&chosen, scheme.keeps_order(), &mut self.rng)
            };
        }
        let picked = if self.settings.auto_symmetrical {
            auto_symmetric(pool.len(), min, max, &mut self.rng)
        } else {
            auto_random(pool.len(), min, max, &mut self.rng)
        };
        let mut p = Pattern::default();
        for (slot, bead) in picked.slots.iter().enumerate() {
            if let Some(b) = bead {
                p.set(slot, pool[*b]);
            }
        }
        p
    }

    fn auto_pattern(&mut self) {
        if self.catalog.is_empty() {
            return;
        }
        self.undo_label = format!("Automatic pattern {}", self.current_pat + 1);
        self.push_undo();
        // Fill the pattern being edited (Pattern 1, 2, ... as chosen under "Editing").
        let target = self.current_pat;
        self.design.patterns[target] = self.random_pattern();
        // A 2-pattern design can't be drawn without Pattern 2: fall back to one pattern.
        if self.design.mode == DesignMode::TwoPattern && self.design.patterns[1].is_empty() {
            self.design.mode = DesignMode::OnePatternLinear;
        }
        self.selected = [false; PATTERN_SIZE];
        self.dirty = true;
        self.draw_design();
        self.status = format!("New automatic Pattern {}.", target + 1);
    }

    fn in_current_folder(&self, bead: usize) -> bool {
        let want = &self.settings.current_bead_folder;
        want.is_empty() || self.catalog.get(bead).is_some_and(|b| &b.folder == want)
    }

    // ----- drawing -----------------------------------------------------------------

    fn draw_design(&mut self) {
        if self.design.mode == DesignMode::TwoPattern && self.design.patterns[1].is_empty() {
            self.status = "2 Patterns needs at least one bead in Pattern 2.".into();
            return;
        }
        if self.design.mode == DesignMode::Manual && self.design.manual.is_empty() {
            self.status = "The Manual Edit design is empty: add patterns on the Edit Patterns tab.".into();
        }
        let parts = parts!(self);
        let strand = build_strand(&self.design, &self.catalog);
        self.circular = layout_circular(&strand, &self.design, &parts);
        self.linear = layout_linear(&strand, &self.design, &parts);

        let items = material_list(&strand, &self.design, &parts);
        let mut text = String::new();
        for (count, name) in &items {
            text += &format!("{count:>3}  {name}\n");
        }
        self.material_lines.clear();
        if !strand.is_empty() {
            let length = length_inches(&strand, &self.design, &parts);
            text += &format!(
                "\nTotal beads: {}\nCalculated length: {:.2} in\nBeading wire to cut: {:.1} in\n  ({} in extra for crimping)\n",
                strand.len(),
                length,
                wire_length_inches(length),
                WIRE_EXTRA_INCHES
            );
            self.material_lines.push(format!("{length:.2} in necklace length"));
            self.material_lines
                .push(format!("{:.1} in beading wire to cut", wire_length_inches(length)));
            self.material_lines.push(String::new());
            self.material_lines
                .extend(items.iter().map(|(c, n)| format!("{c:>3}  {n}")));
        }
        // Stock and prices (only shown once something is entered on the Inventory tab).
        self.cost_lines = self.inventory.cost(&items);
        if !strand.is_empty() && !self.inventory.is_empty() {
            let cur = &self.settings.currency;
            let total: f64 = self.cost_lines.iter().map(|l| l.cost()).sum();
            let to_buy: Vec<&CostLine> = self.cost_lines.iter().filter(|l| l.to_buy > 0).collect();
            let buy_cost: f64 = to_buy.iter().map(|l| l.buy_cost()).sum();
            text += &format!("\nCost of this design: {cur}{total:.2}\n");
            if to_buy.is_empty() {
                text += "You have everything you need.\n";
            } else {
                text += &format!("To buy ({cur}{buy_cost:.2}):\n");
                for l in &to_buy {
                    text += &format!("{:>3}  {}\n", l.to_buy, inventory::base_name(&l.name));
                }
            }
        }
        self.materials = text;
    }

    /// Lines for the PDF design sheet: the material list, then cost and shopping list.
    fn pdf_lines(&self) -> Vec<String> {
        let mut lines = self.material_lines.clone();
        if !self.inventory.is_empty() && !self.cost_lines.is_empty() {
            let cur = &self.settings.currency;
            let total: f64 = self.cost_lines.iter().map(|l| l.cost()).sum();
            lines.push(String::new());
            lines.push(format!("Cost of this design: {cur}{total:.2}"));
            for l in self.cost_lines.iter().filter(|l| l.to_buy > 0) {
                lines.push(format!(
                    "Buy {} x {} (have {})",
                    l.to_buy,
                    inventory::base_name(&l.name),
                    l.on_hand
                ));
            }
        }
        lines
    }

    /// The pieces shown on the current tab (the circle for tabs without their own view).
    fn placed_for_tab(&self) -> Vec<Placed> {
        match self.tab {
            Tab::Linear => self.linear.clone(),
            Tab::Arc => self.arc_layout(),
            Tab::Bookmarks if !self.bookmark_preview.is_empty() => self.bookmark_preview.clone(),
            _ => self.circular.clone(),
        }
    }

    /// Renders `placed` into a picture `width` pixels wide on the chosen
    /// background. With `with_list`, the material list is written inside the circle.
    fn render_picture(&mut self, ctx: &egui::Context, placed: &[Placed], width: u32, with_list: bool) -> Rgba {
        let parts = parts!(self);
        for p in placed {
            if let Some(file) = parts.info(p.piece).and_then(|i| i.path) {
                self.images.get(ctx, file); // make sure pixels are loaded
            }
        }
        let margin = width as f64 / 120.0;
        let height = ((width as f64) * Fit::aspect(placed)).ceil().max(1.0) as u32 + 2 * margin as u32;
        let [r, g, b] = self.settings.background.rgb();
        let mut canvas = Rgba::filled(width, height, [r, g, b, 255]);
        let images = &self.images;
        render::compose(&mut canvas, placed, margin, |piece| {
            parts
                .info(piece)
                .and_then(|i| i.path)
                .and_then(|file| images.pixels(file))
        });
        if with_list && !self.material_lines.is_empty() {
            if self.text_painter.is_none() {
                self.text_painter = TextPainter::new();
            }
            if let (Some(tp), Some(fit), Some(radius)) = (
                self.text_painter.as_ref(),
                Fit::compute(placed, width as f64, height as f64, margin),
                ring_radius(placed),
            ) {
                let half = radius * 0.62;
                let (x0, y0) = fit.apply(-half, -half);
                let (x1, y1) = fit.apply(half, half);
                let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
                let lines = &self.material_lines;
                let widest = lines.iter().map(|l| tp.width(l, 100.0)).fold(1.0, f32::max) / 100.0;
                let size = (bh / (lines.len() as f32 * 1.3))
                    .min(bw / widest)
                    .clamp(8.0, width as f32 / 40.0);
                let top = y0 as f32 + (bh - lines.len() as f32 * size * 1.3).max(0.0) / 2.0;
                let colour = text_colour(self.settings.background);
                for (i, line) in lines.iter().enumerate() {
                    tp.draw(&mut canvas, x0 as f32, top + i as f32 * size * 1.3, line, size, colour);
                }
            }
        }
        canvas
    }

    fn save_image(&mut self, ctx: &egui::Context) {
        let placed = self.placed_for_tab();
        if placed.is_empty() {
            self.status = "Draw a design first.".into();
            return;
        }
        let start = (!self.settings.images_folder.is_empty()).then(|| PathBuf::from(&self.settings.images_folder));
        let Some(path) = platform::save_file("BeadDesign.png", Filter("PNG image", &["png"]), start.as_deref()) else {
            return;
        };
        // The material list goes inside the circle (like 2013), not on the other views.
        let circle = !matches!(self.tab, Tab::Linear | Tab::Arc | Tab::Bookmarks);
        let canvas = self.render_picture(ctx, &placed, 2400, circle && self.settings.materials_on_picture);
        self.status = match encode_png(&canvas).and_then(|png| platform::write(&path, png)) {
            Ok(()) => format!("Saved {}", path.display()),
            Err(e) => format!("Could not save image: {e}"),
        };
    }

    /// File > Print: a one-page PDF design sheet (picture + material list), opened
    /// in the computer's PDF viewer to print.
    fn print_pdf(&mut self, ctx: &egui::Context) {
        let placed = if matches!(self.tab, Tab::Linear | Tab::Arc) {
            self.placed_for_tab()
        } else {
            self.circular.clone()
        };
        if placed.is_empty() {
            self.status = "Draw a design first.".into();
            return;
        }
        let name = match (&self.current_file, &self.imported_name) {
            (Some(p), _) => p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            (None, Some(n)) => n.trim_end_matches(".txt").to_string(),
            (None, None) => "Bead design".to_string(),
        };
        let start = (!self.settings.images_folder.is_empty()).then(|| PathBuf::from(&self.settings.images_folder));
        let Some(path) = platform::save_file(
            &format!("{name}.pdf"),
            Filter("PDF document", &["pdf"]),
            start.as_deref(),
        ) else {
            return;
        };

        let canvas = self.render_picture(ctx, &placed, 1800, false);
        // The PDF holds the picture as a JPEG on the background colour (no transparency needed).
        let rgb: Vec<u8> = canvas.pixels.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut jpeg = Vec::new();
        let encoded = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode(
            &rgb,
            canvas.width,
            canvas.height,
            image::ExtendedColorType::Rgb8,
        );
        if let Err(e) = encoded {
            self.status = format!("Could not make the picture: {e}");
            return;
        }
        let mode = match self.design.mode {
            DesignMode::OnePatternLinear => "1 pattern, linear",
            DesignMode::OnePatternMirrored => "1 pattern, mirrored",
            DesignMode::TwoPattern => "2 patterns",
            DesignMode::Manual => "manual edit design",
        };
        let date = chrono::Local::now().format("%d %b %Y").to_string();
        let subtitle = format!("Bead design  -  {mode}  -  {date}");
        let pdf =
            beads_core::pdf::design_sheet(&name, &subtitle, &jpeg, canvas.width, canvas.height, &self.pdf_lines());
        match platform::write(&path, pdf) {
            Ok(()) if platform::IS_WEB => {
                self.status = format!("Downloaded {name}.pdf - open it to print.");
            }
            Ok(()) => {
                self.status = format!("Saved {} - opening it to print", path.display());
                if let Err(e) = platform::open_with_system(&path) {
                    self.status = format!("Saved {} (couldn't open it: {e})", path.display());
                }
            }
            Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
        }
    }

    // ----- UI pieces ---------------------------------------------------------------

    fn menu_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Choose Bead Folder…").clicked() {
                    actions.push(Action::ChooseFolder(FolderKind::Beads));
                    ui.close_menu();
                }
                let file_items: [(&str, fn() -> Action); 5] = [
                    ("New Design            Ctrl+N", || Action::New),
                    ("Open Necklace File…   Ctrl+O", || Action::Open),
                    ("Save Necklace File    Ctrl+S", || Action::Save),
                    ("Save As…", || Action::SaveAs),
                    ("Bookmark (quick save) Ctrl+B", || Action::QuickBookmark),
                ];
                for (label, make) in file_items {
                    if ui.button(label).clicked() {
                        actions.push(make());
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Save Design Image…").clicked() {
                    actions.push(Action::SaveImage);
                    ui.close_menu();
                }
                if ui.button("Print / PDF Design Sheet…  Ctrl+P").clicked() {
                    actions.push(Action::PrintPdf);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    actions.push(Action::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                let items: [(&str, fn() -> Action); 7] = [
                    ("Undo            Ctrl+Z", || Action::Undo),
                    ("Redo            Ctrl+Y", || Action::Redo),
                    ("History…        Ctrl+H", || Action::ToggleHistory),
                    ("Select All      Ctrl+A", || Action::SelectAll),
                    ("Deselect All    Ctrl+D", || Action::DeselectAll),
                    ("Shift Left      Ctrl+←", || Action::ShiftLeft),
                    ("Shift Right     Ctrl+→", || Action::ShiftRight),
                ];
                for (label, make) in items {
                    if ui.button(label).clicked() {
                        actions.push(make());
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.menu_button("Duplicate Pattern", |ui| {
                    for from in 0..MAX_PATTERNS {
                        ui.menu_button(format!("Pattern {} to", from + 1), |ui| {
                            for to in (0..MAX_PATTERNS).filter(|&t| t != from) {
                                if ui.button(format!("Pattern {}", to + 1)).clicked() {
                                    actions.push(Action::Duplicate(from, to));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
                ui.menu_button("Swap Patterns", |ui| {
                    for a in 0..MAX_PATTERNS {
                        for b in a + 1..MAX_PATTERNS {
                            if ui.button(format!("Pattern {} and {}", a + 1, b + 1)).clicked() {
                                actions.push(Action::Swap(a, b));
                                ui.close_menu();
                            }
                        }
                    }
                });
                ui.menu_button("Clear", |ui| {
                    for p in 0..MAX_PATTERNS {
                        if ui.button(format!("Pattern {}", p + 1)).clicked() {
                            actions.push(Action::ClearPattern(p));
                            ui.close_menu();
                        }
                    }
                    if ui.button("Manual Edit Design").clicked() {
                        actions.push(Action::ClearManual);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Everything").clicked() {
                        actions.push(Action::ClearAll);
                        ui.close_menu();
                    }
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About Bunch-a-Beads…").clicked() {
                    actions.push(Action::ToggleAbout);
                    ui.close_menu();
                }
            });
        });
    }

    fn bead_list_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if ui
            .add(colour_button("Bead Folder…", FILE_TAN))
            .on_hover_text("Choose the folder that holds your bead pictures")
            .clicked()
        {
            actions.push(Action::ChooseFolder(FolderKind::Beads));
        }
        let before = self.settings.current_bead_folder.clone();
        let shown_name = if before.is_empty() {
            "All folders".to_string()
        } else {
            before.clone()
        };
        egui::ComboBox::from_label("Folder")
            .selected_text(shown_name)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.settings.current_bead_folder, String::new(), "All folders");
                for f in &self.bead_subfolders {
                    ui.selectable_value(&mut self.settings.current_bead_folder, f.clone(), f.as_str());
                }
            });
        if self.settings.current_bead_folder != before {
            self.save_settings();
        }
        ui.horizontal(|ui| {
            ui.label("Find:");
            ui.text_edit_singleline(&mut self.filter);
        });
        ui.horizontal(|ui| {
            let label = self.colour_filter.map_or("All colours", |f| f.label());
            egui::ComboBox::from_id_salt("colour_filter")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.colour_filter, None, "All colours");
                    for fam in ColourFamily::ALL {
                        ui.selectable_value(&mut self.colour_filter, Some(fam), fam.label());
                    }
                });
            ui.label("Colour");
            if self.colour_rx.is_some() {
                ui.spinner().on_hover_text("Working out bead colours from the photos…");
            }
        });
        let filter = self.filter.to_lowercase();
        let colour_filter = self.colour_filter;
        let shown: Vec<usize> = (0..self.catalog.len())
            .filter(|&i| self.in_current_folder(i))
            .filter(|&i| match colour_filter {
                None => true,
                Some(f) => self.colours.get(i).copied().flatten().is_some_and(|c| c.family == f),
            })
            .filter(|&i| {
                filter.is_empty()
                    || self
                        .catalog
                        .get(i)
                        .is_some_and(|b| b.name.to_lowercase().contains(&filter))
            })
            .collect();

        let preview_h = 170.0;
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 2.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((ui.available_height() - preview_h).max(100.0))
            .show_rows(ui, row_h, shown.len(), |ui, rows| {
                for &i in &shown[rows] {
                    let name = self.catalog.get(i).map(|b| b.name.as_str()).unwrap_or("?");
                    ui.horizontal(|ui| {
                        // A small swatch of the bead's colour.
                        let (sw, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        if let Some(c) = self.colours.get(i).copied().flatten() {
                            ui.painter()
                                .rect_filled(sw, 2.0, Color32::from_rgb(c.rgb[0], c.rgb[1], c.rgb[2]));
                        }
                        if ui.selectable_label(self.list_selection == Some(i), name).clicked() {
                            self.list_selection = Some(i);
                        }
                    });
                }
            });

        ui.separator();
        if let Some(info) = self.list_selection.and_then(|i| self.catalog.get(i)) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(120.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::WHITE);
            if let Some(img) = info.image_path.as_deref().and_then(|p| self.images.get(ui.ctx(), p)) {
                paint_thumb(
                    ui.painter(),
                    img.texture.id(),
                    info.width_px,
                    info.height_px,
                    rect,
                    true,
                );
            }
            ui.label(format!(
                "{}\n{} × {} px  ({:.1} × {:.1} mm)",
                match self.list_selection.and_then(|i| self.colours.get(i).copied().flatten()) {
                    Some(c) => format!("{}  ({})", info.name, c.family.label()),
                    None => info.name.clone(),
                },
                info.width_px,
                info.height_px,
                catalog::px_to_mm(info.width_px as f64),
                catalog::px_to_mm(info.height_px as f64)
            ));
        }
    }

    fn main_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        // All five patterns, as in 2013. The selected one is edited below and is
        // the one drawn by the 1-pattern modes.
        let shown = MAX_PATTERNS;

        // Which pattern is being edited: one row per pattern, radio button beside its preview.
        if self.came_from_edit_tab
            && ui
                .add(colour_button("◀ Back to Edit Patterns", PATTERN_BLUE))
                .on_hover_text("Return to the Edit Patterns tab")
                .clicked()
        {
            actions.push(Action::BackToEditTab);
        }
        ui.horizontal(|ui| {
            egui::Grid::new("pattern_rows").spacing([8.0, 3.0]).show(ui, |ui| {
                ui.label(RichText::new("Editing").strong());
                ui.label(
                    RichText::new("(also the pattern drawn by the 1-pattern designs)")
                        .small()
                        .color(Color32::GRAY),
                );
                ui.end_row();
                for i in 0..shown {
                    if ui.radio(self.current_pat == i, format!("Pattern {}", i + 1)).clicked() {
                        actions.push(Action::EditPattern(i));
                    }
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(420.0, 22.0), Sense::click());
                    let bg = if i == self.current_pat {
                        Color32::from_rgb(225, 240, 255)
                    } else {
                        Color32::WHITE
                    };
                    ui.painter().rect_filled(rect, 2.0, bg);
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0_f32, Color32::LIGHT_GRAY),
                        egui::StrokeKind::Inside,
                    );
                    let mut x = rect.left() + 2.0;
                    for bead in self.design.patterns[i].beads() {
                        let Some(info) = self.catalog.get(bead) else { continue };
                        let Some(img) = info.image_path.as_deref().and_then(|p| self.images.get(&ctx, p)) else {
                            continue;
                        };
                        let h = rect.height() - 4.0;
                        let w = h * info.width_px as f32 / info.height_px.max(1) as f32;
                        let r = Rect::from_min_size(Pos2::new(x, rect.top() + 2.0), Vec2::new(w, h));
                        ui.painter().image(img.texture.id(), r, full_uv(), Color32::WHITE);
                        x += w + 1.0;
                    }
                    if resp.on_hover_text(format!("Click to edit Pattern {}", i + 1)).clicked() {
                        actions.push(Action::EditPattern(i));
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(4.0);

        // The 12 slots: picture / Select n / Remove.
        egui::Grid::new("slots").spacing([4.0, 2.0]).show(ui, |ui| {
            let pattern = self.design.patterns[self.current_pat];
            // Dragging a bead to another place: (from slot, its picture) while dragging.
            let mut slot_rects = [Rect::NOTHING; PATTERN_SIZE];
            let mut dragging: Option<(usize, Option<(egui::TextureId, u32, u32)>)> = None;
            let mut dropped: Option<usize> = None;
            for s in 0..PATTERN_SIZE {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT_PX), Sense::click_and_drag());
                slot_rects[s] = rect;
                let painter = ui.painter();
                painter.rect_filled(
                    rect,
                    3.0,
                    if self.selected[s] {
                        SELECTED_GOLD
                    } else {
                        Color32::from_gray(250)
                    },
                );
                let stroke = if self.selected[s] {
                    Stroke::new(3.0_f32, Color32::from_rgb(200, 120, 0))
                } else {
                    Stroke::new(1.0_f32, Color32::from_gray(200))
                };
                painter.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                let mut tip = "(empty)".to_string();
                let mut picture = None;
                if let Some(info) = pattern.get(s).and_then(|b| self.catalog.get(b)) {
                    if let Some(img) = info.image_path.as_deref().and_then(|p| self.images.get(&ctx, p)) {
                        picture = Some((img.texture.id(), info.width_px, info.height_px));
                        if !resp.dragged() {
                            paint_thumb(
                                ui.painter(),
                                img.texture.id(),
                                info.width_px,
                                info.height_px,
                                rect.shrink(3.0),
                                true,
                            );
                        }
                    }
                    tip = format!("{}\nDrag to move it; Ctrl+click to select it for shifting", info.name);
                }
                if pattern.get(s).is_some() && resp.dragged() {
                    dragging = Some((s, picture));
                }
                if pattern.get(s).is_some() && resp.drag_stopped() {
                    dropped = Some(s);
                }
                let resp = resp.on_hover_text(tip);
                if resp.clicked() {
                    let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                    actions.push(if ctrl || shift {
                        Action::SlotToggleSelect(s)
                    } else {
                        Action::SlotClicked(s)
                    });
                }
            }
            // While dragging: mark where it will go and carry the picture under the pointer.
            let pointer = ui.ctx().pointer_latest_pos();
            let target = |p: Option<Pos2>| p.and_then(|p| slot_rects.iter().position(|r| r.expand(2.0).contains(p)));
            if let Some((from, picture)) = dragging {
                if let Some(to) = target(pointer).filter(|&to| to != from) {
                    let r = slot_rects[to];
                    let x = if to > from { r.right() + 2.0 } else { r.left() - 2.0 };
                    ui.painter().line_segment(
                        [Pos2::new(x, r.top()), Pos2::new(x, r.bottom())],
                        Stroke::new(3.0_f32, Color32::from_rgb(200, 120, 0)),
                    );
                }
                if let (Some(p), Some((tex, w, h))) = (pointer, picture) {
                    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("slot_drag"));
                    let r = Rect::from_center_size(p, Vec2::splat(SLOT_PX));
                    paint_thumb(&ui.ctx().layer_painter(layer), tex, w, h, r.shrink(3.0), true);
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if let Some(from) = dropped {
                if let Some(to) = target(pointer).filter(|&to| to != from) {
                    actions.push(Action::MoveSlot(from, to));
                }
            }
            ui.end_row();
            for s in 0..PATTERN_SIZE {
                if ui
                    .add(colour_button(&format!("Select {}", s + 1), PATTERN_BLUE))
                    .on_hover_text("Put the bead chosen in the list into this position")
                    .clicked()
                {
                    actions.push(Action::SelectIntoSlot(s));
                }
            }
            ui.end_row();
            for s in 0..PATTERN_SIZE {
                if ui.add(colour_button("Remove", PATTERN_BLUE)).clicked() {
                    actions.push(Action::RemoveSlot(s));
                }
            }
            ui.end_row();
        });

        // Pattern edit buttons.
        ui.horizontal(|ui| {
            let was = self.select_mode;
            ui.toggle_value(&mut self.select_mode, "Select/Deselect Beads")
                .on_hover_text(
                    "Turn on, then click beads in the pattern to select them for shifting \
                 (or Ctrl+click them at any time)",
                );
            if was && !self.select_mode {
                self.selected = [false; PATTERN_SIZE];
            }
            let buttons: [(&str, &str, Action); 3] = [
                (
                    "< Left",
                    "Shift the selected beads left; with none selected, the whole pattern (Ctrl+Left)",
                    Action::ShiftLeft,
                ),
                (
                    "Right >",
                    "Shift the selected beads right; with none selected, the whole pattern (Ctrl+Right)",
                    Action::ShiftRight,
                ),
                (
                    "<<Reorder>>",
                    "Remove blanks and rotate the pattern one bead to the right",
                    Action::Reorder,
                ),
            ];
            for (label, tip, action) in buttons {
                if ui.add(colour_button(label, PATTERN_BLUE)).on_hover_text(tip).clicked() {
                    actions.push(action);
                }
            }
            // Swap or copy the selected pattern with any other one.
            let cur = self.current_pat;
            ui.menu_button(
                RichText::new(format!("Swap {} ↔ ▾", cur + 1)).color(Color32::BLACK),
                |ui| {
                    for other in (0..MAX_PATTERNS).filter(|&o| o != cur) {
                        if ui.button(format!("with Pattern {}", other + 1)).clicked() {
                            actions.push(Action::Swap(cur, other));
                            ui.close_menu();
                        }
                    }
                },
            )
            .response
            .on_hover_text("Swap this pattern with another one");
            ui.menu_button(
                RichText::new(format!("Copy {} to ▾", cur + 1)).color(Color32::BLACK),
                |ui| {
                    for other in (0..MAX_PATTERNS).filter(|&o| o != cur) {
                        if ui.button(format!("Pattern {}", other + 1)).clicked() {
                            actions.push(Action::Duplicate(cur, other));
                            ui.close_menu();
                        }
                    }
                },
            )
            .response
            .on_hover_text("Copy this pattern over another one");
        });
        ui.separator();

        // Controls on the left, the design on the right.
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(210.0);
                self.design_controls(ui, actions);
            });
            let (grid, bg) = (self.settings.show_grid, self.settings.background);
            let list = self
                .settings
                .materials_on_picture
                .then_some(self.material_lines.as_slice());
            design_canvas_listed(ui, &mut self.images, &parts!(self), &self.circular, grid, bg, list);
        });
    }

    fn design_controls(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.label(RichText::new("# of Patterns").strong());
        for (mode, label) in [
            (DesignMode::OnePatternLinear, "1 Pattern - Linear"),
            (DesignMode::OnePatternMirrored, "1 Pattern - Mirrored"),
            (DesignMode::TwoPattern, "2 Patterns"),
            (DesignMode::Manual, "Manual Edit Design"),
        ] {
            if ui.radio_value(&mut self.design.mode, mode, label).changed() {
                actions.push(Action::SettingsChanged);
            }
        }
        let manual = self.design.mode == DesignMode::Manual;
        ui.add_enabled_ui(!manual, |ui| {
            ui.horizontal(|ui| {
                ui.label("Length:");
                let r = ui.add(
                    egui::DragValue::new(&mut self.design.target_length_inches)
                        .range(1.0..=36.0)
                        .speed(0.25)
                        .suffix(" in"),
                );
                if r.changed() {
                    actions.push(Action::SettingsChanged);
                }
                ui.menu_button("▾", |ui| {
                    for (name, inches) in LENGTH_PRESETS {
                        if ui.button(format!("{name}  ({inches} in)")).clicked() {
                            actions.push(Action::SetLength(inches));
                            ui.close_menu();
                        }
                    }
                })
                .response
                .on_hover_text("Common necklace and bracelet lengths");
            })
        });
        egui::Grid::new("adjust").show(ui, |ui| {
            let mode = self.design.mode;
            for (side, label, value, enabled) in [
                (Side::Left, "Left", self.design.left_adjust, true),
                (
                    Side::Right,
                    "Right",
                    self.design.right_adjust,
                    mode != DesignMode::OnePatternLinear,
                ),
                (
                    Side::Bottom,
                    "Bottom",
                    self.design.bottom_adjust,
                    mode == DesignMode::TwoPattern,
                ),
            ] {
                let enabled = enabled && !manual;
                ui.add_enabled(enabled, egui::Label::new(label));
                if ui.add_enabled(enabled, colour_button("- Sub", DISPLAY_GREEN)).clicked() {
                    actions.push(Action::Adjust(side, -1));
                }
                if ui.add_enabled(enabled, colour_button("+ Add", DISPLAY_GREEN)).clicked() {
                    actions.push(Action::Adjust(side, 1));
                }
                ui.add_enabled(enabled, egui::Label::new(format!("{value:+}")));
                ui.end_row();
            }
        });
        ui.checkbox(&mut self.settings.auto_refresh, "Auto Refresh");
        ui.checkbox(&mut self.settings.show_grid, "Show Grid");
        ui.checkbox(&mut self.settings.materials_on_picture, "Material List on Picture");
        if ui.checkbox(&mut self.design.use_clasp, "Add Clasp").changed() {
            actions.push(Action::OptionsChanged);
        }
        let has_pendant = self.design.pendant.is_some();
        if ui
            .add_enabled(
                has_pendant,
                egui::Checkbox::new(&mut self.design.use_pendant, "Show Pendant"),
            )
            .on_disabled_hover_text("Choose a pendant on the Pendants tab first")
            .changed()
        {
            actions.push(Action::OptionsChanged);
        }
        if ui
            .add_sized([200.0, 24.0], colour_button("Draw Design", DISPLAY_GREEN))
            .clicked()
        {
            actions.push(Action::Draw);
        }
        ui.add_space(8.0);

        ui.label(RichText::new("Automatic Designs").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.settings.auto_symmetrical, false, "Random");
            ui.radio_value(&mut self.settings.auto_symmetrical, true, "Symmetrical");
        });
        ui.horizontal(|ui| {
            ui.label("Colours");
            let current = ColourScheme::from_key(&self.settings.auto_colours).unwrap_or_default();
            egui::ComboBox::from_id_salt("colour_scheme")
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    for scheme in ColourScheme::ALL {
                        if ui.selectable_label(current == scheme, scheme.label()).clicked() {
                            self.settings.auto_colours = scheme.key().to_string();
                        }
                    }
                })
                .response
                .on_hover_text("How automatic designs pick bead colours");
        });
        ui.horizontal(|ui| {
            ui.label("Min");
            ui.add(egui::DragValue::new(&mut self.settings.auto_min_beads).range(1..=12));
            ui.label("Max");
            ui.add(egui::DragValue::new(&mut self.settings.auto_max_beads).range(1..=12));
        });
        if ui
            .add_sized([200.0, 24.0], colour_button("Automatic Pattern", AUTO_GOLD))
            .on_hover_text("Fill the pattern being edited (see \"Editing\") with an automatic pattern")
            .clicked()
        {
            actions.push(Action::AutoPattern);
        }
        let label = if self.slide_show {
            "Stop Slide Show"
        } else {
            "Slide Show of Designs"
        };
        if ui
            .add_sized([200.0, 24.0], colour_button(label, AUTO_GOLD))
            .on_hover_text("A new automatic design every 3 seconds")
            .clicked()
        {
            self.slide_show = !self.slide_show;
            self.next_slide_at = 0.0;
        }
        ui.add_space(8.0);
        if ui
            .add_sized([200.0, 24.0], colour_button("Bookmark", FILE_TAN))
            .on_hover_text("Quick save into the Bookmarks folder with an automatic name (Ctrl+B)")
            .clicked()
        {
            actions.push(Action::QuickBookmark);
        }
        if ui
            .add_sized([200.0, 24.0], colour_button("Open…", FILE_TAN))
            .on_hover_text("Open a saved design or a 2013 bookmark (Ctrl+O)")
            .clicked()
        {
            actions.push(Action::Open);
        }
        if ui
            .add_sized([200.0, 24.0], colour_button("Save Image", FILE_TAN))
            .clicked()
        {
            actions.push(Action::SaveImage);
        }
        if ui.add_sized([200.0, 24.0], colour_button("Quit", FILE_TAN)).clicked() {
            actions.push(Action::Quit);
        }
    }

    fn options_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        ui.columns(3, |cols| {
            // ---- Clasp ----
            let ui = &mut cols[0];
            ui.heading("Clasp");
            if ui.checkbox(&mut self.design.use_clasp, "Add clasp to design").changed() {
                actions.push(Action::OptionsChanged);
            }
            if self.clasps.is_empty() {
                ui.label(
                    "No clasps found. Set the Clasps folder on the \"Bead & Clasp Folders\" tab.\n\
                          Clasps are pictures named Name_both.png (with optional Name_left.png and Name_right.png).",
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("clasp_list")
                .max_height(260.0)
                .show(ui, |ui| {
                    let mut last_folder = None;
                    for (i, c) in self.clasps.iter().enumerate() {
                        if last_folder != Some(&c.folder) {
                            ui.label(RichText::new(&c.folder).strong());
                            last_folder = Some(&c.folder);
                        }
                        if ui
                            .selectable_label(self.design.clasp == Some(i), format!("   {}", c.name))
                            .clicked()
                        {
                            self.design.clasp = Some(i);
                            actions.push(Action::OptionsChanged);
                        }
                    }
                });
            if let Some(c) = self.design.clasp.and_then(|i| self.clasps.get(i)) {
                ui.separator();
                for (label, img) in [
                    ("Circular view", Some(&c.both)),
                    ("Linear left", c.left.as_ref()),
                    ("Linear right", c.right.as_ref()),
                ] {
                    let Some(img) = img else {
                        ui.label(format!("{label}: (no picture)"));
                        continue;
                    };
                    ui.label(format!("{label}: {} × {} px", img.width_px, img.height_px));
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(200.0, 80.0), Sense::hover());
                    if let Some(t) = img.path.as_deref().and_then(|p| self.images.get(&ctx, p)) {
                        paint_thumb(ui.painter(), t.texture.id(), img.width_px, img.height_px, rect, false);
                    }
                }
            }

            // ---- Crimp ----
            let ui = &mut cols[1];
            ui.heading("Crimp Beads");
            ui.horizontal(|ui| {
                ui.label("Crimp beads at each end of the strand:");
                for n in 0..=2u32 {
                    if ui
                        .radio_value(&mut self.design.crimps_per_end, n, n.to_string())
                        .on_hover_text(match n {
                            0 => "No crimp beads",
                            1 => "One crimp bead next to each half of the clasp",
                            _ => "Two crimp beads next to each half of the clasp (stronger)",
                        })
                        .changed()
                    {
                        actions.push(Action::OptionsChanged);
                    }
                }
            });
            let per_end = self.design.crimps_per_end;
            ui.label(format!(
                "{} crimp bead{} per end = {} in the necklace",
                per_end,
                if per_end == 1 { "" } else { "s" },
                per_end * 2
            ));
            if self.crimps.is_empty() {
                ui.label("No crimps found. Set the Crimps folder on the \"Bead & Clasp Folders\" tab.");
            }
            egui::ScrollArea::vertical()
                .id_salt("crimp_list")
                .max_height(260.0)
                .show(ui, |ui| {
                    for (i, c) in self.crimps.all().iter().enumerate() {
                        if ui
                            .selectable_label(self.design.crimp == Some(i), c.name.as_str())
                            .clicked()
                        {
                            self.design.crimp = Some(i);
                            actions.push(Action::OptionsChanged);
                        }
                    }
                });
            if let Some(c) = self.design.crimp.and_then(|i| self.crimps.get(i)) {
                ui.separator();
                ui.label(format!(
                    "{} × {} px  ({:.1} × {:.1} mm), shown enlarged:",
                    c.width_px,
                    c.height_px,
                    catalog::px_to_mm(c.width_px as f64),
                    catalog::px_to_mm(c.height_px as f64)
                ));
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(80.0), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::WHITE);
                if let Some(t) = c.image_path.as_deref().and_then(|p| self.images.get(&ctx, p)) {
                    paint_thumb(
                        ui.painter(),
                        t.texture.id(),
                        c.width_px,
                        c.height_px,
                        rect.shrink(8.0),
                        false,
                    );
                }
            }

            // ---- Background ----
            let ui = &mut cols[2];
            ui.heading("Design Background");
            for choice in Background::CHOICES {
                if ui
                    .radio_value(&mut self.settings.background, choice, choice.label())
                    .changed()
                {
                    actions.push(Action::OptionsChanged);
                }
            }
            ui.horizontal(|ui| {
                let mut rgb = self.settings.background.rgb();
                let is_custom = matches!(self.settings.background, Background::Custom(_));
                if ui.radio(is_custom, "Custom:").clicked() && !is_custom {
                    self.settings.background = Background::Custom(rgb);
                    actions.push(Action::OptionsChanged);
                }
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    self.settings.background = Background::Custom(rgb);
                    actions.push(Action::OptionsChanged);
                }
            });

            // ---- Screen size ----
            ui.add_space(12.0);
            ui.heading("Screen Size");
            let mut scale = self.settings.screen_scale;
            let shown = if scale == 0 {
                "Fit to screen".to_string()
            } else {
                format!("{scale}%")
            };
            egui::ComboBox::from_id_salt("screen_scale")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut scale, 0, "Fit to screen");
                    for p in [60, 70, 80, 90, 100, 110, 125, 150] {
                        ui.selectable_value(&mut scale, p, format!("{p}%"));
                    }
                });
            if scale != self.settings.screen_scale {
                self.settings.screen_scale = scale;
                self.save_settings();
            }
            ui.label(
                RichText::new(
                    "Fit to screen makes everything smaller on tablets and small laptops; \
                     on phones the page also scrolls.",
                )
                .small()
                .weak(),
            );
        });
    }

    fn folders_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Folders");
        ui.label("Bead pictures are read from the Beads folder and all its sub-folders.");
        ui.label(
            "Choosing a Main Folder fills in every folder below that it contains (Beads, Clasps, Crimps, Pendants, \
             Bookmarks, Images, AutomaticDesigns). You can still change any of them on its own.",
        );
        ui.add_space(6.0);
        egui::Grid::new("folders")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                for (kind, label) in FOLDERS {
                    ui.label(RichText::new(label).strong());
                    let path = self.folder_mut(kind).clone();
                    let exists = !path.is_empty() && platform::is_dir(Path::new(&path));
                    let shown = if path.is_empty() {
                        "(not chosen)".to_string()
                    } else {
                        path
                    };
                    ui.label(if exists {
                        RichText::new(shown)
                    } else {
                        RichText::new(shown).color(Color32::from_rgb(180, 60, 40))
                    });
                    if ui.add(colour_button("Choose…", FILE_TAN)).clicked() {
                        actions.push(Action::ChooseFolder(kind));
                    }
                    ui.end_row();
                }
            });
        ui.add_space(10.0);
        ui.label(format!(
            "Found: {} beads in {} folders, {} clasps, {} crimps, {} pendants.",
            self.catalog.len(),
            self.bead_subfolders.len(),
            self.clasps.len(),
            self.crimps.len(),
            self.pendants.len()
        ));
        ui.add_space(10.0);
        ui.label(format!("Settings are saved in: {}", self.settings_path.display()));
        ui.label(
            "Tip: put a BunchABeads.ini next to BunchABeads.exe to keep the settings with the program (portable mode).",
        );
    }

    /// Linear Design: the straight strand, with magnify and scroll bars.
    fn linear_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        const MIN_ZOOM: f64 = 100.0;
        const MAX_ZOOM: f64 = 2500.0;
        const SCROLLBAR: f32 = 16.0;
        let avail_w = (ui.available_width() - SCROLLBAR).max(100.0);

        ui.horizontal(|ui| {
            ui.label("Magnify:");
            if ui
                .add(colour_button(" − ", DISPLAY_GREEN))
                .on_hover_text("Zoom out")
                .clicked()
            {
                self.linear_zoom /= 1.25;
            }
            ui.add(
                egui::DragValue::new(&mut self.linear_zoom)
                    .range(MIN_ZOOM..=MAX_ZOOM)
                    .speed(5.0)
                    .suffix(" %"),
            );
            if ui
                .add(colour_button(" + ", DISPLAY_GREEN))
                .on_hover_text("Zoom in")
                .clicked()
            {
                self.linear_zoom *= 1.25;
            }
            if ui
                .add(colour_button("Fit", DISPLAY_GREEN))
                .on_hover_text("Whole strand fits the window")
                .clicked()
            {
                self.linear_zoom = MIN_ZOOM;
            }
            if ui
                .add(colour_button("Actual Size", DISPLAY_GREEN))
                .on_hover_text("Bead pictures at their full 300 dpi size")
                .clicked()
            {
                let (min_x, max_x) = self.linear.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                    (lo.min(p.cx - p.width / 2.0), hi.max(p.cx + p.width / 2.0))
                });
                if max_x > min_x {
                    self.linear_zoom = 100.0 * (max_x - min_x + 40.0) / avail_w as f64;
                }
            }
            if ui.add(colour_button("Save Image", FILE_TAN)).clicked() {
                actions.push(Action::SaveImage);
            }
            if ui.add(colour_button("Print / PDF", FILE_TAN)).clicked() {
                actions.push(Action::PrintPdf);
            }
            ui.label("Tip: Ctrl + mouse wheel also zooms. Drag the picture or use the scroll bars to move along it.");
        });
        self.linear_zoom = self.linear_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        ui.separator();

        let avail_h = (ui.available_height() - SCROLLBAR).max(100.0);
        let width = avail_w * (self.linear_zoom / 100.0) as f32;
        let height = (width * Fit::aspect(&self.linear) as f32 + 20.0).max(avail_h);
        let (grid, bg) = (self.settings.show_grid, self.settings.background);
        let response = egui::ScrollArea::both()
            .id_salt("linear_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                design_canvas_sized(
                    ui,
                    Vec2::new(width, height),
                    &mut self.images,
                    &parts!(self),
                    &self.linear,
                    grid,
                    bg,
                    None,
                )
            })
            .inner;
        if response.hovered() {
            let z = ui.input(|i| i.zoom_delta()) as f64;
            if z != 1.0 {
                self.linear_zoom = (self.linear_zoom * z).clamp(MIN_ZOOM, MAX_ZOOM);
            }
        }
    }

    /// Edit Patterns: the manual design, built from patterns 1-5 and single beads.
    fn edit_patterns_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.edit_tool = EditTool::None;
        }
        let tool = self.edit_tool;

        if self.design.mode != DesignMode::Manual {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        "The Main tab is showing a pattern design. Changes here switch it to the Manual Edit design.",
                    )
                    .color(Color32::from_rgb(160, 80, 20)),
                );
                if ui
                    .add(colour_button("Show Manual Edit Design", DISPLAY_GREEN))
                    .clicked()
                {
                    actions.push(Action::UseManual);
                }
            });
        }

        // ---- start from / whole-design buttons ----
        ui.horizontal_wrapped(|ui| {
            ui.label("Start from the Main design as:");
            egui::ComboBox::from_id_salt("copy_from")
                .selected_text(match self.copy_from_mode {
                    DesignMode::OnePatternMirrored => "1 Pattern - Mirrored",
                    DesignMode::TwoPattern => "2 Patterns",
                    _ => "1 Pattern - Linear",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.copy_from_mode,
                        DesignMode::OnePatternLinear,
                        "1 Pattern - Linear",
                    );
                    ui.selectable_value(
                        &mut self.copy_from_mode,
                        DesignMode::OnePatternMirrored,
                        "1 Pattern - Mirrored",
                    );
                    ui.selectable_value(&mut self.copy_from_mode, DesignMode::TwoPattern, "2 Patterns");
                });
            if ui
                .add(colour_button("Copy Main Design", PATTERN_BLUE))
                .on_hover_text("Replace the manual design with the Main design (same length and Add/Sub settings)")
                .clicked()
            {
                actions.push(Action::ManCopyMain);
            }
            if ui
                .add(colour_button("Auto Generate 5 Patterns", AUTO_GOLD))
                .on_hover_text("Fill patterns 1-5 with automatic patterns (Ctrl+Z undoes it)")
                .clicked()
            {
                actions.push(Action::ManAuto5);
            }
            if ui
                .add(colour_button("Split ALL Patterns Apart", PATTERN_BLUE))
                .on_hover_text("Turn every pattern in the design into single beads")
                .clicked()
            {
                actions.push(Action::ManSplitAll);
            }
            if ui.add(colour_button("Clear Design", PATTERN_BLUE)).clicked() {
                actions.push(Action::ClearManual);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Add and insert patterns:");
            ui.radio_value(&mut self.edit_mirrored, false, "Linear (as shown)");
            ui.radio_value(&mut self.edit_mirrored, true, "Mirrored (reversed)");
        });
        ui.separator();

        // ---- patterns 1-5 and the chosen bead ----
        let tool_button = |ui: &mut egui::Ui, label: &str, this: EditTool, tip: &str, actions: &mut Vec<Action>| {
            let on = tool == this;
            let fill = if on { SELECTED_GOLD } else { PATTERN_BLUE };
            if ui
                .add(colour_button(label, fill).selected(on))
                .on_hover_text(tip)
                .clicked()
            {
                actions.push(Action::ManTool(this));
            }
        };
        egui::Grid::new("manual_patterns").spacing([8.0, 4.0]).show(ui, |ui| {
            for p in 0..MAX_PATTERNS {
                ui.label(
                    RichText::new(format!("Pattern {}", p + 1))
                        .strong()
                        .color(PATTERN_COLOURS[p]),
                );
                let beads = self.design.patterns[p].beads();
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(300.0, 26.0), Sense::click());
                let fill = if p == self.current_pat {
                    Color32::from_rgb(225, 240, 255)
                } else {
                    Color32::WHITE
                };
                ui.painter().rect_filled(rect, 3.0, fill);
                ui.painter().rect_stroke(
                    rect,
                    3.0,
                    Stroke::new(1.5_f32, PATTERN_COLOURS[p]),
                    egui::StrokeKind::Inside,
                );
                let mut x = rect.left() + 3.0;
                for b in &beads {
                    let Some(info) = self.catalog.get(*b) else { continue };
                    let Some(img) = info.image_path.as_deref().and_then(|path| self.images.get(&ctx, path)) else {
                        continue;
                    };
                    let h = rect.height() - 6.0;
                    let w = h * info.width_px as f32 / info.height_px.max(1) as f32;
                    if x + w > rect.right() {
                        break;
                    }
                    let r = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(w, h));
                    ui.painter().image(img.texture.id(), r, full_uv(), Color32::WHITE);
                    x += w + 1.0;
                }
                if beads.is_empty() {
                    ui.painter().text(
                        rect.left_center() + Vec2::new(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        "(empty: click to choose beads on the Main tab)",
                        egui::FontId::proportional(12.0),
                        Color32::GRAY,
                    );
                }
                if resp
                    .on_hover_text(format!(
                        "Click to select Pattern {} (its beads are changed on the Main tab)",
                        p + 1
                    ))
                    .clicked()
                {
                    actions.push(Action::GoEditPattern(p));
                }
                ui.label(format!("× {}", self.design.manual.count_pattern(p)));
                if ui
                    .add(colour_button("+ Add", PATTERN_BLUE))
                    .on_hover_text("Add to the end of the design")
                    .clicked()
                {
                    actions.push(Action::ManAddPattern(p));
                }
                if ui
                    .add(colour_button("− Sub", PATTERN_BLUE))
                    .on_hover_text("Remove its last use")
                    .clicked()
                {
                    actions.push(Action::ManRemovePattern(p));
                }
                tool_button(
                    ui,
                    "Insert",
                    EditTool::InsertPattern(p),
                    "Then click a gap in the design",
                    actions,
                );
                tool_button(
                    ui,
                    "Replace",
                    EditTool::ReplacePattern(p),
                    "Then click an item in the design",
                    actions,
                );
                ui.end_row();
            }
            // The bead chosen in the bead list (Main tab).
            ui.label(RichText::new("Bead").strong());
            let (rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 26.0), Sense::hover());
            ui.painter().rect_filled(rect, 3.0, Color32::WHITE);
            match self.list_selection.and_then(|b| self.catalog.get(b)) {
                Some(info) => {
                    let h = rect.height() - 6.0;
                    let w = h * info.width_px as f32 / info.height_px.max(1) as f32;
                    if let Some(img) = info.image_path.as_deref().and_then(|path| self.images.get(&ctx, path)) {
                        let r = Rect::from_min_size(rect.left_top() + Vec2::new(3.0, 3.0), Vec2::new(w, h));
                        ui.painter().image(img.texture.id(), r, full_uv(), Color32::WHITE);
                    }
                    ui.painter().text(
                        rect.left_center() + Vec2::new(w + 10.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &info.name,
                        egui::FontId::proportional(13.0),
                        Color32::DARK_GRAY,
                    );
                }
                None => {
                    ui.painter().text(
                        rect.left_center() + Vec2::new(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        "(choose a bead in the list on the Main tab)",
                        egui::FontId::proportional(12.0),
                        Color32::GRAY,
                    );
                }
            }
            ui.label("");
            if ui
                .add(colour_button("+ Add", PATTERN_BLUE))
                .on_hover_text("Add this bead to the end")
                .clicked()
            {
                actions.push(Action::ManAddBead);
            }
            ui.label("");
            tool_button(
                ui,
                "Insert",
                EditTool::InsertBead,
                "Then click a gap in the design",
                actions,
            );
            tool_button(
                ui,
                "Replace",
                EditTool::ReplaceBead,
                "Then click an item in the design",
                actions,
            );
            ui.end_row();
        });
        ui.horizontal(|ui| {
            tool_button(
                ui,
                "Delete",
                EditTool::Delete,
                "Then click items in the design to delete them",
                actions,
            );
            tool_button(
                ui,
                "Split a Pattern Apart",
                EditTool::Split,
                "Then click a pattern in the design",
                actions,
            );
            let hint = match tool {
                EditTool::None => "Right-click an item for more choices (move, flip, delete, split).".to_string(),
                EditTool::InsertPattern(p) => format!("Click a gap (green line) to insert Pattern {}.", p + 1),
                EditTool::InsertBead => "Click a gap (green line) to insert the bead.".into(),
                EditTool::ReplacePattern(p) => format!("Click an item to replace it with Pattern {}.", p + 1),
                EditTool::ReplaceBead => "Click an item to replace it with the bead.".into(),
                EditTool::Delete => "Click items to delete them.".into(),
                EditTool::Split => "Click a pattern to split it into single beads.".into(),
            };
            ui.label(RichText::new(hint).italics());
            if tool != EditTool::None && ui.button("Done (Esc)").clicked() {
                actions.push(Action::ManTool(EditTool::None));
            }
        });
        ui.separator();

        // ---- the design as a row of item blocks, with gaps for inserting ----
        let mut preview = self.design.clone();
        preview.mode = DesignMode::Manual;
        let strand = build_strand(&preview, &self.catalog);
        let parts = parts!(self);
        ui.label(format!(
            "Design: {} items, {} beads, {:.2} in",
            preview.manual.len(),
            strand.len(),
            if strand.is_empty() {
                0.0
            } else {
                length_inches(&strand, &preview, &parts)
            }
        ));
        let inserting = matches!(tool, EditTool::InsertPattern(_) | EditTool::InsertBead);
        let item_tool = matches!(
            tool,
            EditTool::ReplacePattern(_) | EditTool::ReplaceBead | EditTool::Delete | EditTool::Split
        );
        let patterns = self.design.patterns;
        egui::ScrollArea::vertical()
            .id_salt("manual_items")
            .max_height(170.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                ui.horizontal_wrapped(|ui| {
                    let n = self.design.manual.len();
                    if n == 0 {
                        ui.label("No items yet: use + Add, or Copy Main Design.");
                    }
                    for i in 0..=n {
                        // Gap before item i (and one at the very end).
                        let (gap, gap_resp) = ui.allocate_exact_size(Vec2::new(9.0, BLOCK_H), Sense::click());
                        let colour = if inserting && gap_resp.hovered() {
                            Color32::from_rgb(0, 170, 0)
                        } else if inserting {
                            Color32::from_rgb(120, 200, 120)
                        } else {
                            Color32::from_gray(215)
                        };
                        let width = if inserting { 3.0_f32 } else { 1.0_f32 };
                        ui.painter()
                            .line_segment([gap.center_top(), gap.center_bottom()], Stroke::new(width, colour));
                        if inserting && gap_resp.clicked() {
                            actions.push(Action::ManClickGap(i));
                        }
                        if i == n {
                            break;
                        }

                        // Item block: its beads drawn side by side.
                        let item = self.design.manual.items[i];
                        let beads = item.beads(&patterns);
                        let width: f32 = beads
                            .iter()
                            .map(|&b| (self.catalog.width_of(b) as f32 * BLOCK_SCALE).max(4.0))
                            .sum::<f32>()
                            .max(16.0)
                            + 6.0;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, BLOCK_H), Sense::click());
                        let (border, label) = match item {
                            ManualItem::Pattern { pattern, mirrored } => (
                                PATTERN_COLOURS[pattern.min(MAX_PATTERNS - 1)],
                                format!("{}{}", pattern + 1, if mirrored { "m" } else { "" }),
                            ),
                            ManualItem::Bead(_) => (Color32::from_gray(150), String::new()),
                        };
                        let hot = item_tool && resp.hovered();
                        ui.painter()
                            .rect_filled(rect, 3.0, if hot { SELECTED_GOLD } else { Color32::WHITE });
                        ui.painter().rect_stroke(
                            rect,
                            3.0,
                            Stroke::new(if hot { 3.0_f32 } else { 1.5_f32 }, border),
                            egui::StrokeKind::Inside,
                        );
                        let mut x = rect.left() + 3.0;
                        for &b in &beads {
                            let w = (self.catalog.width_of(b) as f32 * BLOCK_SCALE).max(4.0);
                            if let Some(info) = self.catalog.get(b) {
                                let h = (info.height_px as f32 * BLOCK_SCALE).min(BLOCK_H - 12.0);
                                let w_img = info.width_px as f32 * BLOCK_SCALE * h
                                    / (info.height_px as f32 * BLOCK_SCALE).max(1.0);
                                let r = Rect::from_center_size(
                                    Pos2::new(x + w / 2.0, rect.center().y - 3.0),
                                    Vec2::new(w_img.min(w + 4.0), h),
                                );
                                if let Some(img) =
                                    info.image_path.as_deref().and_then(|path| self.images.get(&ctx, path))
                                {
                                    ui.painter().image(img.texture.id(), r, full_uv(), Color32::WHITE);
                                }
                            }
                            x += w;
                        }
                        if !label.is_empty() {
                            ui.painter().text(
                                rect.right_bottom() + Vec2::new(-3.0, -1.0),
                                egui::Align2::RIGHT_BOTTOM,
                                &label,
                                egui::FontId::proportional(10.0),
                                border,
                            );
                        }
                        let tip = match item {
                            ManualItem::Pattern { pattern, mirrored } => format!(
                                "Pattern {}{} ({} beads)",
                                pattern + 1,
                                if mirrored { ", mirrored" } else { "" },
                                beads.len()
                            ),
                            ManualItem::Bead(b) => self.catalog.get(b).map(|i| i.name.clone()).unwrap_or_default(),
                        };
                        let resp = resp.on_hover_text(tip);
                        if item_tool && resp.clicked() {
                            actions.push(Action::ManClickItem(i));
                        }
                        let is_pattern = matches!(item, ManualItem::Pattern { .. });
                        resp.context_menu(|ui| {
                            let mut choice = |ui: &mut egui::Ui, label: &str, a: Action| {
                                if ui.button(label).clicked() {
                                    actions.push(a);
                                    ui.close_menu();
                                }
                            };
                            choice(ui, "◀ Move Left", Action::ManMove(i, true));
                            choice(ui, "Move Right ▶", Action::ManMove(i, false));
                            if is_pattern {
                                choice(ui, "Flip (Linear ↔ Mirrored)", Action::ManFlip(i));
                                choice(ui, "Split Apart", Action::ManSplit(i));
                            }
                            choice(ui, "Delete", Action::ManDelete(i));
                        });
                    }
                });
            });
        ui.separator();

        // ---- the result ----
        let placed = layout_circular(&strand, &preview, &parts);
        let (grid, bg) = (self.settings.show_grid, self.settings.background);
        design_canvas(ui, &mut self.images, &parts, &placed, grid, bg);
    }

    /// The current design laid out for the Arc tab (Hanging or 2013 Arc view).
    fn arc_layout(&self) -> Vec<Placed> {
        let strand = build_strand(&self.design, &self.catalog);
        let parts = parts!(self);
        if self.settings.arc_hanging {
            layout_hanging(
                &strand,
                &self.design,
                &parts,
                self.settings.hand_left,
                self.settings.hand_right,
            )
        } else {
            layout_arc(&strand, &self.design, &parts, self.settings.arc_angle)
        }
    }

    /// Inventory: how many of each part you have and what one costs.
    fn inventory_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // How many of each part this design uses (by name).
        let in_design: std::collections::HashMap<String, usize> = self
            .cost_lines
            .iter()
            .map(|l| (inventory::base_name(&l.name).to_lowercase(), l.needed))
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label("Find:");
            ui.text_edit_singleline(&mut self.inventory_filter);
            ui.checkbox(&mut self.inventory_only_design, "Only parts in this design");
            ui.label("Currency:");
            if ui
                .add(egui::TextEdit::singleline(&mut self.settings.currency).desired_width(30.0))
                .lost_focus()
            {
                self.save_settings();
                self.draw_design();
            }
            if ui
                .add(colour_button("Export Shopping List…", FILE_TAN))
                .on_hover_text("What to buy for this design, as a spreadsheet (CSV) file")
                .clicked()
            {
                let csv = inventory::shopping_list_csv(&self.cost_lines);
                if let Some(path) = platform::save_file("Shopping list.csv", Filter("CSV spreadsheet", &["csv"]), None)
                {
                    self.status = match platform::write(&path, csv) {
                        Ok(()) => format!("Saved {}", path.display()),
                        Err(e) => format!("Could not save {}: {e}", path.display()),
                    };
                }
            }
        });
        ui.label(
            RichText::new(format!(
                "Saved automatically to {}. It's a CSV file, so it also opens in a spreadsheet.",
                self.inventory_path.display()
            ))
            .small(),
        );
        ui.separator();

        // Every part by name: beads, clasps, crimps, pendants.
        let mut rows: Vec<(String, &'static str, Option<PathBuf>, Option<[u8; 3]>)> = Vec::new();
        for (i, b) in self.catalog.all().iter().enumerate() {
            let rgb = self.colours.get(i).copied().flatten().map(|c| c.rgb);
            rows.push((b.name.clone(), "Bead", b.image_path.clone(), rgb));
        }
        for c in &self.clasps {
            rows.push((c.name.clone(), "Clasp", c.both.path.clone(), None));
        }
        for c in self.crimps.all() {
            rows.push((c.name.clone(), "Crimp", c.image_path.clone(), None));
        }
        for p in self.pendants.all() {
            if !rows.iter().any(|r| r.0 == p.name) {
                rows.push((p.name.clone(), "Pendant", p.image_path.clone(), None));
            }
        }
        let filter = self.inventory_filter.to_lowercase();
        let only_design = self.inventory_only_design;
        rows.retain(|r| {
            (filter.is_empty() || r.0.to_lowercase().contains(&filter))
                && (!only_design || in_design.contains_key(&r.0.to_lowercase()))
        });

        ui.horizontal(|ui| {
            let columns: [(&str, f32); 7] = [
                ("", 28.0),
                ("Name", 260.0),
                ("Kind", 60.0),
                ("In design", 70.0),
                ("Have", 80.0),
                ("Price each", 90.0),
                ("To buy", 60.0),
            ];
            for (text, w) in columns {
                ui.add_sized([w, 18.0], egui::Label::new(RichText::new(text).strong()));
            }
        });
        let cur = self.settings.currency.clone();
        let mut changed = false;
        let row_h = 26.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows.len(), |ui, range| {
                for (name, kind, path, rgb) in &rows[range] {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 22.0), Sense::hover());
                        if let Some(img) = path.as_deref().and_then(|p| self.images.get(&ctx, p)) {
                            let size = img.texture.size_vec2();
                            let s = (rect.height() / size.y).min(rect.width() / size.x);
                            ui.painter().image(
                                img.texture.id(),
                                Rect::from_center_size(rect.center(), size * s),
                                full_uv(),
                                Color32::WHITE,
                            );
                        } else if let Some(c) = rgb {
                            ui.painter()
                                .rect_filled(rect.shrink(4.0), 2.0, Color32::from_rgb(c[0], c[1], c[2]));
                        }
                        ui.add_sized([260.0, 20.0], egui::Label::new(name.as_str()).truncate());
                        ui.add_sized([60.0, 20.0], egui::Label::new(*kind));
                        let used = in_design.get(&name.to_lowercase()).copied().unwrap_or(0);
                        let used_text = if used > 0 { used.to_string() } else { String::new() };
                        ui.add_sized([70.0, 20.0], egui::Label::new(used_text));
                        let mut stock: Stock = self.inventory.get(name);
                        let a = ui.add_sized([80.0, 20.0], egui::DragValue::new(&mut stock.on_hand).range(0..=99_999));
                        let b = ui.add_sized(
                            [90.0, 20.0],
                            egui::DragValue::new(&mut stock.price_each)
                                .range(0.0..=99_999.0)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .prefix(cur.as_str()),
                        );
                        if a.changed() || b.changed() {
                            self.inventory.set(name, stock);
                            changed = true;
                        }
                        let buy = used.saturating_sub(stock.on_hand as usize);
                        let text = if buy > 0 {
                            RichText::new(buy.to_string()).color(Color32::from_rgb(190, 60, 30))
                        } else {
                            RichText::new("")
                        };
                        ui.add_sized([60.0, 20.0], egui::Label::new(text));
                    });
                }
            });
        if changed {
            self.save_inventory();
            self.draw_design();
        }
    }

    /// Arc Design: the necklace held up by its clasp ends.
    fn arc_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        let mut fit_now = false;
        ui.horizontal_wrapped(|ui| {
            let mut changed = ui
                .radio_value(&mut self.settings.arc_hanging, true, "Hanging (drag the hands)")
                .on_hover_text("The necklace drapes between two hands like a real chain")
                .changed();
            changed |= ui
                .radio_value(&mut self.settings.arc_hanging, false, "Arc (2013 style)")
                .on_hover_text("The necklace follows part of a circle")
                .changed();
            ui.separator();
            if self.settings.arc_hanging {
                if ui.add(colour_button("Reset Hands", DISPLAY_GREEN)).clicked() {
                    self.settings.hand_left = beads_core::settings::DEFAULT_HAND_LEFT;
                    self.settings.hand_right = beads_core::settings::DEFAULT_HAND_RIGHT;
                    self.hang_zoom = 1.0;
                    self.hang_center = None;
                    changed = true;
                }
                ui.separator();
                ui.label("Magnify:");
                if ui
                    .add(colour_button(" − ", DISPLAY_GREEN))
                    .on_hover_text("Zoom out")
                    .clicked()
                {
                    self.hang_zoom /= 1.25;
                }
                let mut percent = self.hang_zoom * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(20.0..=500.0)
                            .speed(2.0)
                            .suffix(" %"),
                    )
                    .changed()
                {
                    self.hang_zoom = percent / 100.0;
                }
                if ui
                    .add(colour_button(" + ", DISPLAY_GREEN))
                    .on_hover_text("Zoom in")
                    .clicked()
                {
                    self.hang_zoom *= 1.25;
                }
                if ui
                    .add(colour_button("Fit", DISPLAY_GREEN))
                    .on_hover_text("Fit the necklace as it hangs now into the window")
                    .clicked()
                {
                    fit_now = true;
                }
                self.hang_zoom = self.hang_zoom.clamp(0.2, 5.0);
                ui.label(
                    RichText::new(
                        "Drag a hand to move it, drag the background to move the view, Ctrl + mouse wheel zooms.",
                    )
                    .italics(),
                );
            } else {
                ui.label("Angle:");
                if ui.add(colour_button("− 10°", DISPLAY_GREEN)).clicked() {
                    self.settings.arc_angle = (self.settings.arc_angle - 10.0).max(10.0);
                    changed = true;
                }
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.settings.arc_angle)
                            .range(10.0..=360.0)
                            .speed(1.0)
                            .suffix("°"),
                    )
                    .changed();
                if ui.add(colour_button("+ 10°", DISPLAY_GREEN)).clicked() {
                    self.settings.arc_angle = (self.settings.arc_angle + 10.0).min(360.0);
                    changed = true;
                }
                ui.label(RichText::new("Tip: drag left/right on the picture to change the angle.").italics());
            }
            ui.separator();
            if ui.add(colour_button("Save Image", FILE_TAN)).clicked() {
                actions.push(Action::SaveImage);
            }
            if changed {
                self.save_settings();
            }
        });
        ui.separator();

        let strand = build_strand(&self.design, &self.catalog);
        let parts = parts!(self);
        let total = open_length_px(&strand, &self.design, &parts);
        let placed = self.arc_layout();

        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let area = resp.rect;
        paint_background(&painter, area, self.settings.show_grid, self.settings.background);
        if placed.is_empty() || total <= 0.0 {
            painter.text(
                area.left_top() + Vec2::splat(10.0),
                egui::Align2::LEFT_TOP,
                "Add beads to a pattern on the Main tab first.",
                egui::FontId::proportional(14.0),
                Color32::GRAY,
            );
            return;
        }

        if !self.settings.arc_hanging {
            if let Some(fit) = Fit::compute(&placed, area.width() as f64, area.height() as f64, 10.0) {
                paint_pieces(&painter, area, &mut self.images, &parts, &placed, fit, &ctx);
            }
            // Drag sideways to open or close the arc.
            if resp.dragged() {
                let dx = resp.drag_delta().x as f64;
                self.settings.arc_angle = (self.settings.arc_angle + dx * 0.4).clamp(10.0, 360.0);
            }
            if resp.drag_stopped() {
                self.save_settings();
            }
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            return;
        }

        // Hanging: a steady view (it doesn't jump while a hand is dragged). At 100 %
        // it's sized to the necklace held by the hands in their starting places,
        // which matches the size of the 2013 arc view.
        let base = render::bounds(&layout_hanging(
            &strand,
            &self.design,
            &parts,
            beads_core::settings::DEFAULT_HAND_LEFT,
            beads_core::settings::DEFAULT_HAND_RIGHT,
        ));
        let Some((bx0, by0, bx1, by1)) = base else { return };
        let (base_w, base_h) = ((bx1 - bx0) * 1.06, (by1 - by0) * 1.06);
        if fit_now {
            if let Some((x0, y0, x1, y1)) = render::bounds(&placed) {
                self.hang_center = Some(((x0 + x1) / 2.0 / total, (y0 + y1) / 2.0 / total));
                self.hang_zoom = (base_w / ((x1 - x0) * 1.06))
                    .min(base_h / ((y1 - y0) * 1.06))
                    .clamp(0.2, 5.0);
            }
        }
        // Ctrl + mouse wheel zooms.
        if resp.hovered() {
            let z = ctx.input(|i| i.zoom_delta()) as f64;
            if z != 1.0 {
                self.hang_zoom = (self.hang_zoom * z).clamp(0.2, 5.0);
            }
        }
        let (cx, cy) = self
            .hang_center
            .map(|c| (c.0 * total, c.1 * total))
            .unwrap_or(((bx0 + bx1) / 2.0, (by0 + by1) / 2.0));
        let (half_w, half_h) = (base_w / 2.0 / self.hang_zoom, base_h / 2.0 / self.hang_zoom);
        let view = (cx - half_w, cy - half_h, cx + half_w, cy + half_h);
        let Some(fit) = Fit::for_box(view, area.width() as f64, area.height() as f64, 10.0) else {
            return;
        };
        paint_pieces(&painter, area, &mut self.images, &parts, &placed, fit, &ctx);

        let to_screen = |frac: (f64, f64)| {
            let (x, y) = fit.apply(frac.0 * total, frac.1 * total);
            area.min + Vec2::new(x as f32, y as f32)
        };
        let hands = [self.settings.hand_left, self.settings.hand_right];
        let pointer = resp.hover_pos();

        // Start dragging the hand nearest the pointer.
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (best, dist) = (0..2)
                    .map(|i| (i, to_screen(hands[i]).distance(pos)))
                    .fold((0, f32::MAX), |acc, x| if x.1 < acc.1 { x } else { acc });
                self.arc_drag = (dist < 40.0).then_some(best);
                // Not on a hand: drag the background to move the view.
                self.arc_pan = self.arc_drag.is_none();
            }
        }
        if self.arc_pan && resp.dragged() {
            let d = resp.drag_delta();
            let (c0, c1) = self
                .hang_center
                .unwrap_or(((bx0 + bx1) / 2.0 / total, (by0 + by1) / 2.0 / total));
            self.hang_center = Some((c0 - d.x as f64 / fit.scale / total, c1 - d.y as f64 / fit.scale / total));
        }
        if let (Some(i), true) = (self.arc_drag, resp.dragged()) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (x, y) = fit.unapply((pos.x - area.min.x) as f64, (pos.y - area.min.y) as f64);
                let mut frac = ((x / total).clamp(-1.0, 1.0), (y / total).clamp(-1.0, 1.0));
                let other = hands[1 - i];
                // Keep the left hand left of the right one.
                if i == 0 {
                    frac.0 = frac.0.min(other.0 - 0.01);
                } else {
                    frac.0 = frac.0.max(other.0 + 0.01);
                }
                // The hands can't be further apart than the necklace is long.
                let (dx, dy) = (frac.0 - other.0, frac.1 - other.1);
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 0.99 {
                    frac = (other.0 + dx * 0.99 / dist, other.1 + dy * 0.99 / dist);
                }
                if i == 0 {
                    self.settings.hand_left = frac;
                } else {
                    self.settings.hand_right = frac;
                }
            }
        }
        if resp.drag_stopped() {
            self.arc_drag = None;
            self.arc_pan = false;
            self.save_settings();
        }

        // Draw the hands.
        for (i, frac) in [self.settings.hand_left, self.settings.hand_right]
            .into_iter()
            .enumerate()
        {
            let c = to_screen(frac);
            let near = pointer.is_some_and(|p| p.distance(c) < 40.0);
            let active = self.arc_drag == Some(i);
            let radius = if active || near { 13.0 } else { 10.0 };
            painter.circle(
                c,
                radius,
                Color32::from_rgba_unmultiplied(240, 170, 60, if active { 230 } else { 170 }),
                Stroke::new(2.0_f32, Color32::from_rgb(150, 80, 0)),
            );
            painter.text(
                c + Vec2::new(0.0, -radius - 4.0),
                egui::Align2::CENTER_BOTTOM,
                if i == 0 { "left hand" } else { "right hand" },
                egui::FontId::proportional(11.0),
                Color32::from_rgb(150, 80, 0),
            );
            if near && self.arc_drag.is_none() {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
        if self.arc_drag.is_some() || self.arc_pan {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    fn pendants_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        egui::SidePanel::left("pendant_list")
            .resizable(true)
            .default_width(280.0)
            .show_inside(ui, |ui| {
                ui.heading("Pendants");
                let ok = self.design.pendant.is_some();
                if ui
                    .add_enabled(
                        ok,
                        egui::Checkbox::new(&mut self.design.use_pendant, "Show pendant on design"),
                    )
                    .changed()
                {
                    actions.push(Action::OptionsChanged);
                }
                if !self.settings.pendant_folder.is_empty()
                    && platform::is_dir(Path::new(&self.settings.pendant_folder))
                {
                    ui.label(RichText::new(format!("From {}", self.settings.pendant_folder)).small());
                } else {
                    ui.label(
                        RichText::new(
                            "Showing beads with \"Pendant\" in their name. To use a separate folder, \
                             set Pendants on the \"Bead & Clasp Folders\" tab.",
                        )
                        .small(),
                    );
                }
                if ui.add(colour_button("Pendants Folder…", FILE_TAN)).clicked() {
                    actions.push(Action::ChooseFolder(FolderKind::Pendants));
                }
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    ui.text_edit_singleline(&mut self.pendant_filter);
                });
                ui.separator();
                let filter = self.pendant_filter.to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if self.pendants.is_empty() {
                        ui.label("No pendants found.");
                    }
                    for (i, p) in self.pendants.all().iter().enumerate() {
                        if !filter.is_empty() && !p.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        if ui
                            .selectable_label(self.design.pendant == Some(i), p.name.as_str())
                            .clicked()
                        {
                            self.design.pendant = Some(i);
                            self.design.use_pendant = true; // picking one means "show it"
                            actions.push(Action::OptionsChanged);
                        }
                    }
                });
            });

        let Some(index) = self.design.pendant else {
            ui.add_space(20.0);
            ui.label("Pick a pendant on the left. It hangs from the middle of the necklace.");
            return;
        };
        let Some(info) = self.pendants.get(index).cloned() else {
            return;
        };
        let (w, h) = (info.width_px as f64, info.height_px as f64);
        let drop = beads_core::design::pendant_drop_px(h, info.adjust.y_offset_px);
        let mut hang = h / 2.0 - drop; // strand position, in pixels from the top of the picture

        ui.horizontal(|ui| {
            ui.label(RichText::new(&info.name).strong());
            ui.label(format!(
                "{} × {} px  ({:.1} × {:.1} mm)",
                info.width_px,
                info.height_px,
                catalog::px_to_mm(w),
                catalog::px_to_mm(h)
            ));
        });
        ui.horizontal(|ui| {
            ui.label("Hangs from:");
            let r = ui
                .add(
                    egui::DragValue::new(&mut hang)
                        .range(0.0..=h)
                        .speed(1.0)
                        .suffix(" px from top"),
                )
                .on_hover_text("Where the strand passes through the pendant's bail (the red line)");
            if r.changed() {
                self.set_pendant_hang(index, hang);
                actions.push(Action::OptionsChanged);
            }
            ui.label(format!("({:.1} mm)", catalog::px_to_mm(hang)));
            if ui
                .add(colour_button("Save Hang Point", FILE_TAN))
                .on_hover_text("Write it to the pendant's .txt file (BeadGuide layout) so it's remembered")
                .clicked()
            {
                self.save_pendant_adjust(index);
            }
            if ui
                .add(colour_button("Undo Changes", FILE_TAN))
                .on_hover_text("Re-read the .txt file")
                .clicked()
            {
                self.reload_pendant_adjust(index);
                actions.push(Action::OptionsChanged);
            }
        });
        ui.separator();

        ui.horizontal_top(|ui| {
            // The pendant picture with the strand drawn through its hang point.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 420.0), Sense::hover());
            let [r, g, b] = self.settings.background.rgb();
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(r, g, b));
            let s = ((rect.width() - 20.0) / w as f32).min((rect.height() - 20.0) / h as f32);
            let pic = Rect::from_center_size(rect.center(), Vec2::new(w as f32 * s, h as f32 * s));
            let line_y = pic.top() + hang as f32 * s;
            let strand = Stroke::new(2.0_f32, Color32::from_rgb(200, 40, 40));
            ui.painter().line_segment(
                [Pos2::new(rect.left(), line_y), Pos2::new(rect.right(), line_y)],
                strand,
            );
            if let Some(img) = info.image_path.as_deref().and_then(|p| self.images.get(&ctx, p)) {
                ui.painter().image(img.texture.id(), pic, full_uv(), Color32::WHITE);
            }
            ui.painter().line_segment(
                [Pos2::new(rect.left(), line_y), Pos2::new(pic.left(), line_y)],
                Stroke::new(3.0_f32, Color32::from_rgb(200, 40, 40)),
            );
            ui.painter().line_segment(
                [Pos2::new(pic.right(), line_y), Pos2::new(rect.right(), line_y)],
                Stroke::new(3.0_f32, Color32::from_rgb(200, 40, 40)),
            );
            // The whole design, so the effect can be seen straight away.
            let (grid, bg) = (self.settings.show_grid, self.settings.background);
            design_canvas(ui, &mut self.images, &parts!(self), &self.circular, grid, bg);
        });
    }

    fn bookmarks_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if self.bookmarks_loaded_from != self.settings.bookmarks_folder || self.bookmarks_loaded_from.is_empty() {
            self.refresh_bookmarks();
        }
        egui::SidePanel::left("bookmark_list")
            .resizable(true)
            .default_width(320.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(colour_button("Folder…", FILE_TAN)).clicked() {
                        actions.push(Action::ChooseFolder(FolderKind::Bookmarks));
                    }
                    if ui.add(colour_button("Refresh", FILE_TAN)).clicked() {
                        actions.push(Action::RefreshBookmarks);
                    }
                });
                if self.settings.bookmarks_folder.is_empty() {
                    ui.label("No Bookmarks folder yet. It is created the first time you click Bookmark.");
                } else {
                    ui.label(RichText::new(&self.settings.bookmarks_folder).small());
                }
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    ui.text_edit_singleline(&mut self.bookmark_filter);
                });
                ui.label(format!("{} designs, newest first", self.bookmarks.len()));
                ui.separator();
                let filter = self.bookmark_filter.to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, path) in self.bookmarks.iter().enumerate() {
                        let name = path
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let r = ui.selectable_label(self.bookmark_selected == Some(i), name);
                        if r.clicked() {
                            actions.push(Action::PreviewBookmark(i));
                        }
                        if r.double_clicked() {
                            actions.push(Action::OpenFile(path.clone()));
                        }
                    }
                });
            });

        // Preview of the selected design (doesn't change the current design until "Open").
        if let Some(path) = self.bookmark_selected.and_then(|i| self.bookmarks.get(i)).cloned() {
            ui.horizontal(|ui| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                ui.label(RichText::new(name).strong());
                if ui
                    .add(colour_button("Open This Design", DISPLAY_GREEN))
                    .on_hover_text("Replace the current design with this one (you can also double-click it)")
                    .clicked()
                {
                    actions.push(Action::OpenFile(path.clone()));
                }
            });
            ui.horizontal(|ui| {
                ui.label(&self.bookmark_info);
                if ui
                    .add(colour_button("Save Image", FILE_TAN))
                    .on_hover_text("Save this preview as a picture")
                    .clicked()
                {
                    actions.push(Action::SaveImage);
                }
            });
            let (grid, bg) = (self.settings.show_grid, self.settings.background);
            design_canvas(ui, &mut self.images, &parts!(self), &self.bookmark_preview, grid, bg);
        } else {
            ui.add_space(20.0);
            ui.label("Click a design on the left to preview it here. Double-click to open it.");
            ui.label("2013 bookmark files in the same folder are listed too and open the same way.");
        }
    }

    /// Makes everything smaller on screens smaller than the layout (tablets, small
    /// laptops), or uses the size chosen in Options.
    fn fit_to_screen(&mut self, ctx: &egui::Context) {
        let target = if self.settings.screen_scale == 0 {
            // The screen's size at 100%, whatever scale is in use now.
            let full = ctx.screen_rect().size() * ctx.zoom_factor();
            if full.x < 100.0 || full.y < 100.0 {
                return; // not laid out yet
            }
            (full.x / LAYOUT_SIZE.x)
                .min(full.y / LAYOUT_SIZE.y)
                .clamp(MIN_AUTO_SCALE, 1.0)
        } else {
            self.settings.screen_scale as f32 / 100.0
        };
        if (target - self.applied_zoom).abs() > 0.01 {
            ctx.set_zoom_factor(target);
            self.applied_zoom = target;
        }
    }

    // ------------------------------------------------------------ Bead Maker

    /// Re-reads the Beads folder (e.g. after new beads were saved) and puts the
    /// design back by bead name, since bead numbers change when the list does.
    fn reload_beads_keeping_design(&mut self) {
        let saved = SavedDesign::from_design(&self.design, &parts!(self), self.settings.background);
        let base = self.design.clone();
        let dirty = self.dirty;
        self.load_beads();
        let (design, _missing) = saved.to_design(&parts!(self), &base);
        self.design = design;
        self.dirty = dirty;
        self.draw_design();
    }

    /// `<Beads folder>/Generated`, asking for the Beads folder first if there isn't one.
    fn maker_root(&mut self) -> Option<PathBuf> {
        if !platform::is_dir(Path::new(&self.settings.bead_folder)) {
            self.choose_folder(FolderKind::Beads);
        }
        if self.settings.bead_folder.is_empty() || !platform::is_dir(Path::new(&self.settings.bead_folder)) {
            self.status = "Choose your Beads folder first (Bead & Clasp Folders tab).".into();
            return None;
        }
        Some(PathBuf::from(&self.settings.bead_folder).join("Generated"))
    }

    fn maker_save(&mut self) {
        let Some(root) = self.maker_root() else { return };
        let dir = root.join(self.maker.material.folder());
        // Never overwrite: RedJasper_8mmRD, then RedJasper2_8mmRD, ...
        let mut stem = self.maker.file_name(&self.maker_name);
        let mut n = 2;
        while platform::exists(&dir.join(format!("{stem}.png"))) {
            stem = self.maker.file_name(&format!("{}{n}", self.maker_name));
            n += 1;
        }
        let path = dir.join(format!("{stem}.png"));
        let img = beadmaker::render_with(&self.maker, self.maker_photo.as_ref());
        let result = platform::create_dir_all(&dir)
            .and_then(|_| encode_png(&img))
            .and_then(|png| platform::write(&path, png))
            .map_err(|e| e.to_string());
        if let Err(e) = result {
            self.status = format!("Could not save {}: {e}", path.display());
            return;
        }
        self.reload_beads_keeping_design();
        self.list_selection = self.catalog.find_by_name(&stem);
        self.status = format!(
            "Saved {stem}.png in {}. It is selected in the bead list, ready to add to a pattern.",
            dir.display()
        );
    }

    fn maker_library(&mut self) {
        if self.maker_rx.is_some() {
            return;
        }
        let Some(root) = self.maker_root() else { return };
        let library = beadmaker::sample_library();
        self.maker_total = library.len();
        self.maker_done = 0;
        self.maker_written = 0;
        self.maker_errors.clear();
        let jobs = library
            .into_iter()
            .map(|bead| -> platform::Job<Result<bool, String>> {
                let root = root.clone();
                Box::new(move || {
                    let dir = root.join(bead.folder);
                    let path = dir.join(format!("{}.png", bead.name));
                    if platform::exists(&path) {
                        return Ok(false); // keep beads already there (they may have been edited)
                    }
                    platform::create_dir_all(&dir)
                        .and_then(|_| encode_png(&beadmaker::render(&bead.spec)))
                        .and_then(|png| platform::write(&path, png))
                        .map(|_| true)
                        .map_err(|e| format!("{}: {e}", bead.name))
                })
            })
            .collect();
        self.maker_rx = Some(Jobs::start(jobs));
        self.status = format!("Making {} sample beads…", self.maker_total);
    }

    /// Progress of the sample library (called every frame).
    fn receive_maker(&mut self, ctx: &egui::Context) {
        let Some(jobs) = &mut self.maker_rx else { return };
        for result in jobs.poll() {
            self.maker_done += 1;
            match result {
                Ok(true) => self.maker_written += 1,
                Ok(false) => {}
                Err(e) => self.maker_errors.push(e),
            }
        }
        if self.maker_done < self.maker_total {
            if platform::IS_WEB {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            return;
        }
        self.maker_rx = None;
        self.reload_beads_keeping_design();
        let kept = self.maker_total - self.maker_written - self.maker_errors.len();
        self.status = match self.maker_errors.first() {
            Some(e) => format!(
                "Sample library: {} beads made, {} could not be saved ({e}).",
                self.maker_written,
                self.maker_errors.len()
            ),
            None if kept > 0 => format!(
                "Sample library: {} beads made, {kept} already there and kept. Look in the Generated folders.",
                self.maker_written
            ),
            None => format!(
                "Sample library: {} beads made in the Generated folders.",
                self.maker_written
            ),
        };
    }

    /// Redraws the preview picture if the bead settings changed.
    fn maker_refresh(&mut self, ctx: &egui::Context) {
        if self.maker_drawn == Some(self.maker)
            && self.maker_drawn_photo == self.maker_photo_path
            && self.maker_texture.is_some()
        {
            return;
        }
        let img = beadmaker::render_with(&self.maker, self.maker_photo.as_ref());
        let colour = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.pixels);
        match &mut self.maker_texture {
            Some(t) => t.set(colour, egui::TextureOptions::LINEAR),
            None => self.maker_texture = Some(ctx.load_texture("bead_maker", colour, egui::TextureOptions::LINEAR)),
        }
        self.maker_drawn = Some(self.maker);
        self.maker_drawn_photo = self.maker_photo_path.clone();
    }

    /// Photo texture: pick a stone picture (best is a seamless, close-up texture).
    fn maker_choose_photo(&mut self, ctx: &egui::Context) {
        let start = if !self.settings.textures_folder.is_empty() {
            Some(PathBuf::from(&self.settings.textures_folder))
        } else {
            self.maker_photo_path
                .as_ref()
                .and_then(|p| p.parent())
                .map(Path::to_path_buf)
        };
        // On the web the picture arrives later (see `take_picked` in update).
        if let Some(path) = platform::pick_file(ctx, Pick::Texture, Filter("Pictures", PICTURE_TYPES), start.as_deref())
        {
            self.maker_load_photo(path);
        }
    }

    /// Pictures in the Textures folder and its sub-folders, sorted by name.
    fn maker_textures(&mut self) -> &[PathBuf] {
        if self.maker_texture_list.is_none() {
            fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
                for (p, is_dir) in platform::list(dir) {
                    if is_dir && depth < 3 {
                        walk(&p, out, depth + 1);
                    } else if p
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| PICTURE_TYPES.iter().any(|t| x.eq_ignore_ascii_case(t)))
                    {
                        out.push(p);
                    }
                }
            }
            let mut list = Vec::new();
            if !self.settings.textures_folder.is_empty() {
                walk(Path::new(&self.settings.textures_folder), &mut list, 0);
            }
            list.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
            self.maker_texture_list = Some(list);
        }
        self.maker_texture_list.as_deref().unwrap_or(&[])
    }

    fn maker_load_photo(&mut self, path: PathBuf) {
        match platform::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| image::load_from_memory(&bytes).map_err(|e| e.to_string()))
        {
            Ok(img) => {
                let img = img.to_rgba8();
                let (width, height) = img.dimensions();
                self.maker_photo = Some(Rgba {
                    width,
                    height,
                    pixels: img.into_raw(),
                });
                self.maker.material = Material::Photo;
                if let Some(stem) = path.file_stem() {
                    // "marble_green-2k" -> "MarbleGreen"
                    let name: String = stem
                        .to_string_lossy()
                        .split(|c: char| !c.is_ascii_alphabetic())
                        .filter(|w| {
                            !w.is_empty()
                                && !matches!(
                                    w.to_lowercase().as_str(),
                                    "k" | "jpg" | "color" | "albedo" | "basecolor" | "diff" | "diffuse"
                                )
                        })
                        .map(|w| {
                            let mut c = w.chars();
                            c.next()
                                .map(|f| f.to_ascii_uppercase().to_string() + &c.as_str().to_lowercase())
                                .unwrap_or_default()
                        })
                        .collect();
                    if !name.is_empty() {
                        self.maker_name = name;
                    }
                }
                self.maker_preset = None;
                self.status = format!("Using {} as the bead's stone.", path.display());
                self.maker_photo_path = Some(path);
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    fn bead_maker_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let busy = self.maker_rx.is_some();
        egui::SidePanel::left("maker_controls")
            .resizable(true)
            .default_width(340.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.heading("Bead Maker");
                    ui.label("Draws a bead picture at the same scale as your photos (300 dpi).");
                    ui.add_space(6.0);
                    egui::Grid::new("maker_grid")
                        .num_columns(2)
                        .spacing([8.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Start from:");
                            let chosen = self
                                .maker_preset
                                .and_then(|i| PRESETS.get(i))
                                .map(|p| p.name.to_string())
                                .unwrap_or_else(|| "Choose a colour…".into());
                            egui::ComboBox::from_id_salt("maker_preset")
                                .selected_text(chosen)
                                .height(400.0)
                                .show_ui(ui, |ui| {
                                    let mut last = None;
                                    for (i, p) in PRESETS.iter().enumerate() {
                                        if last != Some(p.material) {
                                            if last.is_some() {
                                                ui.separator();
                                            }
                                            ui.label(RichText::new(p.material.label()).small().weak());
                                            last = Some(p.material);
                                        }
                                        if ui.selectable_label(self.maker_preset == Some(i), p.name).clicked() {
                                            self.maker_preset = Some(i);
                                            self.maker.material = p.material;
                                            self.maker.colour = p.colour;
                                            self.maker.colour2 = p.colour2;
                                            self.maker_name = p.name.to_string();
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Shape:");
                            egui::ComboBox::from_id_salt("maker_shape")
                                .selected_text(self.maker.shape.label())
                                .show_ui(ui, |ui| {
                                    for shape in Shape::ALL {
                                        if ui.selectable_label(self.maker.shape == shape, shape.label()).clicked()
                                            && self.maker.shape != shape
                                        {
                                            self.maker.shape = shape;
                                            (self.maker.width_mm, self.maker.height_mm) = shape.default_size();
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Size:");
                            ui.vertical(|ui| {
                                if self.maker.shape.is_square() {
                                    ui.add(
                                        egui::DragValue::new(&mut self.maker.width_mm)
                                            .range(1.0..=40.0)
                                            .speed(0.1)
                                            .fixed_decimals(1)
                                            .suffix(" mm"),
                                    );
                                    self.maker.height_mm = self.maker.width_mm;
                                    ui.horizontal_wrapped(|ui| {
                                        for mm in [3.0, 4.0, 6.0, 8.0, 10.0, 12.0] {
                                            if ui.small_button(format!("{mm}")).clicked() {
                                                self.maker.width_mm = mm;
                                                self.maker.height_mm = mm;
                                            }
                                        }
                                    });
                                } else {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut self.maker.width_mm)
                                                .range(1.0..=60.0)
                                                .speed(0.1)
                                                .fixed_decimals(1)
                                                .suffix(" mm"),
                                        );
                                        ui.label("along the strand");
                                    });
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut self.maker.height_mm)
                                                .range(1.0..=60.0)
                                                .speed(0.1)
                                                .fixed_decimals(1)
                                                .suffix(" mm"),
                                        );
                                        ui.label("across");
                                    });
                                }
                                let (w, h) = self.maker.pixel_size();
                                ui.label(RichText::new(format!("{w} x {h} pixels")).small().weak());
                            });
                            ui.end_row();

                            ui.label("Material:");
                            egui::ComboBox::from_id_salt("maker_material")
                                .selected_text(self.maker.material.label())
                                .show_ui(ui, |ui| {
                                    for m in Material::ALL {
                                        ui.selectable_value(&mut self.maker.material, m, m.label());
                                    }
                                });
                            ui.end_row();

                            if self.maker.material == Material::Photo {
                                ui.label("Picture:");
                                ui.vertical(|ui| {
                                    let current = self.maker_photo_path.clone();
                                    let shown = current
                                        .as_ref()
                                        .and_then(|p| p.file_name())
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "none yet (plain colour below)".into());
                                    let list = self.maker_textures().to_vec();
                                    if !list.is_empty() {
                                        egui::ComboBox::from_id_salt("maker_texture")
                                            .selected_text(shown.clone())
                                            .width(190.0)
                                            .height(400.0)
                                            .show_ui(ui, |ui| {
                                                for p in &list {
                                                    let name = p
                                                        .file_name()
                                                        .map(|n| n.to_string_lossy().into_owned())
                                                        .unwrap_or_default();
                                                    if ui.selectable_label(current.as_ref() == Some(p), name).clicked()
                                                    {
                                                        actions.push(Action::MakerUsePhoto(p.clone()));
                                                    }
                                                }
                                            });
                                    } else {
                                        ui.label(RichText::new(shown).small());
                                    }
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button("Browse…")
                                            .on_hover_text(
                                                "A close-up photo of stone. It does not need to be seamless. \
                                                 Keep these pictures outside your Beads folder.",
                                            )
                                            .clicked()
                                        {
                                            actions.push(Action::MakerChoosePhoto);
                                        }
                                        if self.settings.textures_folder.is_empty() {
                                            if ui
                                                .button("Textures Folder…")
                                                .on_hover_text("Pick a folder of stone pictures to list here")
                                                .clicked()
                                            {
                                                actions.push(Action::ChooseFolder(FolderKind::Textures));
                                            }
                                        } else if ui
                                            .small_button("⟳")
                                            .on_hover_text("Look in the Textures folder again")
                                            .clicked()
                                        {
                                            self.maker_texture_list = None;
                                        }
                                    });
                                });
                                ui.end_row();

                                ui.label("Picture covers:");
                                ui.add(
                                    egui::DragValue::new(&mut self.maker.texture_mm)
                                        .range(2.0..=300.0)
                                        .speed(0.5)
                                        .fixed_decimals(0)
                                        .suffix(" mm"),
                                )
                                .on_hover_text(
                                    "How much stone the picture's width shows. Smaller = bigger pattern on the bead.",
                                );
                                ui.end_row();
                            }

                            ui.label("Colour:");
                            ui.color_edit_button_srgb(&mut self.maker.colour);
                            ui.end_row();

                            if self.maker.material.uses_second_colour() {
                                ui.label(format!("{}:", self.maker.material.second_colour_label()));
                                ui.color_edit_button_srgb(&mut self.maker.colour2);
                                ui.end_row();
                            }

                            ui.label("Pattern:");
                            ui.horizontal(|ui| {
                                if ui
                                    .button("New Pattern")
                                    .on_hover_text(
                                        "A different stone pattern, grain, chip outline or part of the photo",
                                    )
                                    .clicked()
                                {
                                    self.maker.seed = random_seed();
                                }
                                ui.label(RichText::new(format!("#{}", self.maker.seed % 100_000)).weak());
                            });
                            ui.end_row();

                            ui.label("Name:");
                            ui.add(egui::TextEdit::singleline(&mut self.maker_name).desired_width(170.0))
                                .on_hover_text("The colour or stone name; the size and shape are added for you");
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{}.png", self.maker.file_name(&self.maker_name))).strong());
                    let target = if self.settings.bead_folder.is_empty() {
                        "your Beads folder (you will be asked for it)".to_string()
                    } else {
                        Path::new(&self.settings.bead_folder)
                            .join("Generated")
                            .join(self.maker.material.folder())
                            .display()
                            .to_string()
                    };
                    ui.label(RichText::new(format!("Saved in {target}")).small());
                    if ui
                        .add_enabled(!busy, colour_button("Save to Beads Folder", DISPLAY_GREEN))
                        .on_hover_text("Adds this bead to your bead list (never overwrites a bead)")
                        .clicked()
                    {
                        actions.push(Action::MakerSave);
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.heading("Sample Library");
                    if platform::IS_WEB {
                        ui.label(
                            "The web version already includes the whole sample library: look in the \
                             Gemstones, Glass, Pearls, Metal and Wood bead folders.",
                        );
                    }
                    let count = beadmaker::sample_library().len();
                    ui.label(format!(
                        "Makes {count} beads: {} colours in the usual shapes and sizes, sorted into \
                         Generated\\Gemstones, Glass, Pearls, Metal and Wood. Beads already there are kept.",
                        PRESETS.len()
                    ));
                    if !platform::IS_WEB
                        && ui
                            .add_enabled(!busy, colour_button("Generate Sample Library", AUTO_GOLD))
                            .clicked()
                    {
                        actions.push(Action::MakerLibrary);
                    }
                    if busy && self.maker_total > 0 {
                        ui.add(
                            egui::ProgressBar::new(self.maker_done as f32 / self.maker_total as f32)
                                .text(format!("{} / {}", self.maker_done, self.maker_total)),
                        );
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Your own photos and generated beads work side by side. Generated beads are \
                             yours to share with the program.",
                        )
                        .small()
                        .weak(),
                    );
                });
            });

        // Preview: a large view, then the bead at its real picture size on a strand.
        self.maker_refresh(ui.ctx());
        let Some(tex) = self.maker_texture.clone() else { return };
        let (pw, ph) = self.maker.pixel_size();
        let (bw, bh) = (pw as f32, ph as f32);
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let area = response.rect;
        let bg = self.settings.background;
        paint_background(&painter, area, false, bg);
        let [tr, tg, tb] = text_colour(bg);
        let ink = Color32::from_rgb(tr, tg, tb);
        let big = Rect::from_min_size(area.min, Vec2::new(area.width(), area.height() * 0.65));
        let s = ((big.width() - 40.0) / bw)
            .min((big.height() - 50.0) / bh)
            .clamp(0.5, 6.0);
        painter.image(
            tex.id(),
            Rect::from_center_size(big.center(), Vec2::new(bw * s, bh * s)),
            full_uv(),
            Color32::WHITE,
        );
        painter.text(
            big.left_top() + Vec2::new(12.0, 10.0),
            egui::Align2::LEFT_TOP,
            format!("{} ({:.0}x magnified)", self.maker.file_name(&self.maker_name), s),
            egui::FontId::proportional(15.0),
            ink,
        );
        let y = big.bottom() + (area.bottom() - big.bottom()) * 0.55;
        painter.text(
            Pos2::new(area.left() + 12.0, big.bottom() + 6.0),
            egui::Align2::LEFT_TOP,
            "Picture size, on a strand",
            egui::FontId::proportional(13.0),
            ink,
        );
        painter.line_segment(
            [Pos2::new(area.left() + 12.0, y), Pos2::new(area.right() - 12.0, y)],
            Stroke::new(2.0_f32, Color32::from_rgb(140, 110, 80)),
        );
        let mut x = area.left() + 30.0;
        let mut n = 0;
        while x + bw < area.right() - 30.0 && n < 12 {
            painter.image(
                tex.id(),
                Rect::from_min_size(Pos2::new(x, y - bh / 2.0), Vec2::new(bw, bh)),
                full_uv(),
                Color32::WHITE,
            );
            x += bw;
            n += 1;
        }
    }

    fn shortcuts(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if ctx.wants_keyboard_input() {
            return; // typing in the Find box
        }
        use egui::{Key, KeyboardShortcut as K, Modifiers as M};
        let table: [(K, fn() -> Action); 12] = [
            (K::new(M::COMMAND, Key::P), || Action::PrintPdf),
            (K::new(M::COMMAND, Key::H), || Action::ToggleHistory),
            (K::new(M::COMMAND, Key::N), || Action::New),
            (K::new(M::COMMAND, Key::O), || Action::Open),
            (K::new(M::COMMAND, Key::S), || Action::Save),
            (K::new(M::COMMAND, Key::B), || Action::QuickBookmark),
            (K::new(M::COMMAND, Key::Z), || Action::Undo),
            (K::new(M::COMMAND, Key::Y), || Action::Redo),
            (K::new(M::COMMAND, Key::A), || Action::SelectAll),
            (K::new(M::COMMAND, Key::D), || Action::DeselectAll),
            (K::new(M::COMMAND, Key::ArrowLeft), || Action::ShiftLeft),
            (K::new(M::COMMAND, Key::ArrowRight), || Action::ShiftRight),
        ];
        for (shortcut, make) in table {
            if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                actions.push(make());
            }
        }
    }
}

impl eframe::App for BeadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut actions = Vec::new();
        self.receive_colours(ctx);
        self.receive_maker(ctx);
        // Web: the sample photos have arrived, so show them and work out their colours.
        if let Some((downloaded, failed)) = platform::take_samples_ready() {
            self.images.clear();
            self.start_colour_analysis();
            self.status = if failed == 0 {
                format!("Ready: {downloaded} sample photos loaded.")
            } else {
                format!("{downloaded} sample photos loaded; {failed} could not be downloaded.")
            };
        }
        // Files picked in the browser arrive a moment after the picker closes.
        while let Some((purpose, path)) = platform::take_picked() {
            match purpose {
                Pick::Design => self.ask_then(AfterConfirm::Open(path), ctx),
                Pick::Texture => {
                    self.maker_texture_list = None;
                    self.maker_load_photo(path);
                }
            }
        }
        self.frame_start = self.design.clone();
        self.fit_to_screen(ctx);
        self.shortcuts(ctx, &mut actions);

        // Window close button: ask to save first if the design has unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty && self.has_design() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(AfterConfirm::Quit);
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            self.menu_bar(ui, &mut actions);
            ui.horizontal_wrapped(|ui| {
                for (tab, label) in TABS {
                    if ui.selectable_value(&mut self.tab, tab, label).clicked() && tab != Tab::Main {
                        self.came_from_edit_tab = false;
                    }
                }
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        if self.tab == Tab::Main {
            egui::SidePanel::left("beads")
                .resizable(true)
                .default_width(240.0)
                .show(ctx, |ui| {
                    self.bead_list_panel(ui, &mut actions);
                });
            // On a narrow screen the material list stays on its own tab, to leave room for the design.
            let wide = ctx.screen_rect().width() >= NARROW_SCREEN;
            egui::SidePanel::right("materials")
                .resizable(true)
                .default_width(230.0)
                .show_animated(ctx, wide, |ui| {
                    ui.label(RichText::new("Material List").strong());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.materials.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
        }

        let tab = self.tab;
        let (grid, bg) = (self.settings.show_grid, self.settings.background);
        let mut contents = |ui: &mut egui::Ui| match tab {
            Tab::Main => self.main_tab(ui, &mut actions),
            Tab::Circular => {
                ui.horizontal(|ui| {
                    if ui.add(colour_button("Save Image", FILE_TAN)).clicked() {
                        actions.push(Action::SaveImage);
                    }
                    if ui
                        .add(colour_button("Print / PDF", FILE_TAN))
                        .on_hover_text("Ctrl+P")
                        .clicked()
                    {
                        actions.push(Action::PrintPdf);
                    }
                    ui.checkbox(&mut self.settings.materials_on_picture, "Material List on Picture");
                });
                let list = self
                    .settings
                    .materials_on_picture
                    .then_some(self.material_lines.as_slice());
                design_canvas_listed(ui, &mut self.images, &parts!(self), &self.circular, grid, bg, list);
            }
            Tab::Linear => self.linear_tab(ui, &mut actions),
            Tab::Materials => {
                if ui.button("Copy Material List to Clipboard").clicked() {
                    ui.ctx().copy_text(self.materials.clone());
                }
                ui.add(
                    egui::TextEdit::multiline(&mut self.materials.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            }
            Tab::Options => self.options_tab(ui, &mut actions),
            Tab::Inventory => self.inventory_tab(ui),
            Tab::Folders => self.folders_tab(ui, &mut actions),
            Tab::Bookmarks => self.bookmarks_tab(ui, &mut actions),
            Tab::Pendants => self.pendants_tab(ui, &mut actions),
            Tab::EditPatterns => self.edit_patterns_tab(ui, &mut actions),
            Tab::Arc => self.arc_tab(ui, &mut actions),
            Tab::BeadMaker => self.bead_maker_tab(ui, &mut actions),
        };
        // Very small screens (phones): scroll around the page instead of squeezing it.
        let screen = ctx.screen_rect().size();
        egui::CentralPanel::default().show(ctx, |ui| {
            if screen.x < SCROLL_BELOW.x || screen.y < SCROLL_BELOW.y {
                egui::ScrollArea::both()
                    .id_salt("small_screen")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_size(SMALL_SCREEN_PAGE);
                        contents(ui);
                    });
            } else {
                contents(ui);
            }
        });

        for a in actions {
            self.apply(a, ctx);
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("About Bunch-a-Beads")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.heading("Bunch-a-Beads Pattern Designer");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(6.0);
                    ui.label("Design bead necklaces and bracelets without getting your beads out.");
                    ui.label("Rust rewrite of Bunch-a-Beads Pattern Design Pro (2013) by David Bunch.");
                    ui.add_space(6.0);
                    ui.label("Built with Rust and egui. Runs on Windows, Linux and macOS.");
                    ui.label(format!("Settings file: {}", self.settings_path.display()));
                });
            if !open {
                self.show_about = false;
            }
        }

        // History window: every change since the program started (up to 100), click one to go back to it.
        if self.show_history {
            let mut open = true;
            let mut go: Option<Action> = None;
            egui::Window::new("History")
                .open(&mut open)
                .default_width(320.0)
                .default_pos([ctx.screen_rect().right() - 360.0, 90.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.undo.is_empty(), egui::Button::new("⟲ Undo"))
                            .clicked()
                        {
                            go = Some(Action::Undo);
                        }
                        if ui
                            .add_enabled(!self.redo.is_empty(), egui::Button::new("Redo ⟳"))
                            .clicked()
                        {
                            go = Some(Action::Redo);
                        }
                    });
                    ui.label(RichText::new("Click a step to go back to just after it.").small());
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let n = self.undo.len();
                            if ui.selectable_label(n == 0, "Start").clicked() && n > 0 {
                                go = Some(Action::UndoSteps(n));
                            }
                            for (k, entry) in self.undo.iter().enumerate() {
                                let now = k + 1 == n;
                                let text = if now {
                                    format!("{}. {}   ◀ now", k + 1, entry.label)
                                } else {
                                    format!("{}. {}", k + 1, entry.label)
                                };
                                if ui.selectable_label(now, text).clicked() && !now {
                                    go = Some(Action::UndoSteps(n - 1 - k));
                                }
                            }
                            // Steps that were undone and can be redone, in order.
                            for (j, entry) in self.redo.iter().rev().enumerate() {
                                let text = RichText::new(format!("   {}", entry.label))
                                    .color(Color32::GRAY)
                                    .italics();
                                if ui
                                    .selectable_label(false, text)
                                    .on_hover_text("Undone: click to redo up to here")
                                    .clicked()
                                {
                                    go = Some(Action::RedoSteps(j + 1));
                                }
                            }
                        });
                });
            if !open {
                self.show_history = false;
            }
            if let Some(a) = go {
                self.apply(a, ctx);
            }
        }

        // "Save changes?" question.
        if let Some(next) = self.confirm.clone() {
            let mut choice = None;
            egui::Window::new("Save changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("The current design has changes that haven't been saved.");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add(colour_button("Save", DISPLAY_GREEN)).clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Don't Save").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(2);
                        }
                    });
                });
            match choice {
                Some(0) => {
                    self.confirm = None;
                    self.save_design();
                    if !self.dirty {
                        self.do_after(next, ctx);
                    }
                }
                Some(1) => {
                    self.confirm = None;
                    self.dirty = false;
                    self.do_after(next, ctx);
                }
                Some(_) => self.confirm = None,
                None => {}
            }
        }

        // Window title: file name, and * when there are unsaved changes.
        let name = match (&self.current_file, &self.imported_name) {
            (Some(p), _) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            (None, Some(n)) => format!("{n} (2013 import)"),
            (None, None) => "Untitled".into(),
        };
        let title = format!(
            "Bunch-a-Beads Pattern Designer - {name}{}",
            if self.dirty && self.has_design() { " *" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        if self.slide_show {
            let now = ctx.input(|i| i.time);
            if now >= self.next_slide_at {
                self.auto_pattern();
                self.next_slide_at = now + SLIDE_SECONDS;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Called by eframe every 30 seconds and when the window closes.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }
}

/// A different starting point for the random generator every time the program
/// starts: the clock mixed with random keys that the operating system gives each
/// program (std's `RandomState`), so two starts can never line up.
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(platform::now_nanos());
    h.write_u32(platform::process_id());
    h.finish()
}

/// Names a change made directly by a control (for the History list).
fn describe_change(before: &Design, after: &Design) -> String {
    if before.mode != after.mode {
        let m = match after.mode {
            DesignMode::OnePatternLinear => "1 pattern linear",
            DesignMode::OnePatternMirrored => "1 pattern mirrored",
            DesignMode::TwoPattern => "2 patterns",
            DesignMode::Manual => "manual edit design",
        };
        format!("Design: {m}")
    } else if before.target_length_inches != after.target_length_inches {
        "Length".into()
    } else if before.use_clasp != after.use_clasp || before.clasp != after.clasp {
        "Clasp".into()
    } else if before.crimp != after.crimp || before.crimps_per_end != after.crimps_per_end {
        "Crimps".into()
    } else if before.pendant != after.pendant || before.use_pendant != after.use_pendant {
        "Pendant".into()
    } else {
        "Change".into()
    }
}

/// Local date and time for automatic file names, like 2013: "261116_1405".
fn timestamp() -> String {
    chrono::Local::now().format("%y%m%d_%H%M").to_string()
}

// ----- drawing helpers -------------------------------------------------------------

fn colour_button(label: &str, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.to_owned()).color(Color32::BLACK)).fill(fill)
}

fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

/// Draws a picture inside `rect`, centred. With `keep_scale`, a 12 mm bead
/// fills the box and smaller beads stay proportionally smaller.
fn paint_thumb(painter: &egui::Painter, tex: egui::TextureId, w: u32, h: u32, rect: Rect, keep_scale: bool) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let s = if keep_scale {
        rect.width().min(rect.height()) / 142f32.max(w).max(h)
    } else {
        (rect.width() / w).min(rect.height() / h) // fill the box
    };
    let r = Rect::from_center_size(rect.center(), Vec2::new(w * s, h * s));
    painter.image(tex, r, full_uv(), Color32::WHITE);
}

/// Fills the rest of `ui` with the design: each piece rotated and scaled to fit.
fn design_canvas(
    ui: &mut egui::Ui,
    images: &mut Images,
    parts: &Parts,
    placed: &[Placed],
    show_grid: bool,
    background: Background,
) {
    let size = ui.available_size();
    let _ = design_canvas_sized(ui, size, images, parts, placed, show_grid, background, None);
}

/// Like `design_canvas`, with the material list written inside the circle.
fn design_canvas_listed(
    ui: &mut egui::Ui,
    images: &mut Images,
    parts: &Parts,
    placed: &[Placed],
    show_grid: bool,
    background: Background,
    list: Option<&[String]>,
) {
    let size = ui.available_size();
    let _ = design_canvas_sized(ui, size, images, parts, placed, show_grid, background, list);
}

/// Radius of the ring of beads in a circular layout (beads sit on a circle round (0, 0)).
fn ring_radius(placed: &[Placed]) -> Option<f64> {
    placed
        .iter()
        .find(|p| matches!(p.piece, beads_core::Piece::Bead(_)))
        .map(|p| p.cx.hypot(p.cy))
}

/// Black text on light backgrounds, white on dark ones.
fn text_colour(background: Background) -> [u8; 3] {
    let [r, g, b] = background.rgb();
    if (r as u32 + g as u32 + b as u32) > 384 {
        [20, 20, 20]
    } else {
        [240, 240, 240]
    }
}

/// A picture as PNG file bytes.
fn encode_png(img: &Rgba) -> std::io::Result<Vec<u8>> {
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &img.pixels,
        img.width,
        img.height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(std::io::Error::other)?;
    Ok(png)
}

/// Draws the design into an area of exactly `size`, scaled to fit it.
fn design_canvas_sized(
    ui: &mut egui::Ui,
    size: Vec2,
    images: &mut Images,
    parts: &Parts,
    placed: &[Placed],
    show_grid: bool,
    background: Background,
    list: Option<&[String]>,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let area = resp.rect;
    paint_background(&painter, area, show_grid, background);
    match Fit::compute(placed, area.width() as f64, area.height() as f64, 10.0) {
        Some(fit) => {
            paint_pieces(&painter, area, images, parts, placed, fit, &ctx);
            if let (Some(lines), Some(radius)) = (list.filter(|l| !l.is_empty()), ring_radius(placed)) {
                // The material list inside the circle, as large as fits.
                let half = radius * 0.62;
                let (x0, y0) = fit.apply(-half, -half);
                let (x1, y1) = fit.apply(half, half);
                let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
                let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
                let size = (bh / (lines.len() as f32 * 1.3))
                    .min(bw / (widest * 0.55))
                    .clamp(6.0, 20.0);
                let top = y0 as f32 + (bh - lines.len() as f32 * size * 1.3).max(0.0) / 2.0;
                let [r, g, b] = text_colour(background);
                for (i, line) in lines.iter().enumerate() {
                    painter.text(
                        area.min + Vec2::new(x0 as f32, top + i as f32 * size * 1.3),
                        egui::Align2::LEFT_TOP,
                        line,
                        egui::FontId::proportional(size),
                        Color32::from_rgb(r, g, b),
                    );
                }
            }
        }
        None => {
            painter.text(
                area.left_top() + Vec2::splat(10.0),
                egui::Align2::LEFT_TOP,
                "Add beads to Pattern 1, then click Draw Design",
                egui::FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
    }
    resp
}

/// Fills `area` with the background colour and, optionally, the grid.
fn paint_background(painter: &egui::Painter, area: Rect, show_grid: bool, background: Background) {
    let [r, g, b] = background.rgb();
    painter.rect_filled(area, 0.0, Color32::from_rgb(r, g, b));
    if show_grid {
        // Grid lines that show up on light and dark backgrounds.
        let line = if (r as u32 + g as u32 + b as u32) > 384 {
            Color32::from_gray(205)
        } else {
            Color32::from_gray(70)
        };
        let stroke = Stroke::new(1.0_f32, line);
        let mut x = area.left();
        while x < area.right() {
            painter.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], stroke);
            x += 24.0;
        }
        let mut y = area.top();
        while y < area.bottom() {
            painter.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], stroke);
            y += 24.0;
        }
    }
}

/// Draws each piece rotated and scaled by `fit` inside `area`.
fn paint_pieces(
    painter: &egui::Painter,
    area: Rect,
    images: &mut Images,
    parts: &Parts,
    placed: &[Placed],
    fit: Fit,
    ctx: &egui::Context,
) {
    let s = fit.scale as f32;
    for p in placed {
        let Some(file) = parts.info(p.piece).and_then(|i| i.path) else {
            continue;
        };
        let Some(img) = images.get(ctx, file) else { continue };
        let (x, y) = fit.apply(p.cx, p.cy);
        let centre = area.min + Vec2::new(x as f32, y as f32);
        let piece_size = Vec2::new(p.width as f32 * s, p.height as f32 * s);
        let shift = Vec2::new(p.x_shift as f32 * s, p.y_offset as f32 * s);
        let mut mesh = egui::Mesh::with_texture(img.texture.id());
        mesh.add_rect_with_uv(
            Rect::from_center_size(centre + shift, piece_size),
            full_uv(),
            Color32::WHITE,
        );
        mesh.rotate(egui::emath::Rot2::from_angle((p.angle_deg as f32).to_radians()), centre);
        painter.add(egui::Shape::mesh(mesh));
    }
}
