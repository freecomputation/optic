// Propagule: the walking harness.
//
// The machine is the harness, not the hardware. Propagule carries
// three of idem's tools - the grower, the broker, the runner - and
// a content-addressed store, and it walks them from host to host.
// The host lends its processor; everything above that line is
// carried, and is the same everywhere it walks.
//
// This shell is the only per-host code, and it is kept small on
// purpose. It does three things. It seals a kit: a manifest
// (propagule.json) pinning the tools it walks with by hash. It
// verifies a kit before anything runs: a tampered tool means the
// harness is not itself today, and it refuses. And it runs: a
// component out of a carried store, through the verified runner,
// under the runner's own meter.
//
//   propagule seal --kit <dir>
//   propagule verify --kit <dir>
//   propagule run --kit <dir> --store <dir> --object <sha256> [--fuel N]
//
// The acceptance test from the design: the same derivation, from
// the same store, run through kits at two different places,
// returns the same result and the same fuel reading.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const TOOLS: [&str; 3] = ["grower", "broker", "runner"];

fn fail(msg: &str) -> ! {
    eprintln!("propagule: {msg}");
    exit(1);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", path.display())));
    sha256_hex(&bytes)
}

#[derive(Debug, Serialize, Deserialize)]
struct KitManifest {
    format: String,
    tools: Vec<ToolPin>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolPin {
    name: String,
    sha256: String,
}

fn manifest_path(kit: &Path) -> PathBuf {
    let current = kit.join("propagule.json");
    if current.exists() {
        return current;
    }
    // Kits sealed before the rename carry pisuk.json; they still walk.
    let legacy = kit.join("pisuk.json");
    if legacy.exists() {
        return legacy;
    }
    current
}

fn cmd_seal(kit: &Path) {
    let mut tools = Vec::new();
    for name in TOOLS {
        let path = kit.join(name);
        if !path.exists() {
            fail(&format!(
                "kit is missing {name} - a kit is a directory holding grower, broker, and runner"
            ));
        }
        let sha = hash_file(&path);
        println!("- {name} -> {sha}");
        tools.push(ToolPin {
            name: name.to_string(),
            sha256: sha,
        });
    }
    let manifest = KitManifest {
        format: "propagule-kit/0".to_string(),
        tools,
    };
    let text = serde_json::to_string_pretty(&manifest).unwrap();
    std::fs::write(manifest_path(kit), text + "\n")
        .unwrap_or_else(|e| fail(&format!("cannot write manifest: {e}")));
    println!("kit sealed: {}", manifest_path(kit).display());
}

fn verify_kit(kit: &Path) -> KitManifest {
    let text = std::fs::read_to_string(manifest_path(kit)).unwrap_or_else(|_| {
        fail("kit carries no propagule.json - seal it first, and do not walk with an unsealed kit")
    });
    let manifest: KitManifest =
        serde_json::from_str(&text).unwrap_or_else(|e| fail(&format!("kit manifest unreadable: {e}")));
    if manifest.format != "propagule-kit/0" && manifest.format != "pisuk-kit/0" {
        fail(&format!("unknown kit format {}", manifest.format));
    }
    for pin in &manifest.tools {
        let path = kit.join(&pin.name);
        if !path.exists() {
            fail(&format!("kit is missing its {}", pin.name));
        }
        let actual = hash_file(&path);
        if actual != pin.sha256 {
            fail(&format!(
                "the kit's {} does not answer to the manifest - pinned {}, found {} - refusing to walk with a tool that is not itself",
                pin.name, pin.sha256, actual
            ));
        }
    }
    manifest
}

fn cmd_verify(kit: &Path) {
    let manifest = verify_kit(kit);
    println!(
        "kit verified: {} tools answer to the manifest",
        manifest.tools.len()
    );
}

fn cmd_run(kit: &Path, store: &Path, object: &str, fuel: Option<u64>) {
    verify_kit(kit);
    if !store.join("objects").join(object).exists() {
        fail(&format!("object {object} is not in the store at {}", store.display()));
    }
    let runner = kit.join("runner");
    let mut cmd = Command::new(&runner);
    cmd.arg("run-component")
        .arg("--object")
        .arg(object)
        .arg("--store")
        .arg(store);
    if let Some(f) = fuel {
        cmd.arg("--fuel").arg(f.to_string());
    }
    let status = cmd
        .status()
        .unwrap_or_else(|e| fail(&format!("cannot start the runner: {e}")));
    if !status.success() {
        exit(status.code().unwrap_or(1));
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  propagule seal --kit <dir>");
    eprintln!("  propagule verify --kit <dir>");
    eprintln!("  propagule run --kit <dir> --store <dir> --object <sha256> [--fuel N]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let kit = flag(&args, "--kit").map(PathBuf::from);
    match args[0].as_str() {
        "seal" => cmd_seal(&kit.unwrap_or_else(|| usage())),
        "verify" => cmd_verify(&kit.unwrap_or_else(|| usage())),
        "run" => {
            let store = flag(&args, "--store").map(PathBuf::from);
            let object = flag(&args, "--object");
            let fuel = flag(&args, "--fuel").and_then(|v| v.parse().ok());
            match (kit, store, object) {
                (Some(k), Some(s), Some(o)) => cmd_run(&k, &s, &o, fuel),
                _ => usage(),
            }
        }
        _ => usage(),
    }
}
