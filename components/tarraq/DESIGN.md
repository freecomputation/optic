# Tarraq - design

Tarraq is the living layer of Optic: a mutually recursive
machine, a true Lisp machine in the old sense, rebuilt under
this house's laws. Its name is Inuktitut - tarraq, glossed in
the old dictionaries as shade and as reflection. In the oldest
accounts a person's tarraq is their shadow-double, the same
shape as the body and inseparable from it. The name is also the
technical word for what the machine does. In computing,
reflection is a program inspecting and modifying itself.
Tarraq's defining act is reflection, and the name was already
the definition. It stands in the family with its siblings:
Siuti hears, Umiak carries, Pisuk walks. Tarraq reflects.

## The idea

Optic's worlds are text: readable, checkable, diffable,
signable, copyable. But a document today only sits there.
Tarraq makes the text live. One medium - S-expressions - in
which code and data are the same text, so a document can be a
program with no second format, and every program inherits the
world's governance for free: the chain ledgers it, the gate
proposes it, the witness can sign it.

At the center sits a seed evaluator: small enough to read in
one sitting, written in Tarraq's own language, grown as a
derivation under idem's rules and run inside the runner's
meter. The mutual recursion is literal. The evaluator's own
source is a document in the world it evaluates. Mold that
document through the window and the machine that evaluates the
next command is the machine the owner just rewrote - with the
chain witnessing the system change itself, edit by edit.

## The board

Tarraq is not run-once machinery. It is a board: a persistent
operating surface of named cells, each cell a definition or a
value, held as documents in a world. Values are computed by
the evaluator and shown through the window, live. Sessions
begin and end; the board does not. The store is its memory,
compost is its forgetting, and nothing about its state lives
outside the text. Killing every session loses nothing: the
board is exactly what the world says it is, verified before it
is shown, like everything else in Optic.

## The laws it runs under

Tarraq introduces no new law. It inherits the house's, applied
to living code:

- Turing complete, never unaccountable. The calculus has no
ceiling in principle; every actual evaluation draws fuel from
a grant, through the runner's meter. A computation that would
run forever does not hang the board - it starves, and the
board shows the corpse, in the chain.
- Reflection through the gate. The evaluator, and any cell on
the board, changes the way everything in Optic changes: by the
owner's direct hand, or as a proposal - from the model, from
another world, from the board itself - kept or discarded in
the open, the discard kept as a fossil.
- Capabilities by grant only. A cell that would touch anything
outside the board - a file, the shelf, the ear - asks the
broker, under the charter's warning sequence. The evaluator
itself holds no ambient authority.
- Growth by receipt. The machine's own parts are grown, not
installed: the seed evaluator is a derivation like any other,
and its grown form answers to its receipt on any host.

## The acceptance tests

- Completeness, shown: the seed evaluates the fixed-point
combinators running real recursion - a factorial through the
sage bird - and the classics of the calculus reduce to their
known values, deterministically, at a stated fuel cost.
- Reflection, shown: the evaluator, evaluating its own source
as a board document, runs a guest program to the same result
and the same fuel as the bootstrap does. The system's double
does the system's work, identically.
- The board persists: a board built up over many sessions,
carried to a second host the way Pisuk carries a store,
resumes with every cell's value intact and every pending
computation accounted.
- Starvation, shown: a non-terminating cell exhausts its grant
and the board records the exhaustion - result: none, fuel:
spent - without disturbing any other cell.

## Build order

- L0, the seed: the metacircular evaluator as a grown Wasm
component in the runner, its own source carried as a world
document from the first day, proving the completeness tests
against a bootstrap host evaluator.
- L1, the board: persistent cells in an Optic world, live
values through the window, proposals for definitions, compost
for dead cells.
- L2, the closing: the bootstrap hands over. The evaluator
document in the world becomes the evaluator of record; the
system runs its own text, and the first thing it evaluates is
itself.

## Open rulings

- Bootstrap host for L0: a Rust evaluator grown like the other
tools, recommended - it keeps the seed under the same receipts
as everything else, and L2 is the designed moment it retires.
- Cell grain: one definition per document, recommended - it
keeps the chain's per-document heads meaningful at the living
layer, and grafting between boards stays whole-document,
decidable by the owner.
- The board's first home: a dedicated world beside homeworld,
recommended, so the living layer's phylogeny starts clean.
