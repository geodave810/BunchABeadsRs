// 1. Puts the program icon into BunchABeads.exe on Windows (the icon shown in
//    Explorer and on the taskbar). Other systems use the window icon set in main.rs.
// 2. Lists the web version's sample files (web-samples/) for the program: each
//    picture's size, and the small BeadGuide .txt files themselves. The pictures
//    are copied next to the web page by Trunk and downloaded when the page opens.
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            if let Err(e) = res.compile() {
                // Not fatal: the program still builds, just without the .exe icon.
                println!("cargo:warning=Could not add the program icon: {e}");
            }
        }
    }
    list_web_samples();
}

fn list_web_samples() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("web-samples");
    println!("cargo:rerun-if-changed=web-samples");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut pngs = String::new();
    let mut texts = String::new();
    for path in files {
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        println!("cargo:rerun-if-changed={}", path.display());
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "png" {
            let bytes = std::fs::read(&path).unwrap_or_default();
            let size = (bytes.len() >= 24 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
                .then(|| {
                    let n = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
                    (n(16), n(20))
                });
            match size {
                Some((w, h)) => pngs.push_str(&format!("    ({rel:?}, {w}, {h}),\n")),
                None => println!("cargo:warning=web-samples/{rel} is not a PNG picture; left out"),
            }
        } else if ext == "txt" {
            texts.push_str(&format!("    ({rel:?}, include_str!({:?})),\n", path.to_string_lossy()));
        }
    }
    let code = format!(
        "/// Sample pictures for the web version: (path inside web-samples, width, height).\n\
         pub const SAMPLE_PNGS: &[(&str, u32, u32)] = &[\n{pngs}];\n\
         /// Sample BeadGuide files, built into the program.\n\
         pub const SAMPLE_TEXTS: &[(&str, &str)] = &[\n{texts}];\n"
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("web_samples.rs");
    std::fs::write(out, code).unwrap();
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}
