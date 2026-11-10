# Quine

Quine is the living layer of Palimpsest: a mutually recursive
machine, a true Lisp machine in the old sense. One medium -
S-expressions - in which code and data are the same text. At
its center sits a seed evaluator written in Quine's own
language, and the system closes over it: the evaluator
evaluates its own source, the copy it produces takes the
original's seat, and the system runs its own text. The design
lives in DESIGN.md. This file is about the machine as built.

The name is the computing word for exactly that closing: a
quine is a program that produces its own source as its output,
and this machine's evaluator, run on its own text, hands back
an evaluator that behaves identically - measured, the closing
is a fixed point, the same fuel at every layer. In the family:
Passband hears, Coffer carries, Propagule walks. Quine closes
over itself.

## The pieces

- src/main.rs - the bootstrap host: a small evaluator for the
core language, the fuel meter, the board, and the drivers.
It has no dependencies beyond the Rust standard library.
- seed.tq - the seed: a metacircular evaluator written in
Quine itself. Loaded by the host, it defines ev, the
evaluator a guest program runs under.
- examples/ - the classics as a runnable file, and a demo
board of three cells.
- recipes/quine.json - the growth recipe: the crate as an
fixity derivation, grown by the grower like everything else in
the house.

## The language

Forms: quote, if, lambda (fixed parameters, or a dotted rest
parameter), define in both shapes - (define name expr) and
(define (name args) body) - set!, begin, let, cond, and, or.

Values: integers (64-bit, checked arithmetic), booleans
written #t and #f, strings, symbols, pairs, procedures.

Primitives: + - * quotient remainder, the comparisons, cons
car cdr and the whole c<ad>+r family to depth four, list,
length, append, reverse, list-ref, the predicates null?
pair? symbol? number? boolean? string? procedure?, eq?,
equal?, not, map, set-car!, set-cdr!, apply, and fail, which
raises an error carrying its argument.

Every evaluation draws from a fuel grant. A computation that
would run forever does not hang anything - it starves, the
error names the fuel spent, and whatever it was computing for
keeps the rest of its state.

## Building

- cargo build --release
- cargo test --release - the battery: 12 tests

## Commands

- quine eval [--fuel N] <expr> - evaluate one expression
under the host
- quine run [--fuel N] <file> [expr] - load a file of forms
into a fresh environment, then evaluate the expression, or
the file's last value
- quine repl - the host, interactively
- quine seed-eval [--fuel N] <seed-file> <expr> - evaluate
the expression under the seed evaluator
- quine close-eval [--times K] [--fuel N] <seed-file> <expr>
- the closing: load the seed, then let the evaluator produce
its own replacement K times, then run the expression under
the evaluator of record
- quine board view <dir> - show every cell of a board and
its value
- quine board eval [--fuel N] <dir> <cell> - evaluate a cell
by name; an expression in place of a name is evaluated
against the board
- quine board set <dir> <name> - write a cell, its form read
from standard input
- quine board repl <dir> - a session against a board; the
session ends, the board does not

## The board

A board is a directory. Under it, cells/<name>.tq holds one
form per cell. A form shaped (define name expr) or
(define (name args) body) is unwrapped: the cell's name is
its file's name, and the cell's expression is the definition's
value, with a function signature rebuilt as a lambda. Any
other form is the cell's expression as written.

Cells evaluate lazily, on first touch, and the value is
memoized for the session. Cells name each other freely -
mutual recursion between cells is the ordinary case. A cycle
is refused and the error names the chain, a -> b -> a. A cell
that starves records nothing, and every other cell still
computes. Cross-references use cell names, so a cell's file
name is the name its neighbours call it by.

The board as shipped is a directory of cell files. The
design's fuller form - cells as documents in an Palimpsest world,
changed through the proposal gate and witnessed in the chain
- is the road from here, not a thing this build claims.

## The closing

setup-environment builds the guest environment: one frame
holding every primitive, the evaluator itself as ev and ap,
and *the-env*, the environment naming itself, so a program
can hand its own world to the evaluator it carries.

Closing layer one loads the seed's text into that
environment by evaluating it there. Each definition finds
its name already present - ev finds ev - and replaces the
value in place: the interpreted evaluator takes the
host-loaded one's seat. Layer two hands the seed's text to
the layer-one evaluator, and so on. The guest is then handed
to the evaluator of record as a procedure application
through the host's ap, so the evaluator's body is interpreted
by the host while its logic does the evaluating.

Measured on this machine, factorial five through the Y
combinator's plainer cousin, plain self-recursion:

- under the host seed - 120, fuel 20,583
- closing layer one - 120, fuel 14,020,255
- closing layer two - 120, fuel 14,020,255

The result is the same at every layer. The fuel steps up
once, from the bootstrap to the first interpreted layer, and
then does not move: the evaluator layer two runs is
structurally identical to the one that produced it. The
closing is a fixed point, and the identical fuel reading is
its signature.

A guest can also call the evaluator it carries:
(ev (quote (+ 20 22)) *the-env*) is 42 under the seed at fuel
1,215, and 42 at closing layer one at fuel 230,674,857.
Reflection works at every layer; it costs what
self-interpretation costs.

One law the build learned, now written into the seed: a
definition that adds a name installs the whole new frame in
a single mutation. Installing the names and the values in
two steps leaves the frame misaligned in between, and an
evaluator interpreting that very body looks its operators up
in the frame while it mutates it - the second step then
resolves its operators shifted by one seat. The host never
sees it, because host lookups do not pass through the frame.
The closing does. The seed's define-variable! carries a
comment saying so, where the next reader will meet it.

## Growth

recipes/quine.json grows the crate with fixity's grower: the
four source files pinned by hash, staged, built, and the
binary and the seed sealed into the store with a receipt.
The grown binary is bit-identical to a direct build when
both are built with the build directory remapped to /quine
- RUSTFLAGS set to --remap-path-prefix <build-dir>=/quine,
the same condition the fixity runner carries. Grown on this
host under that condition, the binary hashes to d3d91fc2 and
runs the closing to the same 120 at the same fuel as the
direct build.

## State

L0, L1 and L2 as the design orders them are built and
tested here in their first form: the seed as a document, the
board as a directory of cells, the closing at two layers.
What is not built: the board as an Palimpsest world with
proposals and witnesses, cells that ask the broker for
capabilities, and the seed grown as a Wasm component in the
runner. Those are named in the design as the road, and this
build does not claim them.
