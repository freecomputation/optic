# Optic

Optic is the total artificial system of Free Computation. The
design is in DESIGN.md. In one paragraph: the system's substance
is text, it is viewed and molded through a single window, and
the name is the computing term for exactly that - an optic, one
composable instrument whose two operations are view and update.

This repository is the total. The components live in their own
repositories and are mirrored into components/ here, so the
whole system can be read, carried, and grown in one place:

- components/idem - the substrate: the grower, the broker, the
runner, the editions, from freecomputation/idem.
- components/umiak - the browser and its shelf, from
freecomputation/umiak.
- components/siuti - the signals ear, from freecomputation/siuti.
- components/pisuk - the walking harness, from
freecomputation/pisuk.

## How the mirror stays current

"python3 tools/sync.py" clones or pulls each component
repository and refreshes its mirror from that repository's main
branch, then rewrites components/MANIFEST.json with the exact
upstream commit each mirror was taken from. The standing rule of
the house is that a component landing is not finished until the
Optic refresh that carries it has landed here too, so this
repository moves whenever any of its parts move.

The mirror is a copy, never an original. Nothing under
components/ is edited by hand. To change a component, change it
in its own repository, land it there, and sync. A file whose
bytes are not valid UTF-8 cannot travel through the text
channels the mirror uses; the sync skips it and the manifest
names it under "excluded", so every omission is a record rather
than a silence.

## The window, v0

The optic binary is the single window. A world is a directory:
docs/ holds the living documents as plain text, and chain.jsonl
is the phylogeny - an append-only, hash-chained record of every
edit, in the shape of idem's edition log. "optic seed" founds a
world, "optic view" shows it, "optic update" molds a document
with new text taken from stdin, "optic log" prints the verified
phylogeny, and "optic compose" draws a picture composed from
the chain's own numbers - bars of edit counts, graphics as a
rendering of text, never an attachment.

Every view verifies before it shows. The chain must hash
unbroken from GENESIS, and every document's current bytes must
answer to the hash its latest edit committed. Proven in the
first battery: a world copied whole to a new path views
identically; a document touched outside the window is refused
with both hashes named; a rewritten chain entry is refused at
its line.

The grain of v0 is the document: a document is the individual
that is viewed, molded, and chained. The design's recommended
grain for the governed generations ahead is the derivation,
which already carries a receipt and a gate; that is v1's work,
when the system begins proposing its own mutations through the
broker.

## License

AGPL-3.0.
