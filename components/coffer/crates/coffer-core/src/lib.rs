//! The Shelf: the library aboard. A shelf is a directory of ZIM archives.
//! Discovery is filename-based; reading goes through zim-ffi only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Book {
    pub id: usize,
    pub file_name: String,
    pub path: PathBuf,
    pub title: String,
    pub language: String,
    pub entry_count: u32,
    pub article_count: u32,
    pub main_path: String,
    archive: Mutex<zim_ffi::Archive>,
}

impl Book {
    pub fn get(&self, path: &str) -> Result<Option<(String, Vec<u8>)>, String> {
        self.archive.lock().unwrap().get(path)
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<(String, String)>, String> {
        self.archive.lock().unwrap().search(query, limit)
    }

    pub fn suggest(&self, query: &str, limit: u32) -> Result<Vec<(String, String)>, String> {
        self.archive.lock().unwrap().suggest(query, limit)
    }
}

pub struct Shelf {
    pub books: Vec<Book>,
}

impl Shelf {
    /// Open every *.zim in `dir`, in filename order. Unreadable files are
    /// reported to stderr and skipped; a bad book never sinks the shelf.
    pub fn open_dir(dir: &Path) -> Shelf {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("zim")) == Some(true) {
                    paths.push(p);
                }
            }
        }
        paths.sort();

        let mut books = Vec::new();
        for path in paths {
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match zim_ffi::Archive::open(&path.to_string_lossy()) {
                Ok(archive) => {
                    let title = archive
                        .metadata("Title")
                        .unwrap_or_else(|| file_name.clone());
                    let language = archive.metadata("Language").unwrap_or_default();
                    let main_path = archive.main_path().unwrap_or_default();
                    let book = Book {
                        id: books.len(),
                        file_name,
                        path,
                        title,
                        language,
                        entry_count: archive.entry_count(),
                        article_count: archive.article_count(),
                        main_path,
                        archive: Mutex::new(archive),
                    };
                    books.push(book);
                }
                Err(e) => eprintln!("coffer: skipping {}: {}", path.display(), e),
            }
        }
        Shelf { books }
    }

    pub fn book(&self, id: usize) -> Option<&Book> {
        self.books.get(id)
    }
}
