//! Everything that differs between the desktop program and the web page:
//! files, file dialogs, background work and the clock.
//!
//! * Desktop: the real disk, the computer's own dialogs, and a background thread.
//! * Web: a file store in memory (`beads_core::memfs`) that is kept in the
//!   browser's local storage between visits; "Save As" becomes a download and
//!   "Open" uploads a file; background work is done a little every frame.

use std::path::{Path, PathBuf};

pub use imp::*;

/// What a picked file is for (the web's file picker answers later, not at once).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Design,
    Texture,
}

/// Lets core code (bead folder scanning) read files the same way as the app.
pub struct AppFiles;

/// A run of jobs done one after another in the background; results are
/// collected every frame with `poll`.
pub struct Jobs<R> {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<R>,
    #[cfg(target_arch = "wasm32")]
    queue: std::collections::VecDeque<Job<R>>,
}

pub type Job<R> = Box<dyn FnOnce() -> R + Send + 'static>;

impl<R: Send + 'static> Jobs<R> {
    pub fn start(jobs: Vec<Job<R>>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for job in jobs {
                    if tx.send(job()).is_err() {
                        return; // nobody is waiting any more (folder changed, or closing)
                    }
                }
            });
            Jobs { rx }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Jobs { queue: jobs.into() }
        }
    }

    /// Results finished since the last call. In the browser this also does the
    /// next few milliseconds of work, so the page stays responsive.
    pub fn poll(&mut self) -> Vec<R> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.rx.try_iter().collect()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let start = now_millis();
            let mut out = Vec::new();
            while let Some(job) = self.queue.pop_front() {
                out.push(job());
                if now_millis().saturating_sub(start) >= 12 {
                    break;
                }
            }
            out
        }
    }
}

/// A file dialog filter: a name and extensions without dots.
pub struct Filter<'a>(pub &'a str, pub &'a [&'a str]);

// ===================================================================== desktop
#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use beads_core::catalog::disk::{DiskFiles, Files};
    use std::io;

    pub const IS_WEB: bool = false;

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    pub fn read_to_string(path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    pub fn write(path: &Path, data: impl AsRef<[u8]>) -> io::Result<()> {
        std::fs::write(path, data)
    }
    pub fn create_dir_all(path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    pub fn is_dir(path: &Path) -> bool {
        path.is_dir()
    }
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }
    /// Everything directly inside `dir`: (path, is it a folder).
    pub fn list(dir: &Path) -> Vec<(PathBuf, bool)> {
        DiskFiles.list(dir)
    }
    /// When a file was last changed, in milliseconds since 1970.
    pub fn modified_millis(path: &Path) -> u64 {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64)
    }
    pub fn now_nanos() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
    }
    #[allow(dead_code)]
    pub fn now_millis() -> u64 {
        now_nanos() / 1_000_000
    }
    pub fn process_id() -> u32 {
        std::process::id()
    }

    impl Files for AppFiles {
        fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
            DiskFiles.list(dir)
        }
        fn head(&self, path: &Path, n: usize) -> Option<Vec<u8>> {
            DiskFiles.head(path, n)
        }
        fn read_text(&self, path: &Path) -> Option<String> {
            DiskFiles.read_text(path)
        }
    }

    pub fn pick_folder(start: &str) -> Option<PathBuf> {
        let mut dlg = rfd::FileDialog::new().set_title("Choose folder");
        if !start.is_empty() {
            dlg = dlg.set_directory(start);
        }
        dlg.pick_folder()
    }

    /// Asks for a file to open. On the desktop the answer comes back at once.
    pub fn pick_file(
        _ctx: &eframe::egui::Context,
        _purpose: Pick,
        filter: Filter,
        start: Option<&Path>,
    ) -> Option<PathBuf> {
        let mut dlg = rfd::FileDialog::new().add_filter(filter.0, filter.1);
        if let Some(dir) = start {
            dlg = dlg.set_directory(dir);
        }
        dlg.pick_file()
    }

    /// A file picked earlier whose answer came later (web only).
    pub fn take_picked() -> Option<(Pick, PathBuf)> {
        None
    }

    /// Web only: starts downloading the sample bead photos.
    pub fn start_sample_downloads(_ctx: &eframe::egui::Context) {}

    /// Web only: Some((downloaded, failed)) once, when the sample photos have all arrived.
    pub fn take_samples_ready() -> Option<(usize, usize)> {
        None
    }

    /// Asks where to save. (On the web this is the Downloads folder.)
    pub fn save_file(name: &str, filter: Filter, start: Option<&Path>) -> Option<PathBuf> {
        let mut dlg = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter(filter.0, filter.1);
        if let Some(dir) = start {
            dlg = dlg.set_directory(dir);
        }
        dlg.save_file()
    }

    /// Opens a file in the program the computer uses for it (e.g. the PDF viewer).
    pub fn open_with_system(path: &Path) -> io::Result<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(path).spawn()?;
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            std::process::Command::new("xdg-open").arg(path).spawn()?;
        }
        Ok(())
    }
}

// ========================================================================= web
#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use beads_core::memfs::{self, MemFs};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io;

    pub const IS_WEB: bool = true;

    /// The web version's "main folder".
    pub const ROOT: &str = "/BunchABeads";
    /// Writing here downloads the file to the visitor's computer.
    const DOWNLOADS: &str = "/Downloads";
    /// Files opened from the visitor's computer that aren't kept.
    const UPLOADS: &str = "/Uploads";
    /// Prefix of our entries in the browser's local storage.
    const STORE: &str = "bab:";
    /// Bigger files are used but not kept between visits (local storage is small).
    const KEEP_LIMIT: usize = 1_500_000;

    // SAMPLE_PNGS and SAMPLE_TEXTS: the files in beads-app/web-samples (made by build.rs).
    include!(concat!(env!("OUT_DIR"), "/web_samples.rs"));

    /// Where Trunk puts the sample files, next to the web page.
    const SAMPLES_URL: &str = "web-samples";

    #[derive(Default)]
    struct State {
        fs: MemFs,
        ready: bool,
        picked: VecDeque<(Pick, PathBuf)>,
        downloads_started: bool,
        downloaded: usize,
        failed: usize,
        samples_ready: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.ready {
                s.ready = true;
                setup(&mut s.fs);
            }
            f(&mut s)
        })
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    /// The built-in folders, the sample beads, and whatever was kept from last time.
    fn setup(fs: &mut MemFs) {
        for sub in [
            "Beads",
            "Clasps",
            "Crimps",
            "Pendants",
            "Bookmarks",
            "Images",
            "AutomaticDesigns",
            "Textures",
        ] {
            fs.create_dir_all(&Path::new(ROOT).join(sub));
        }
        // The Bead Maker's sample library, drawn only when a bead is needed.
        for bead in beads_core::beadmaker::sample_library() {
            let path = Path::new(ROOT)
                .join("Beads")
                .join("Generated")
                .join(bead.folder)
                .join(format!("{}.png", bead.name));
            fs.add_bead(&path, bead.spec);
        }
        // The sample photos: listed now (with their sizes) so the bead lists are
        // complete at once; the pictures themselves arrive a moment later.
        for (rel, w, h) in SAMPLE_PNGS {
            fs.add_coming(&Path::new(ROOT).join(rel), memfs::png_header(*w, *h));
        }
        for (rel, text) in SAMPLE_TEXTS {
            fs.write(&Path::new(ROOT).join(rel), text.as_bytes().to_vec(), 0);
        }
        // Crimp beads (small metal tubes) if the samples have none.
        use beads_core::beadmaker::{BeadSpec, Material, Shape};
        let has_crimps = SAMPLE_PNGS.iter().any(|(rel, _, _)| rel.starts_with("Crimps/"));
        const CRIMPS: [(&str, [u8; 3]); 4] = [
            ("Silver", [205, 208, 212]),
            ("Gold", [230, 180, 70]),
            ("Copper", [200, 110, 70]),
            ("Gunmetal", [95, 95, 100]),
        ];
        let crimps: &[(&str, [u8; 3])] = if has_crimps { &[] } else { &CRIMPS };
        for &(name, colour) in crimps {
            let spec = BeadSpec {
                shape: Shape::Tube,
                material: Material::Metal,
                width_mm: 2.0,
                height_mm: 2.0,
                colour,
                ..BeadSpec::default()
            };
            fs.add_bead(
                &Path::new(ROOT).join("Crimps").join(format!("{name}_2mmCrimp.png")),
                spec,
            );
        }
        // Files saved on earlier visits.
        if let Some(store) = storage() {
            let n = store.length().unwrap_or(0);
            for i in 0..n {
                let Ok(Some(k)) = store.key(i) else { continue };
                let Some(path) = k.strip_prefix(STORE) else { continue };
                let Ok(Some(v)) = store.get_item(&k) else { continue };
                // "modified:base64"
                let (modified, data) = v.split_once(':').unwrap_or(("0", v.as_str()));
                if let Some(bytes) = memfs::base64_decode(data) {
                    fs.write(Path::new(path), bytes, modified.parse().unwrap_or(0));
                }
            }
        }
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, format!("{} not found", path.display()))
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        with(|s| {
            if let Some(b) = s.fs.read(path) {
                return Ok(b.to_vec());
            }
            // A sample bead: draw it now and remember the picture.
            let spec = s.fs.pending_bead(path).ok_or_else(|| not_found(path))?;
            let img = beads_core::beadmaker::render(&spec);
            let mut png = Vec::new();
            image::ImageEncoder::write_image(
                image::codecs::png::PngEncoder::new(&mut png),
                &img.pixels,
                img.width,
                img.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(io::Error::other)?;
            s.fs.write(path, png.clone(), 0);
            Ok(png)
        })
    }

    pub fn read_to_string(path: &Path) -> io::Result<String> {
        read(path).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub fn write(path: &Path, data: impl AsRef<[u8]>) -> io::Result<()> {
        let data = data.as_ref();
        let key = memfs::key(path);
        if key.starts_with(&format!("{DOWNLOADS}/")) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return download(&name, data);
        }
        let now = now_millis();
        with(|s| s.fs.write(path, data.to_vec(), now));
        if key.starts_with(&format!("{UPLOADS}/")) || data.len() > KEEP_LIMIT {
            return Ok(());
        }
        let store = storage().ok_or_else(|| io::Error::other("the browser has no local storage"))?;
        store
            .set_item(
                &format!("{STORE}{key}"),
                &format!("{now}:{}", memfs::base64_encode(data)),
            )
            .map_err(|_| io::Error::other("the browser's storage is full (it's used for this visit only)"))
    }

    pub fn create_dir_all(path: &Path) -> io::Result<()> {
        with(|s| s.fs.create_dir_all(path));
        Ok(())
    }
    pub fn is_dir(path: &Path) -> bool {
        with(|s| s.fs.is_dir(path))
    }
    pub fn exists(path: &Path) -> bool {
        with(|s| s.fs.exists(path))
    }
    pub fn list(dir: &Path) -> Vec<(PathBuf, bool)> {
        with(|s| s.fs.list(dir))
    }
    pub fn modified_millis(path: &Path) -> u64 {
        with(|s| s.fs.modified(path))
    }
    pub fn now_millis() -> u64 {
        js_sys::Date::now() as u64
    }
    pub fn now_nanos() -> u64 {
        // Date.now() is whole milliseconds; add the high-resolution page clock.
        let fine = web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now());
        now_millis().wrapping_mul(1_000_000) ^ (fine * 1_000_000.0) as u64
    }
    pub fn process_id() -> u32 {
        (js_sys::Math::random() * u32::MAX as f64) as u32
    }

    impl beads_core::catalog::disk::Files for AppFiles {
        fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
            list(dir)
        }
        fn head(&self, path: &Path, n: usize) -> Option<Vec<u8>> {
            with(|s| beads_core::catalog::disk::Files::head(&s.fs, path, n))
        }
        fn read_text(&self, path: &Path) -> Option<String> {
            with(|s| beads_core::catalog::disk::Files::read_text(&s.fs, path))
        }
    }

    /// Default folders for the web version (all inside the browser).
    pub fn web_folders(s: &mut beads_core::Settings) {
        let fill = |field: &mut String, name: &str| {
            if field.is_empty() {
                *field = format!("{ROOT}/{name}");
            }
        };
        s.main_folder = ROOT.to_string();
        fill(&mut s.bead_folder, "Beads");
        fill(&mut s.clasp_folder, "Clasps");
        fill(&mut s.crimp_folder, "Crimps");
        fill(&mut s.pendant_folder, "Pendants");
        fill(&mut s.bookmarks_folder, "Bookmarks");
        fill(&mut s.images_folder, "Images");
        fill(&mut s.automatic_designs_folder, "AutomaticDesigns");
        fill(&mut s.textures_folder, "Textures");
    }

    /// Folders are built in on the web.
    pub fn pick_folder(_start: &str) -> Option<PathBuf> {
        None
    }

    /// Opens the browser's file picker. The answer arrives later through `take_picked`.
    pub fn pick_file(
        ctx: &eframe::egui::Context,
        purpose: Pick,
        filter: Filter,
        _start: Option<&Path>,
    ) -> Option<PathBuf> {
        let dlg = rfd::AsyncFileDialog::new().add_filter(filter.0, filter.1);
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(file) = dlg.pick_file().await else { return };
            let name = file.file_name();
            let data = file.read().await;
            // Textures are kept (they appear in the Bead Maker's list); designs are just opened.
            let dir = match purpose {
                Pick::Texture => format!("{ROOT}/Textures"),
                Pick::Design => UPLOADS.to_string(),
            };
            let path = Path::new(&dir).join(&name);
            let _ = write(&path, &data);
            with(|s| s.picked.push_back((purpose, path)));
            ctx.request_repaint();
        });
        None
    }

    pub fn take_picked() -> Option<(Pick, PathBuf)> {
        with(|s| s.picked.pop_front())
    }

    /// Downloads every sample photo from the web site, several at a time.
    pub fn start_sample_downloads(ctx: &eframe::egui::Context) {
        if with(|s| std::mem::replace(&mut s.downloads_started, true)) || SAMPLE_PNGS.is_empty() {
            return;
        }
        for (rel, _, _) in SAMPLE_PNGS {
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let url = String::from(js_sys::encode_uri(&format!("{SAMPLES_URL}/{rel}")));
                let got = fetch_bytes(&url).await;
                let path = Path::new(ROOT).join(rel);
                let done = with(|s| {
                    match got {
                        Ok(bytes) => {
                            s.fs.write(&path, bytes, 0);
                            s.downloaded += 1;
                        }
                        Err(_) => s.failed += 1,
                    }
                    s.downloaded + s.failed == SAMPLE_PNGS.len()
                });
                if done {
                    with(|s| s.samples_ready = true);
                    ctx.request_repaint();
                }
            });
        }
    }

    pub fn take_samples_ready() -> Option<(usize, usize)> {
        with(|s| std::mem::take(&mut s.samples_ready).then_some((s.downloaded, s.failed)))
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        let window = web_sys::window().ok_or("no window")?;
        let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url)).await?.dyn_into()?;
        if !response.ok() {
            return Err(format!("{url}: HTTP {}", response.status()).into());
        }
        let buffer = JsFuture::from(response.array_buffer()?).await?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    /// "Save As" on the web: the file goes to the visitor's Downloads folder.
    pub fn save_file(name: &str, _filter: Filter, _start: Option<&Path>) -> Option<PathBuf> {
        Some(Path::new(DOWNLOADS).join(name))
    }

    /// The browser opens or saves a download itself.
    pub fn open_with_system(_path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn download(name: &str, data: &[u8]) -> io::Result<()> {
        use wasm_bindgen::JsCast;
        let fail = |what: &str| io::Error::other(format!("could not download {name} ({what})"));
        let bytes = js_sys::Uint8Array::from(data);
        let parts = js_sys::Array::new();
        parts.push(&bytes);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(|_| fail("blob"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| fail("url"))?;
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| fail("page"))?;
        let link = document
            .create_element("a")
            .map_err(|_| fail("link"))?
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .map_err(|_| fail("link"))?;
        link.set_href(&url);
        link.set_download(name);
        link.click();
        Ok(())
    }
}
