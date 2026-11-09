// Optic: the window onto a world of text.
//
// A world is a directory. Inside it, docs/ holds the living
// documents - plain text, the whole substance of the system -
// and chain.jsonl is the phylogeny: an append-only, hash-chained
// record of every edit the world has ever taken, written in the
// same shape as idem's edition log. Nothing about a world is
// hidden in a binary. Read the directory and you have read the
// system.
//
// The window has the two operations its name promises, and two
// that serve them:
//
//   optic seed <world>            found a world with one document
//   optic view <world> [doc]      see the world, or one document
//   optic update <world> <doc>    mold a document (new text on stdin)
//   optic log <world>             the phylogeny, verified
//   optic compose <world>         a picture composed from the text
//
// Every view verifies before it shows: the chain must hash
// unbroken from GENESIS, and every document's current bytes must
// answer to the hash its latest edit committed. A world that has
// been touched outside the window does not render - it reports.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;

fn fail(msg: &str) -> ! {
    eprintln!("optic: {msg}");
    exit(1);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn short(hash: &str) -> String {
    hash.chars().take(10).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edit {
    seq: u64,
    doc: String,
    before: Option<String>,
    after: String,
    prev: String,
    hash: String,
}

fn entry_body(seq: u64, doc: &str, before: &Option<String>, after: &str, prev: &str) -> String {
    format!(
        "{seq}|{doc}|{}|{after}|{prev}",
        before.as_deref().unwrap_or("-")
    )
}

fn chain_path(world: &Path) -> PathBuf {
    world.join("chain.jsonl")
}

fn docs_dir(world: &Path) -> PathBuf {
    world.join("docs")
}

fn read_chain(world: &Path) -> Vec<Edit> {
    let text = std::fs::read_to_string(chain_path(world))
        .unwrap_or_else(|_| fail("no chain.jsonl here - is this a world? seed one first"));
    let mut edits = Vec::new();
    let mut prev = "GENESIS".to_string();
    for (i, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let e: Edit = serde_json::from_str(line)
            .unwrap_or_else(|err| fail(&format!("chain line {} unreadable: {err}", i + 1)));
        if e.seq != (i + 1) as u64 {
            fail(&format!("chain entry {} claims seq {}", i + 1, e.seq));
        }
        if e.prev != prev {
            fail(&format!(
                "chain broken at entry {}: it stands on {}, the chain's head there is {}",
                e.seq, e.prev, prev
            ));
        }
        let want = sha256_hex(entry_body(e.seq, &e.doc, &e.before, &e.after, &e.prev).as_bytes());
        if want != e.hash {
            fail(&format!("chain entry {} does not hash to its line", e.seq));
        }
        prev = e.hash.clone();
        edits.push(e);
    }
    edits
}

// The head hash each document currently answers to, from the chain.
fn heads(edits: &[Edit]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for e in edits {
        m.insert(e.doc.clone(), e.after.clone());
    }
    m
}

// Verify every document on disk against the chain, and return the
// verified current texts. This is the gate every view passes.
fn verified_world(world: &Path) -> (Vec<Edit>, BTreeMap<String, String>) {
    if !world.exists() {
        fail(&format!("no world at {}", world.display()));
    }
    let edits = read_chain(world);
    let want = heads(&edits);
    let mut texts = BTreeMap::new();
    let dir = docs_dir(world);
    if dir.exists() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(entry.path()).unwrap();
            match want.get(&name) {
                Some(h) if *h == sha256_hex(&bytes) => {
                    texts.insert(name, String::from_utf8_lossy(&bytes).to_string());
                }
                Some(h) => fail(&format!(
                    "document {name} does not answer to the chain - its head is {}, its bytes hash to {} - it was touched outside the window",
                    short(h),
                    short(&sha256_hex(&bytes))
                )),
                None => fail(&format!(
                    "document {name} exists but no edit in the chain ever made it - it was placed outside the window"
                )),
            }
        }
    }
    for name in want.keys() {
        if !texts.contains_key(name) {
            fail(&format!("document {name} is in the chain but missing from docs/"));
        }
    }
    (edits, texts)
}

fn append_edit(world: &Path, doc: &str, new_text: &str) {
    let edits = if chain_path(world).exists() {
        read_chain(world)
    } else {
        Vec::new()
    };
    let prev = edits
        .last()
        .map(|e| e.hash.clone())
        .unwrap_or_else(|| "GENESIS".to_string());
    let doc_path = docs_dir(world).join(doc);
    let before = if doc_path.exists() {
        Some(sha256_hex(&std::fs::read(&doc_path).unwrap()))
    } else {
        None
    };
    let after = sha256_hex(new_text.as_bytes());
    let seq = edits.len() as u64 + 1;
    let hash = sha256_hex(entry_body(seq, doc, &before, &after, &prev).as_bytes());
    let edit = Edit {
        seq,
        doc: doc.to_string(),
        before,
        after,
        prev,
        hash,
    };
    std::fs::create_dir_all(docs_dir(world)).unwrap();
    std::fs::write(&doc_path, new_text).unwrap();
    let mut chain = std::fs::read_to_string(chain_path(world)).unwrap_or_default();
    if !chain.is_empty() && !chain.ends_with('\n') {
        chain.push('\n');
    }
    chain.push_str(&serde_json::to_string(&edit).unwrap());
    chain.push('\n');
    std::fs::write(chain_path(world), chain).unwrap();
    println!(
        "- edit {}: {} {} -> {}",
        edit.seq,
        edit.doc,
        edit.before.as_deref().map(short).unwrap_or_else(|| "new".into()),
        short(&edit.after)
    );
}

fn cmd_seed(world: &Path) {
    if world.exists() {
        fail(&format!("{} already exists", world.display()));
    }
    std::fs::create_dir_all(docs_dir(world)).unwrap();
    let founding = "This is a world of Optic.\n\nIt is made of text, it is viewed and molded through one\nwindow, and every shape it has ever held is in its chain.\nMold it: optic update <world> world.txt, new text on stdin.\n";
    append_edit(world, "world.txt", founding);
    println!("world seeded at {}", world.display());
}

fn cmd_view(world: &Path, only: Option<&str>) {
    let (edits, texts) = verified_world(world);
    match only {
        Some(name) => match texts.get(name) {
            Some(t) => print!("{t}"),
            None => fail(&format!("no document named {name} in this world")),
        },
        None => {
            println!(
                "world: {} documents, {} edits, chain head {}",
                texts.len(),
                edits.len(),
                edits.last().map(|e| short(&e.hash)).unwrap_or_else(|| "GENESIS".into())
            );
            for (name, text) in &texts {
                println!();
                println!("== {name} ==");
                print!("{text}");
                if !text.ends_with('\n') {
                    println!();
                }
            }
        }
    }
}

fn cmd_update(world: &Path, doc: &str) {
    if !world.exists() {
        fail(&format!("no world at {} - seed one first", world.display()));
    }
    // The world's existing state must verify before it is molded.
    if chain_path(world).exists() {
        let _ = verified_world(world);
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .unwrap_or_else(|e| fail(&format!("cannot read new text from stdin: {e}")));
    append_edit(world, doc, &text);
}

fn cmd_log(world: &Path) {
    let (edits, _) = verified_world(world);
    for e in &edits {
        println!(
            "- {}: {} {} -> {}",
            e.seq,
            e.doc,
            e.before.as_deref().map(short).unwrap_or_else(|| "new".into()),
            short(&e.after)
        );
    }
    println!(
        "chain verified: {} edits, unbroken from GENESIS",
        edits.len()
    );
}

fn cmd_compose(world: &Path) {
    // A picture composed from the text: the world's edit history,
    // one bar per document, drawn from the chain's own numbers.
    let (edits, texts) = verified_world(world);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &edits {
        *counts.entry(e.doc.clone()).or_default() += 1;
    }
    let width = counts.values().max().copied().unwrap_or(0).max(1);
    println!("the world, composed from its chain");
    println!();
    for (name, text) in &texts {
        let n = counts.get(name).copied().unwrap_or(0);
        let bar: String = "-".repeat(((n * 40) / width).max(1));
        println!("{name}  {bar} {n} edits, {} bytes", text.len());
    }
    println!();
    println!("{} edits in total; every bar above is a count taken from the chain itself.", edits.len());
}

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  optic seed <world>");
    eprintln!("  optic view <world> [doc]");
    eprintln!("  optic update <world> <doc>   (new text on stdin)");
    eprintln!("  optic log <world>");
    eprintln!("  optic compose <world>");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        usage();
    }
    let world = PathBuf::from(&args[1]);
    match args[0].as_str() {
        "seed" => cmd_seed(&world),
        "view" => cmd_view(&world, args.get(2).map(|s| s.as_str())),
        "update" => match args.get(2) {
            Some(doc) => cmd_update(&world, doc),
            None => usage(),
        },
        "log" => cmd_log(&world),
        "compose" => cmd_compose(&world),
        _ => usage(),
    }
}
