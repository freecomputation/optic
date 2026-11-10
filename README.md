# Optic

Optic is the total artificial system of Free Computation. The
design is in DESIGN.md. In one paragraph: the system's substance
is text, it is viewed and molded through a single window, and
the name is the computing term for exactly that - an optic, one
composable instrument whose two operations are view and update.

This repository is the total. The components live in their own
repositories and are mirrored into components/ here, so the
whole system can be read, carried, and grown in one place:

- components/idem - grow-system: the substrate, the grower, the
broker, the runner, the editions, from freecomputation/idem.
- components/umiak - keep-files: the browser and its shelf, from
freecomputation/umiak.
- components/siuti - hear-signals: the signals ear, from
freecomputation/siuti.
- components/pisuk - carry-system: the walking harness, from
freecomputation/pisuk.
- components/tarraq - reflect-code: the self-reflecting
evaluator, from freecomputation/tarraq.

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

## The window manager - the-window

Optic is the window manager itself. It is not a window inside a
desktop. It is the desktop.

- the-window - Optic itself: the window manager and the desktop,
one white field with a slight blue aura, behaving like a text
editor.

- grow-system - idem, the repo handle freecomputation/idem: the
substrate that grows the system from material aboard.

- keep-files - Umiak, the repo handle freecomputation/umiak: the
browser and shelf that keep files aboard and usable offline.

- hear-signals - Siuti, the repo handle freecomputation/siuti:
the ear that hears radio signals and records them as events.

- carry-system - Pisuk, the repo handle freecomputation/pisuk:
the harness that carries the system from host to host unchanged.

- reflect-code - Tarraq, the repo handle freecomputation/tarraq:
the evaluator that reflects on code, evaluating its own text.

The desktop is one document, desktop.txt in a world. Run it with
"optic window <world>" and open the local address it prints, by
default on port 27340. The surface is a full white field with a
slight blue aura at the edges, a canvas, and the desktop text over
it. Nothing else is on the surface except three plain controls at
the bottom: Apply, Refresh, and Reset.

The environment is programmable on the spot. A line in the text
can set a variable, set a color, clear the field, or draw, and
the canvas re-renders on every keystroke as the text changes.

- set <name> <number> - a variable usable as $<name> in numeric
positions in later lines.

- color <css-color> - the stroke and fill for following draws.

- clear - clears the canvas at that point in the text.

- draw circle <x> <y> <r> - a circle composed from text.

- draw rect <x> <y> <w> <h> - a rectangle composed from text.

- draw line <x1> <y1> <x2> <y2> - a line composed from text.

- draw text <x> <y> <words...> - words composed from text.

Graphics are compositions from simple draw text. They are never
attachments. Any other line is plain editor text and draws
nothing.

Apply saves the editor text as desktop.txt through the chain, so
the change is a kept edit. Refresh reloads the editor from the
chain, from GET /api/text. Reset returns to the founding
document, applying the founding desktop text again through the
chain. A world that fails verification is refused and not served.

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
that is viewed, molded, and chained.

## Directed evolution, v1

Evolution in Optic is directed: nothing mutates behind the
owner's back. "optic propose" files a mutation in the chain as
a proposal - the proposed text stored by its hash under
proposals/, the document itself untouched. "optic mutate" is
the world proposing its own variation: one of three small
operators (rotate the paragraphs, echo the first line, trim the
last paragraph), chosen deterministically from the document's
head hash and the chain head, so the same world always proposes
the same mutation. "optic keep" applies a proposal as a kept
edit that names the proposal it came from; "optic discard"
refuses it, and the refusal stays in the chain as a fossil.
Double resolution is refused. A proposal whose stored text has
been touched outside the window cannot be kept.

"optic graft" is the islands meeting: another world's documents
are offered to this one as proposals, whole-document states
through the same keep and discard gate - identical documents
are noted and not offered. Cross-pollination therefore enters
under the owner's hand, and both phylogenies record it. Textual
merging of diverged documents is deliberately not attempted;
the owner molds the kept state afterward if the graft needs it.

v0 chains still verify: legacy is judged per entry, from its
own line, so a v0 world keeps growing under new code and every
entry verifies under the shape it was written with - never
re-hashed under new rules.

## Witnessed chains, v2

A world's phylogeny can be co-signed, the way idem's edition
log is. "optic keygen" writes an Ed25519 key in the house
format (the loader also reads idem grower keys, so one key can
witness both systems). "optic witness" signs the current chain
head - the signed body is domain-separated, optic-witness, so
neither system's signatures can be replayed as the other's -
and appends the witness as its own line in chain.jsonl, where
it names exactly the head it stands over. Verification checks
every witness at its position: a witness line moved to stand
over a different head is refused, and a forged signature is
refused at its line. Witnessing the same head twice with the
same key appends nothing.

## License

AGPL-3.0.
