# Coffer - design determinations

Coffer is the browser and shelf: it keeps books, maps, and pages
aboard and usable offline.

Determined 2026-11-09 by commissioned research over the whole design
space, against the Fixity charter's laws. The full sourced report is the
project record; this file is the working summary the code answers to.

Coffer ships first: a standalone browser, fully functional with no
internet, whose internet is the Shelf aboard. Fixity, the system, grows
underneath it later, and Coffer becomes its shell.

Engine: system webviews through Tauri 2 and Wry (WebView2, WKWebView, WebKitGTK, Android WebView), all content served over loopback HTTP. Fallback: CEF. Designated future swap: Servo. Ladybird is not ready.

Content: ZIM through libzim directly, behind one thin isolated FFI, served by our own Rust server following Kiwix URL and OPDS conventions. libkiwix is a prototype aid, never a dependency.

Search: Xapian inside each corpus through libzim's own searcher, untouched. Across the Shelf: fan-out plus rank fusion, with SQLite FTS5 for the catalogue and the user's own material. Tantivy is held for Coffer-native and grown content.

Language: Rust end to end. C++ exists only behind the FFI shim.

Growth (idem): the Nix derivation model, no existing implementation. Our own grower, derivations as data, small enough to read whole.

Base (idem): FreeBSD 15, ratified 2026-11-09 - Capsicum gives the broker its capability model natively, one coherent base tree, ZFS snapshots and boot environments, veriexec signed manifests. Bedrock only: replaceable, sealed, outside the trusted core, which is the seed, the grower, the verifiers, the capability broker, and the keys. Gated on a hardware census of cheap machines before the system phase; the Linux immutable image pattern is the standing fallback.

Grown code (idem): WebAssembly components on Wasmtime, WASI, deterministic profile with fuel. Nothing grown holds ambient authority.

Intelligence (both): an unprivileged local model service proposing through the capability broker. The owner's power structure is a machine, not a policy: the broker runs a two attempt state machine, first attempt returns a signed warning, second attempt requires the owner's confirmation to mint a one shot token bound to the action. The intelligence cannot mint tokens or confirm itself.

Sync: desync chunks over any transport, including a folder on a USB stick. Verification: Ed25519 signed edition manifests plus a witness cosigned transparency log whose proofs travel offline.

Fetching (Coffer, determined by the builds of 2026-11-09): the Shelf holds two formats, ZIM through the FFI and EPUB read directly (the spine becomes the contents page, and the book's own files are served at their zip paths so its links and images resolve). Catalogs are OPDS: Kiwix as the stocking catalog, Gutenberg and Unglue.it and SermonIndex as OPDS 1.x side doors (Gutenberg needs a second hop from its search feed to each book page), Open Library as OPDS 2.0 discovery whose books are lent, not shelved, because its feed offers no file to take. Downloads run in parallel and resume from partial files by range request, because a connection that comes and goes must still finish the book. TLS is the platform stack and the machine's own certificate store, never a bundled root list: a browser that cannot verify what the reader's other programs verify is broken on real networks. A configured proxy is a supply line to the outside only - loopback, private addresses, and NO_PROXY hosts are always fetched directly.

Sync (Coffer, determined by the builds of 2026-11-09): editions reach the Shelf as content-defined chunks under a signed manifest - the desync shape, with the manifest verification of the Fixity capsule gate at shelf scale. Chunking is gear-hash content-defined cutting (minimum 64 KiB, boundary mask at 256 KiB average, maximum 1 MiB, table derived from a fixed seed so every copy cuts identically). Mirrors are dumb: an http directory or a folder on a carried disk, holding edition.json and chunks named by their hashes. The pool is content-addressed and shared across editions and books, and assembly verifies the whole file against the manifest before anything is shelved. Signed editions verify against trusted-keys.txt in the shelf directory; unsigned ones need the owner's explicit tick. The measured case from the first end-to-end run: one byte changed in a 3.3 MB book, one chunk fetched.

Maps (Coffer, determined by the builds of 2026-11-09): the map layer is PMTiles read by our own reader in the server - one file per map on the Shelf, tiles served over the same loopback HTTP as everything else. Raster archives render in a server-drawn tile grid that needs no scripts and no tile server; vector (MVT) archives are served at the tile endpoint, with their rendering library a later vendoring decision rather than a network dependency.


First benchmarks owed: desync chunk reuse across successive
recompressed ZIM editions, and search latency on genuinely weak
hardware. Neither has published numbers anywhere.
