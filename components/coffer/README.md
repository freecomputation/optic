# Coffer

by Free Computation. AGPL-3.0.

A browser for people with unstable or no internet. Coffer is fully
functional with no connection at all: its internet is the Shelf, a
library aboard the machine - Wikipedia and thousands of other open
corpora in ZIM form, maps, books, documentation. When a connection
appears it drinks: verified content arrives in chunks, from the network
or from a stick passed hand to hand, and is checked before it is
shelved.

The name is the household word: a coffer is the strong chest a
family kept its valuables in - books, documents, letters - at
home, under the owner's own hand. Coffer keeps the library the
same way: aboard, verified, and openable with no one's permission.

Coffer is the first form of Fixity, the system that grows
underneath it.

## Layout

`crates/zim-ffi` - the only C++ in the project: a thin C shim over
libzim with a safe Rust wrapper.

`crates/coffer-core` - the Shelf: discovery and reading of the books
aboard.

`crates/coffer-server` - the loopback HTTP server every surface talks
to. The chrome, the Shelf home, book content, one federated search
across everything aboard, and the catalog.

`app/` - the Tauri shell: one system-webview window onto the server.
Deliberately thin; the engine is borrowed and swappable.

`tools/` - the fixture generator (indexed and no-index test books)
and `smoke.sh`, the end to end check.

## Build and run

Needs a Rust toolchain, a C++ compiler (one shim in `zim-ffi`), and
libzim (runtime plus headers). On a Debian-family system:

    sudo apt install -y build-essential libzim-dev

Then:

    COFFER_LIBZIM_LIB=$(ldconfig -p | grep -m1 'libzim.so' | awk '{print $NF}') \
      cargo run -p coffer-server -- --shelf /path/to/zims

(If `cargo` is not on your path, it lives in `~/.cargo/bin`.)

Where only the runtime is installed, extract the headers from the
libzim-dev package into `third-party/libzim-dev/` (the build finds
them there; the directory is git-ignored) and point
`COFFER_LIBZIM_LIB` at the runtime library, for example
`/lib/x86_64-linux-gnu/libzim.so.8`:

    apt-get download libzim-dev
    mkdir -p third-party/libzim-dev
    dpkg -x libzim-dev_*.deb third-party/libzim-dev

Then open http://127.0.0.1:27331/ - the chrome, with the Shelf inside
it. `sh tools/smoke.sh` builds fixture books and checks the whole
path end to end.

## The shell

The Tauri shell needs the system webview development libraries:

    sudo apt install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev \
      librsvg2-dev libxdo-dev libayatana-appindicator3-dev
    python3 tools/make-icons.py
    cd app/src-tauri && cargo build

The icons are generated (the Open Loop, drawn by the script), never
committed. At run time the shell starts `coffer-server` when it finds
it beside the executable, so put the built server next to the built
app, and run the app from a directory holding your shelf folder.

## Using it

The window is plain chrome: back, forward, reload, an address field
(a search, or a path like /book/0), and tabs, which are saved states
over one view. The star keeps a bookmark, and Marks and History sit
beside it; both live in a plain JSON file in the shelf directory, the
reader's own record on the reader's own disk. Everything is served by
the local server, so the same surfaces work in any browser pointed at
the loopback address.

The Shelf is the library: every ZIM in the shelf directory, searchable
as one collection, and every EPUB too - an EPUB opens at a contents
page built from its own navigation, its chapters and images served
from inside the book, and its text joins the shelf search. The catalog
reads the Kiwix OPDS feed when a connection exists, organized by
category and by publisher, and shelves what you pick; a progress page
shows the book coming down, and reading keeps working while it does.
Four more catalogs sit beside Kiwix in the source bar - Project
Gutenberg, Unglue.it, and Open Library for EPUB books, SermonIndex for
ZIMs - each entry naming a file the Shelf takes directly. Downloads
run several at a time, each with its own progress, and a fetch that
stops leaves its partial file on the shelf directory: fetch the same
book again and it resumes from those bytes when the source honors
range requests, so a connection that comes and goes still finishes
the book. Offline the catalogs simply say so, and nothing else
changes. The fetch honors the standard proxy environment variables
when they are set.

Maps ride on the Shelf too. A PMTiles archive - one file holding a
whole map pyramid, the format Protomaps publishes from city extracts
to the planet file - goes in the shelf directory beside the books,
and the Maps page lists what is aboard with its zooms and bounds.
Raster archives open in a built-in viewer: a tile grid with zoom and
pan links, no scripts and no tile server, everything read out of the
one file on disk. Vector archives are served tile by tile at
/map/<file>/tile/Z/X/Y for readers that speak vector tiles. The
reader is our own, in the server: header, directories, and the
archive's Hilbert-curve tile ids, checked against a real archive
whose directory it reproduces exactly.

The Shelf also grows by editions. An edition is a signed manifest
naming shelf files and the content-defined chunks each file breaks
into; a mirror is anywhere that holds the manifest and the chunks -
an http address, or a directory on a disk that arrived by hand. The
Sync page takes a mirror and fetches only the chunks this shelf
does not already have into a shared pool, verifies every chunk and
every assembled file by hash, and shelves what passes. A changed
byte in a book costs the one chunk that covers it; an interrupted
sync resumes from the pool; a tampered chunk is refused by its
hash and nothing is shelved. Signed editions must answer to a key
named in trusted-keys.txt in the shelf directory, and an unsigned
edition is refused unless the owner ticks the box on purpose - the
same discipline as the downloads, one level up. The publisher side
is a subcommand of the server itself:
"coffer-server edition pack <shelf-dir> <mirror-dir> <name>" with
"--key <key-file>" to sign.

## Design

See DESIGN.md for the determinations this code answers to.
