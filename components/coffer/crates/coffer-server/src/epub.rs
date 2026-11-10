//! EPUB books aboard the Shelf. An EPUB is a zip of XHTML; Coffer opens
//! it directly: the spine becomes a contents page, and every file in
//! the zip is served at a path that mirrors its place inside the book,
//! so the book's own chapter links, images, and styles resolve as the
//! publisher laid them out.

use std::io::Read;
use std::path::{Path, PathBuf};

pub struct EpubBook {
    pub file_name: String,
    pub title: String,
    pub path: PathBuf,
    /// (zip path, label) in spine order.
    pub chapters: Vec<(String, String)>,
}

fn attr(tag_text: &str, name: &str) -> String {
    let pat = format!("{}=\"", name);
    if let Some(start) = tag_text.find(&pat) {
        let rest = &tag_text[start + pat.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    String::new()
}

fn tag_text(xml: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xml.find(&open) {
        let rest = &xml[start + open.len()..];
        if let Some(end) = rest.find(&close) {
            return rest[..end].trim().to_string();
        }
    }
    String::new()
}

/// Join an OPF-relative href onto the OPF's directory, resolving the
/// dot segments EPUB paths sometimes carry.
fn join_path(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let mut parts: Vec<&str> = Vec::new();
    if !base_dir.is_empty() {
        parts.extend(base_dir.split('/').filter(|s| !s.is_empty()));
    }
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

pub fn mime_for(name: &str) -> String {
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "xhtml" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "js" => "text/javascript",
        "xml" | "opf" | "ncx" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

impl EpubBook {
    pub fn open(path: &Path) -> Option<EpubBook> {
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file = std::fs::File::open(path).ok()?;
        let mut zip = zip::ZipArchive::new(file).ok()?;

        let container = String::from_utf8(read_entry(&mut zip, "META-INF/container.xml")?).ok()?;
        // "<rootfile" also matches the "<rootfiles>" wrapper; the real
        // element is followed by a space and its attributes.
        let rootfile = container
            .find("<rootfile ")
            .or_else(|| container.find("<rootfile\t"))?;
        let tag_end = container[rootfile..]
            .find('>')
            .map(|e| rootfile + e)
            .unwrap_or(container.len());
        let opf_path = attr(&container[rootfile..tag_end], "full-path");
        if opf_path.is_empty() {
            return None;
        }
        let opf_dir = opf_path
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        let opf = String::from_utf8(read_entry(&mut zip, &opf_path)?).ok()?;

        let mut title = tag_text(&opf, "dc:title");
        if title.is_empty() {
            title = file_name.clone();
        }

        // Manifest: id -> (href, media-type, properties).
        let mut manifest: Vec<(String, String, String, String)> = Vec::new();
        for block in opf.split("<item ").skip(1) {
            let end = block.find("/>").map(|e| e + 2).unwrap_or(block.len());
            let tag = &block[..end];
            let id = attr(tag, "id");
            let href = attr(tag, "href");
            let mt = attr(tag, "media-type");
            let props = attr(tag, "properties");
            if !id.is_empty() && !href.is_empty() {
                manifest.push((id, href, mt, props));
            }
        }
        let by_id = |id: &str| {
            manifest
                .iter()
                .find(|(i, _, _, _)| i == id)
                .map(|(_, h, m, _)| (h.clone(), m.clone()))
        };

        // Chapter labels from the navigation document, when there is one.
        let mut labels: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Some((_, href, _, _)) = manifest.iter().find(|(_, _, _, p)| p.contains("nav")) {
            let nav_path = join_path(&opf_dir, href);
            if let Some(bytes) = read_entry(&mut zip, &nav_path) {
                let nav = String::from_utf8_lossy(&bytes);
                for block in nav.split("<a ").skip(1) {
                    let tag_end = block.find('>').unwrap_or(0);
                    let href = attr(&block[..tag_end], "href");
                    let text_end = block.find("</a>").unwrap_or(block.len());
                    let text = block[tag_end + 1..text_end]
                        .replace(|c: char| c == '\n' || c == '\r', " ")
                        .trim()
                        .to_string();
                    if !href.is_empty() && !text.is_empty() && !text.contains('<') {
                        labels.insert(join_path(&opf_dir, &href), text);
                    }
                }
            }
        }

        // The spine, in reading order, HTML entries only.
        let mut chapters: Vec<(String, String)> = Vec::new();
        for block in opf.split("<itemref ").skip(1) {
            let end = block.find("/>").map(|e| e + 2).unwrap_or(block.len());
            let idref = attr(&block[..end], "idref");
            if let Some((href, mt)) = by_id(&idref) {
                if mt.contains("html") {
                    let full = join_path(&opf_dir, &href);
                    let label = labels.get(&full).cloned().unwrap_or_else(|| {
                        full.rsplit('/')
                            .next()
                            .unwrap_or(&full)
                            .trim_end_matches(".xhtml")
                            .trim_end_matches(".html")
                            .to_string()
                    });
                    chapters.push((full, label));
                }
            }
        }
        if chapters.is_empty() {
            return None;
        }
        Some(EpubBook {
            file_name,
            title,
            path: path.to_path_buf(),
            chapters,
        })
    }

    /// Any file inside the book, served at its own zip path.
    pub fn entry(&self, name: &str) -> Option<(String, Vec<u8>)> {
        let file = std::fs::File::open(&self.path).ok()?;
        let mut zip = zip::ZipArchive::new(file).ok()?;
        let bytes = read_entry(&mut zip, name)?;
        Some((mime_for(name), bytes))
    }

    /// A plain substring search over chapter text: honest, and fast
    /// enough for books. Returns (chapter zip path, label).
    pub fn search(&self, query: &str) -> Vec<(String, String)> {
        let q = query.to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (path, label) in &self.chapters {
            if label.to_lowercase().contains(&q) {
                out.push((path.clone(), label.clone()));
                continue;
            }
            if let Some((_, bytes)) = self.entry(path) {
                if bytes.len() > 3_000_000 {
                    continue;
                }
                let raw = String::from_utf8_lossy(&bytes);
                let mut text = String::with_capacity(raw.len());
                let mut in_tag = false;
                for c in raw.chars() {
                    match c {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        c if !in_tag => text.push(c),
                        _ => {}
                    }
                }
                if text.to_lowercase().contains(&q) {
                    out.push((path.clone(), label.clone()));
                }
            }
            if out.len() >= 10 {
                break;
            }
        }
        out
    }
}

/// Open every *.epub in `dir`, in filename order. Unreadable files are
/// skipped; a bad book never sinks the shelf.
pub fn scan_epubs(dir: &Path) -> Vec<EpubBook> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x.eq_ignore_ascii_case("epub")) == Some(true) {
                paths.push(p);
            }
        }
    }
    paths.sort();
    let mut books = Vec::new();
    for path in paths {
        match EpubBook::open(&path) {
            Some(b) => books.push(b),
            None => eprintln!("coffer: skipping {}: not a readable EPUB", path.display()),
        }
    }
    books
}
