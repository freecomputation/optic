use std::env;
use std::path::PathBuf;

// Builds the C++ shim against libzim headers and links the libzim runtime.
//
// Header location, first hit wins:
//   1. COFFER_LIBZIM_INCLUDE (explicit)
//   2. ../../third-party/libzim-dev/usr/include (headers extracted from the
//      distro libzim-dev package, used when the -dev package cannot be
//      installed alongside pinned system libraries)
//   3. system include path (plain <zim/...> resolution)
//
// Library: with COFFER_LIBZIM_LIB set, that exact shared object is linked by
// soname symlink in OUT_DIR. Otherwise a system libzim.so is expected.
fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").file("cpp/zim_shim.cpp");

    if let Ok(inc) = env::var("COFFER_LIBZIM_INCLUDE") {
        build.include(inc);
    } else {
        let vendored = manifest.join("../../third-party/libzim-dev/usr/include");
        if vendored.join("zim/archive.h").exists() {
            build.include(vendored);
        }
    }
    build.compile("zim_shim");

    if let Ok(lib) = env::var("COFFER_LIBZIM_LIB") {
        let out = PathBuf::from(env::var("OUT_DIR").unwrap());
        let link = out.join("libzim.so");
        // A dangling symlink (its target moved or vanished) reads as
        // absent to exists() but still blocks creation; check the
        // link itself, and re-point it when the target has changed.
        let stale = match std::fs::read_link(&link) {
            Ok(target) => target != std::path::PathBuf::from(&lib),
            Err(_) => std::fs::symlink_metadata(&link).is_ok(),
        };
        if stale {
            let _ = std::fs::remove_file(&link);
        }
        if std::fs::symlink_metadata(&link).is_err() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&lib, &link).expect("symlink libzim");
        }
        println!("cargo:rustc-link-search=native={}", out.display());
    }
    println!("cargo:rustc-link-lib=zim");
    println!("cargo:rerun-if-changed=cpp/zim_shim.cpp");
    println!("cargo:rerun-if-changed=cpp/zim_shim.h");
    println!("cargo:rerun-if-env-changed=COFFER_LIBZIM_INCLUDE");
    println!("cargo:rerun-if-env-changed=COFFER_LIBZIM_LIB");
}
