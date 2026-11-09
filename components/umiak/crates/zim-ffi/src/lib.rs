//! Safe Rust wrapper over the libzim C shim. This crate is the only door
//! between Rust and C++ in Umiak; everything above it speaks plain Rust.

use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

#[allow(non_camel_case_types)]
type zim_handle = c_void;

extern "C" {
    fn umiak_zim_open(path: *const c_char, err_out: *mut *mut c_char) -> *mut zim_handle;
    fn umiak_zim_close(h: *mut zim_handle);
    fn umiak_zim_entry_count(h: *mut zim_handle) -> u32;
    fn umiak_zim_article_count(h: *mut zim_handle) -> u32;
    fn umiak_zim_main_path(h: *mut zim_handle, err_out: *mut *mut c_char) -> *mut c_char;
    fn umiak_zim_metadata(h: *mut zim_handle, key: *const c_char) -> *mut c_char;
    fn umiak_zim_get(
        h: *mut zim_handle,
        path: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
        mime_buf: *mut c_char,
        mime_cap: usize,
        err_out: *mut *mut c_char,
    ) -> i32;
    fn umiak_zim_search(
        h: *mut zim_handle,
        query: *const c_char,
        limit: u32,
        err_out: *mut *mut c_char,
    ) -> *mut c_char;
    fn umiak_zim_suggest(
        h: *mut zim_handle,
        query: *const c_char,
        limit: u32,
        err_out: *mut *mut c_char,
    ) -> *mut c_char;
    fn umiak_zim_free_buf(p: *mut u8);
    fn umiak_zim_free_str(p: *mut c_char);
}

fn take_err(err: *mut c_char) -> String {
    if err.is_null() {
        return "unknown libzim error".to_string();
    }
    unsafe {
        let msg = CStr::from_ptr(err).to_string_lossy().into_owned();
        umiak_zim_free_str(err);
        msg
    }
}

fn take_string(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    unsafe {
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        umiak_zim_free_str(p);
        Some(s)
    }
}

fn parse_hits(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(path, title)| (path.to_string(), title.to_string()))
        .collect()
}

/// One open ZIM archive. libzim archives are not safe for unsynchronized
/// concurrent use, so callers serialize access (the server holds each
/// archive behind a mutex).
pub struct Archive {
    raw: *mut zim_handle,
}

// The raw handle is only ever touched under the caller's lock.
unsafe impl Send for Archive {}

impl Archive {
    pub fn open(path: &str) -> Result<Archive, String> {
        let cpath = CString::new(path).map_err(|_| "path contains NUL".to_string())?;
        let mut err: *mut c_char = ptr::null_mut();
        let raw = unsafe { umiak_zim_open(cpath.as_ptr(), &mut err) };
        if raw.is_null() {
            return Err(take_err(err));
        }
        Ok(Archive { raw })
    }

    pub fn entry_count(&self) -> u32 {
        unsafe { umiak_zim_entry_count(self.raw) }
    }

    pub fn article_count(&self) -> u32 {
        unsafe { umiak_zim_article_count(self.raw) }
    }

    pub fn main_path(&self) -> Result<String, String> {
        let mut err: *mut c_char = ptr::null_mut();
        let p = unsafe { umiak_zim_main_path(self.raw, &mut err) };
        if p.is_null() {
            return Err(take_err(err));
        }
        Ok(take_string(p).unwrap_or_default())
    }

    pub fn metadata(&self, key: &str) -> Option<String> {
        let ckey = CString::new(key).ok()?;
        let p = unsafe { umiak_zim_metadata(self.raw, ckey.as_ptr()) };
        take_string(p)
    }

    /// Fetch one item: Ok(Some((mime, bytes))), Ok(None) when absent.
    pub fn get(&self, path: &str) -> Result<Option<(String, Vec<u8>)>, String> {
        let cpath = CString::new(path).map_err(|_| "path contains NUL".to_string())?;
        let mut out: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let mut mime = vec![0u8; 128];
        let mut err: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            umiak_zim_get(
                self.raw,
                cpath.as_ptr(),
                &mut out,
                &mut out_len,
                mime.as_mut_ptr() as *mut c_char,
                mime.len(),
                &mut err,
            )
        };
        match rc {
            1 => {
                let bytes = unsafe {
                    let v = std::slice::from_raw_parts(out, out_len).to_vec();
                    umiak_zim_free_buf(out);
                    v
                };
                let mime_str = CStr::from_bytes_until_nul(&mime)
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "application/octet-stream".to_string());
                Ok(Some((mime_str, bytes)))
            }
            0 => Ok(None),
            _ => Err(take_err(err)),
        }
    }

    /// Full-text search inside this archive. Errors when the archive
    /// carries no full-text index.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<(String, String)>, String> {
        let cquery = CString::new(query).map_err(|_| "query contains NUL".to_string())?;
        let mut err: *mut c_char = ptr::null_mut();
        let p = unsafe { umiak_zim_search(self.raw, cquery.as_ptr(), limit, &mut err) };
        if p.is_null() {
            return Err(take_err(err));
        }
        Ok(parse_hits(&take_string(p).unwrap_or_default()))
    }

    /// Title suggestions; works on archives without a full-text index.
    pub fn suggest(&self, query: &str, limit: u32) -> Result<Vec<(String, String)>, String> {
        let cquery = CString::new(query).map_err(|_| "query contains NUL".to_string())?;
        let mut err: *mut c_char = ptr::null_mut();
        let p = unsafe { umiak_zim_suggest(self.raw, cquery.as_ptr(), limit, &mut err) };
        if p.is_null() {
            return Err(take_err(err));
        }
        Ok(parse_hits(&take_string(p).unwrap_or_default()))
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        unsafe { umiak_zim_close(self.raw) }
    }
}
