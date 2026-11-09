//! Editions and chunk sync for the Shelf.
//!
//! An edition is a signed manifest naming shelf files and, for
//! each file, the content-defined chunks it breaks into. A mirror
//! is any place that holds the manifest and the chunks - an HTTP
//! server, or a directory on a disk that arrived by hand. Syncing
//! fetches only the chunks the shelf does not already have, which
//! is what makes editions cheap to carry: a new edition of a book
//! reuses most of the old one's chunks, an interrupted sync resumes
//! chunk by chunk, and chunks shared between books move once.
//!
//! Trust is the idem shape, at shelf scale: a manifest that carries
//! a signer must be signed by a key the owner has named trusted,
//! and every chunk and every assembled file is verified by hash
//! before it is shelved. An unsigned edition is refused unless the
//! owner allows it out loud for that sync.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

// Content-defined chunking, gear-hash family: a rolling hash over
// the bytes, a boundary where the masked hash is zero once the
// minimum size has passed, a hard boundary at the maximum. The gear
// table is generated from a fixed seed so every Umiak everywhere
// cuts the same file into the same chunks.
pub const CHUNK_MIN: usize = 64 * 1024;
pub const CHUNK_MAX: usize = 1024 * 1024;
const CHUNK_MASK: u64 = (1 << 18) - 1;

fn gear_table() -> [u64; 256] {
    let mut table = [0u64; 256];
    let mut state: u64 = 0x243F6A8885A308D3;
    for entry in table.iter_mut() {
        // splitmix64: deterministic, well distributed.
        state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        *entry = z ^ (z >> 31);
    }
    table
}

/// The chunk boundaries of a byte slice, as (offset, length) pairs
/// covering the whole input with no gaps.
pub fn chunk_bounds(data_len: usize, mut read: impl FnMut(usize, usize) -> Vec<u8>) -> Vec<(usize, usize)> {
    let gear = gear_table();
    let mut bounds = Vec::new();
    let mut start = 0usize;
    let mut pos = 0usize;
    let mut hash: u64 = 0;
    // Read in blocks; the rolling hash only needs the byte stream.
    const BLOCK: usize = 64 * 1024;
    let mut block: Vec<u8> = Vec::new();
    let mut block_start = 0usize;
    while pos < data_len {
        if pos >= block_start + block.len() {
            let want = BLOCK.min(data_len - pos);
            block = read(pos, want);
            block_start = pos;
            if block.is_empty() {
                break;
            }
        }
        let byte = block[pos - block_start];
        hash = (hash << 1).wrapping_add(gear[byte as usize]);
        pos += 1;
        let here = pos - start;
        if (here >= CHUNK_MIN && (hash & CHUNK_MASK) == 0) || here >= CHUNK_MAX {
            bounds.push((start, here));
            start = pos;
            hash = 0;
        }
    }
    if start < data_len {
        bounds.push((start, data_len - start));
    }
    if data_len == 0 {
        bounds.push((0, 0));
    }
    bounds
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditionFile {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
    pub chunks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edition {
    pub format: String,
    pub name: String,
    pub files: Vec<EditionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// The bytes an edition signature answers to: the name and, for
/// each file, its name, hash, and size in a fixed order. Chunks are
/// not signed individually; each is verified by its own hash, and
/// the file hash the signature does cover is verified whole.
pub fn edition_body(e: &Edition) -> Vec<u8> {
    let mut body = String::new();
    body.push_str(&format!("name: {}\n", e.name));
    for f in &e.files {
        body.push_str(&format!("file: {} {} {}\n", f.name, f.sha256, f.bytes));
    }
    body.into_bytes()
}

pub struct Chunked {
    pub file: EditionFile,
    pub chunks: Vec<(String, Vec<u8>)>,
}

/// Break one file's bytes into its edition entry and chunks.
pub fn chunk_bytes(name: &str, data: &[u8]) -> Chunked {
    let bounds = chunk_bounds(data.len(), |off, len| {
        data[off..off + len].to_vec()
    });
    let mut chunks = Vec::new();
    let mut hashes = Vec::new();
    for (off, len) in bounds {
        let slice = &data[off..off + len];
        let hash = sha256_hex(slice);
        hashes.push(hash.clone());
        chunks.push((hash, slice.to_vec()));
    }
    Chunked {
        file: EditionFile {
            name: name.to_string(),
            sha256: sha256_hex(data),
            bytes: data.len() as u64,
            chunks: hashes,
        },
        chunks,
    }
}

/// Where chunks live on this machine: a content-addressed pool in
/// the shelf directory, so every edition and every book shares it.
pub fn pool_dir(shelf_dir: &Path) -> PathBuf {
    shelf_dir.join(".umiak-chunks")
}

pub fn pool_has(pool: &Path, chunk: &str) -> bool {
    pool.join(chunk).exists()
}

pub fn pool_put(pool: &Path, chunk: &str, bytes: &[u8]) -> Result<(), String> {
    if sha256_hex(bytes) != chunk {
        return Err(format!("chunk bytes do not hash to {chunk}"));
    }
    let path = pool.join(chunk);
    if !path.exists() {
        std::fs::write(&path, bytes).map_err(|e| format!("cannot store chunk: {e}"))?;
    }
    Ok(())
}

/// Assemble a file from the pool and verify it whole. Returns the
/// bytes; the caller decides where they are shelved.
pub fn assemble(pool: &Path, file: &EditionFile) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(file.bytes as usize);
    for chunk in &file.chunks {
        let bytes = std::fs::read(pool.join(chunk))
            .map_err(|_| format!("chunk {chunk} is not in the pool"))?;
        if sha256_hex(&bytes) != *chunk {
            return Err(format!("chunk {chunk} in the pool does not hash to its name"));
        }
        out.extend_from_slice(&bytes);
    }
    if sha256_hex(&out) != file.sha256 {
        return Err(format!(
            "assembled {} does not hash to the manifest's {}",
            file.name, file.sha256
        ));
    }
    Ok(out)
}

/// Read a whole local file for chunking without holding two copies
/// longer than needed; small helper for the pack path.
pub fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("cannot open: {e}"))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).map_err(|e| format!("cannot read: {e}"))?;
    Ok(out)
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}

/// Verify an edition's signature against a list of trusted public
/// keys (hex). Ok(()) only when signer is trusted and the signature
/// verifies; Err names the reason otherwise.
pub fn verify_edition(e: &Edition, trusted: &[String]) -> Result<String, String> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let (signer, signature) = match (&e.signer, &e.signature) {
        (Some(s), Some(g)) => (s, g),
        _ => return Err("edition is unsigned".to_string()),
    };
    if !trusted.iter().any(|t| t.eq_ignore_ascii_case(signer)) {
        return Err(format!("signer {signer} is not in the trust file"));
    }
    let key_bytes = hex_decode(signer)
        .filter(|b| b.len() == 32)
        .ok_or("signer is not 32 bytes of hex")?;
    let mut kb = [0u8; 32];
    kb.copy_from_slice(&key_bytes);
    let verifying = VerifyingKey::from_bytes(&kb).map_err(|_| "signer is not a valid public key")?;
    let sig_bytes = hex_decode(signature)
        .filter(|b| b.len() == 64)
        .ok_or("signature is not 64 bytes of hex")?;
    let mut sb = [0u8; 64];
    sb.copy_from_slice(&sig_bytes);
    if verifying
        .verify(&edition_body(e), &Signature::from_bytes(&sb))
        .is_err()
    {
        return Err("edition signature does not verify".to_string());
    }
    Ok(signer.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize) -> Vec<u8> {
        // Deterministic pseudo-random bytes.
        let mut out = Vec::with_capacity(n);
        let mut state: u64 = 0xDEADBEEF;
        for _ in 0..n {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            out.push((state >> 33) as u8);
        }
        out
    }

    #[test]
    fn chunking_covers_and_reassembles() {
        let d = data(3_500_000);
        let c = chunk_bytes("book.zim", &d);
        assert!(c.chunks.len() > 1, "3.5 MB should be several chunks");
        for (hash, bytes) in &c.chunks {
            assert_eq!(&sha256_hex(bytes), hash);
        }
        let total: usize = c.chunks.iter().map(|(_, b)| b.len()).sum();
        assert_eq!(total, d.len());
        assert_eq!(c.file.sha256, sha256_hex(&d));
    }

    #[test]
    fn chunking_is_deterministic() {
        let d = data(900_000);
        let a = chunk_bytes("x", &d);
        let b = chunk_bytes("x", &d);
        assert_eq!(a.file.chunks, b.file.chunks);
    }

    #[test]
    fn a_small_change_keeps_most_chunks() {
        let mut d = data(3_500_000);
        let before = chunk_bytes("x", &d).file.chunks;
        d[1_000_000] ^= 0xFF;
        d[1_500_000] ^= 0xFF;
        let after = chunk_bytes("x", &d).file.chunks;
        let kept = before.iter().filter(|h| after.contains(h)).count();
        assert!(
            kept * 2 > before.len(),
            "expected most chunks reused, kept {kept} of {}",
            before.len()
        );
    }

    #[test]
    fn empty_file_is_one_empty_chunk() {
        let c = chunk_bytes("empty", &[]);
        assert_eq!(c.file.chunks.len(), 1);
        assert_eq!(c.file.bytes, 0);
    }
}
