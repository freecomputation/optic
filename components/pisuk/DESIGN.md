# Pisuk - design

Pisuk is the walking harness of Free Computation. Its name is the
Inuktitut root pisuk-, "to be walking", and walking is what it
does: it carries a person's computing from machine to machine, so
the machine is the harness and not the hardware in front of them.

## The idea

A program grown on the desktop runs on the laptop, the Mac, and
the old ThinkPad, unchanged, and gives the same result on each -
because the code is executed locally, in the harness, not
internally in whatever operating system happens to own the metal.
The host lends its processor and its screen. Everything above
that line - the programs, the store they live in, the rules they
run under - is carried, and is the same everywhere it walks.

This is what "running FreeBSD on the Mac" means here: the
environment arrives whole and identical, as editions and grown
derivations, rather than being installed per machine and drifting
per machine. The kernel is borrowed from the host in the first
form of Pisuk. A carried system image with a real FreeBSD kernel
inside a virtual machine is a later layer, named in the build
order below, for the workloads that truly need a kernel of their
own.

## The acceptance test

Pisuk inherits idem's law and sharpens it for travel: the same
derivation, from the same store, run on any two hosts, returns
the same result and the same fuel reading. If an answer depends
on which machine it ran on, it is not Pisuk yet. Every part below
exists to make that sentence true.

## The form

Three things are carried, and one thin thing is per-host.

- The store: the grower's content-addressed objects, receipts,
and editions, exactly as idem keeps them. Nothing about the store
changes per host. A store walks on a disk, or syncs the way Umiak
shelves editions - only the chunks a destination lacks.
- The runner: WebAssembly components under a fuel meter and a
memory cap, on an empty linker - idem's runner, which already
runs components the same way everywhere Wasmtime runs.
- The broker: the only door between a carried program and the
host. Capabilities - a directory, the clock, a network socket -
are granted by the broker per program and per host, with the
charter's rule intact: intelligence proposes, the owner disposes,
a warning first and then obedience. What a program may touch is
therefore a decision the owner makes on each machine, while the
program itself never changes.
- The shell: the only native code per platform, kept as small as
the grower is. It starts the runner, seats the broker, and mounts
the store. Mac, Linux, FreeBSD, Windows - the shell differs, the
system it carries does not.

## Why this order

idem's system phase waits on two things that are not code: a
hardware census on the owner's machines, and the FreeBSD bedrock
that follows from it. Pisuk needs neither. It runs on the systems
people already have - the same ruling that put Umiak first - and
when the bedrock arrives, Pisuk is how it will travel: the system
grown once, walked everywhere.

## Build order

- v0 - the walk itself: the shell on Linux hosts, the store
carried by hand on a disk, a derivation grown on one machine and
run on another with identical result and fuel. One guest, the
idem answer component, is the walking proof.
- v1 - the broker's grants: carried programs ask for capabilities
through the broker and receive per-host answers, so useful guests
(files, clock, arguments) become possible without ambient
authority.
- v2 - the carried system: a FreeBSD system image as an edition,
run in a virtual machine under the same broker discipline, for
workloads that need a kernel.

## Open rulings

- First host order after Linux: macOS or Windows - recommended
macOS, because the owner's daily machines include a Mac and the
shell there is the harder fitting to prove early.
- The store's travel: by-hand disks first is recommended, with
edition sync on Umiak's chunk machinery as the standing second
step, not a new mechanism.
- Whether v0's shell is one binary beside idem's three tools or a
fourth crate in the idem tree while Pisuk is young - recommended:
its own repository from the start, since Chase has made one, with
the runner and broker shared by path until they are worth a
library.

Nothing in this document is settled except the name and the
acceptance test, which are Chase's. The rest is the recommended
shape, written to be ruled on.
