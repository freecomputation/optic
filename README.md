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

## License

AGPL-3.0.
