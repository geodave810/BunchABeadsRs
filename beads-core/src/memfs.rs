//! A small file store kept in memory, used by the web version in place of the
//! computer's disk. Paths look like `/BunchABeads/Beads/Glass/Ruby_6mmRD.png`.
//!
//! Besides ordinary files it holds "made on demand" beads: a Bead Maker recipe
//! whose picture is only drawn the first time it's needed, so the built-in
//! sample library costs nothing until a bead is shown.

use crate::beadmaker::BeadSpec;
use crate::catalog::disk::Files;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
enum Content {
    Bytes(Vec<u8>),
    /// A bead drawn when first read.
    Bead(BeadSpec),
    /// A file still downloading: only its first bytes (the PNG size) are known.
    Coming(Vec<u8>),
}

#[derive(Clone, Debug)]
struct Entry {
    content: Content,
    /// Milliseconds since 1970, for "newest first" lists.
    modified: u64,
}

#[derive(Default, Debug)]
pub struct MemFs {
    files: BTreeMap<String, Entry>,
    dirs: BTreeSet<String>,
}

/// "C:\a\\b/" or "a/b" -> "/a/b".
pub fn key(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = raw.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    format!("/{}", parts.join("/"))
}

/// The first 24 bytes of a PNG file of this size (all a size check reads).
pub fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut h = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    h.extend_from_slice(b"IHDR");
    h.extend_from_slice(&width.to_be_bytes());
    h.extend_from_slice(&height.to_be_bytes());
    h
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_dir_all(&mut self, path: &Path) {
        let k = key(path);
        let mut cur = String::new();
        for part in k.split('/').filter(|p| !p.is_empty()) {
            cur.push('/');
            cur.push_str(part);
            self.dirs.insert(cur.clone());
        }
    }

    fn add_parents(&mut self, k: &str) {
        if let Some(cut) = k.rfind('/') {
            if cut > 0 {
                self.create_dir_all(Path::new(&k[..cut]));
            }
        }
    }

    pub fn write(&mut self, path: &Path, data: Vec<u8>, modified: u64) {
        let k = key(path);
        self.add_parents(&k);
        self.files.insert(
            k,
            Entry {
                content: Content::Bytes(data),
                modified,
            },
        );
    }

    /// Adds a bead that is drawn the first time it is read.
    pub fn add_bead(&mut self, path: &Path, spec: BeadSpec) {
        let k = key(path);
        self.add_parents(&k);
        self.files.insert(
            k,
            Entry {
                content: Content::Bead(spec),
                modified: 0,
            },
        );
    }

    /// A file that is still downloading; `head` is its start (e.g. `png_header`),
    /// enough for the bead lists. `write` replaces it when it arrives.
    pub fn add_coming(&mut self, path: &Path, head: Vec<u8>) {
        let k = key(path);
        self.add_parents(&k);
        self.files.insert(
            k,
            Entry {
                content: Content::Coming(head),
                modified: 0,
            },
        );
    }

    /// The recipe of a bead not drawn yet (the caller draws it and `write`s the PNG).
    pub fn pending_bead(&self, path: &Path) -> Option<BeadSpec> {
        match self.files.get(&key(path))?.content {
            Content::Bead(spec) => Some(spec),
            _ => None,
        }
    }

    /// File contents (None for a missing file or a bead not drawn yet).
    pub fn read(&self, path: &Path) -> Option<&[u8]> {
        match &self.files.get(&key(path))?.content {
            Content::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        self.files.remove(&key(path)).is_some()
    }

    pub fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(&key(path))
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        let k = key(path);
        if k == "/" || self.dirs.contains(&k) {
            return true;
        }
        let prefix = format!("{k}/");
        self.files
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(f, _)| f.starts_with(&prefix))
    }

    pub fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    pub fn modified(&self, path: &Path) -> u64 {
        self.files.get(&key(path)).map_or(0, |e| e.modified)
    }

    /// Everything directly inside `dir`: (path, is it a folder), sorted by name.
    pub fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
        let k = key(dir);
        let prefix = if k == "/" { "/".to_string() } else { format!("{k}/") };
        let mut out: BTreeMap<String, bool> = BTreeMap::new();
        let names = self
            .files
            .keys()
            .map(|f| (f, false))
            .chain(self.dirs.iter().map(|d| (d, true)));
        for (full, is_dir) in names {
            let Some(rest) = full.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            match rest.split_once('/') {
                Some((child, _)) => {
                    out.insert(child.to_string(), true);
                }
                None => {
                    let e = out.entry(rest.to_string()).or_insert(is_dir);
                    *e |= is_dir;
                }
            }
        }
        out.into_iter()
            .map(|(name, d)| (PathBuf::from(format!("{prefix}{name}")), d))
            .collect()
    }

    /// Every ordinary file (not made-on-demand beads) whose path starts with `prefix`.
    pub fn files_under(&self, prefix: &str) -> Vec<(String, &[u8], u64)> {
        self.files
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .filter_map(|(k, e)| match &e.content {
                Content::Bytes(b) => Some((k.clone(), b.as_slice(), e.modified)),
                _ => None,
            })
            .collect()
    }
}

impl Files for MemFs {
    fn list(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
        MemFs::list(self, dir)
    }
    fn head(&self, path: &Path, n: usize) -> Option<Vec<u8>> {
        match &self.files.get(&key(path))?.content {
            Content::Bytes(b) | Content::Coming(b) => (b.len() >= n).then(|| b[..n].to_vec()),
            Content::Bead(spec) => {
                let (w, h) = spec.pixel_size();
                let mut head = png_header(w, h);
                head.truncate(n);
                (head.len() == n).then_some(head)
            }
        }
    }
    fn read_text(&self, path: &Path) -> Option<String> {
        self.read(path).map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

/// Text-safe form of binary files, for the browser's storage (which only holds text).
pub fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for q in bytes.chunks(4) {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        let mut n = 0u32;
        for &c in &q[..4 - pad] {
            n = n << 6 | val(c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::png_size;
    use crate::BeadCatalog;

    #[test]
    fn keys_are_tidy() {
        assert_eq!(key(Path::new("BunchABeads\\Beads//Glass/")), "/BunchABeads/Beads/Glass");
        assert_eq!(key(Path::new("/")), "/");
    }

    #[test]
    fn folders_list_and_exist() {
        let mut fs = MemFs::new();
        fs.write(Path::new("/B/Beads/Glass/a.png"), vec![1], 5);
        fs.write(Path::new("/B/Beads/top.txt"), b"hi".to_vec(), 6);
        fs.create_dir_all(Path::new("/B/Bookmarks"));
        assert!(fs.is_dir(Path::new("/B")) && fs.is_dir(Path::new("/B/Beads/Glass")));
        assert!(fs.is_dir(Path::new("/B/Bookmarks")) && !fs.is_dir(Path::new("/B/Beads/top.txt")));
        assert!(!fs.is_dir(Path::new("/B/Bead")));
        let l = fs.list(Path::new("/B/Beads"));
        assert_eq!(
            l,
            vec![
                (PathBuf::from("/B/Beads/Glass"), true),
                (PathBuf::from("/B/Beads/top.txt"), false)
            ]
        );
        assert_eq!(fs.list(Path::new("/B")).len(), 2);
        assert_eq!(fs.read_text(Path::new("/B/Beads/top.txt")).as_deref(), Some("hi"));
        assert_eq!(fs.modified(Path::new("/B/Beads/top.txt")), 6);
        assert!(fs.remove(Path::new("/B/Beads/top.txt")));
        assert!(!fs.exists(Path::new("/B/Beads/top.txt")));
    }

    #[test]
    fn made_on_demand_beads_load_into_the_catalog() {
        let mut fs = MemFs::new();
        let spec = BeadSpec::default();
        fs.add_bead(Path::new("/B/Beads/Gemstones/RedJasper_8mmRD.png"), spec);
        assert_eq!(png_size(&png_header(47, 71)), Some((47, 71)));
        let mut cat = BeadCatalog::default();
        assert_eq!(cat.add_folder(&fs, Path::new("/B/Beads")), 1);
        let b = &cat.all()[0];
        assert_eq!((b.name.as_str(), b.folder.as_str()), ("RedJasper_8mmRD", "Gemstones"));
        assert_eq!((b.width_px, b.height_px), spec.pixel_size());
        let p = Path::new("/B/Beads/Gemstones/RedJasper_8mmRD.png");
        assert!(fs.read(p).is_none() && fs.pending_bead(p).is_some());
        fs.write(p, vec![9, 9], 1);
        assert!(fs.pending_bead(p).is_none() && fs.read(p) == Some(&[9u8, 9][..]));
    }

    #[test]
    fn downloading_files_are_listed_before_they_arrive() {
        let mut fs = MemFs::new();
        let p = Path::new("/B/Beads/Natural/Howlite6mmRD.png");
        fs.add_coming(p, png_header(71, 71));
        let mut cat = BeadCatalog::default();
        assert_eq!(cat.add_folder(&fs, Path::new("/B/Beads")), 1);
        assert_eq!(cat.all()[0].width_px, 71);
        assert!(fs.read(p).is_none() && fs.pending_bead(p).is_none());
        fs.write(p, vec![1, 2, 3], 0);
        assert_eq!(fs.read(p), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn base64_round_trip() {
        for n in 0..20 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&data)).unwrap(), data);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert!(base64_decode("abc").is_none());
    }
}
