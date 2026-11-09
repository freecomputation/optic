# Pisuk

Pisuk is the walking harness of Free Computation. Its name is the
Inuktitut root pisuk-, "to be walking", and walking is what it
does: it carries a person's computing from machine to machine, so
the machine is the harness and not the hardware in front of them.

The design is in DESIGN.md. In one paragraph: three of idem's
tools - the grower, the broker, the runner - and a
content-addressed store are carried together as a kit, and a thin
native shell per host walks them. The host lends its processor.
The code is executed locally, in the harness, not internally in
whatever operating system happens to own the metal, so the same
derivation from the same store gives the same result on the
desktop, the laptop, and the old ThinkPad.

## The shell

"pisuk seal --kit <dir>" writes the kit manifest, pisuk.json,
pinning grower, broker, and runner by sha256. "pisuk verify
--kit <dir>" checks a kit against its manifest. "pisuk run
--kit <dir> --store <dir> --object <sha256>" verifies the kit,
then runs the named component out of the carried store through
the verified runner, under the runner's fuel meter. A kit whose
tool does not answer to the manifest is refused before anything
runs: the harness will not walk with a tool that is not itself.

The shell does not pin itself in its own manifest - a manifest
cannot carry its own hash honestly. The shell's integrity is the
store's business, the same way the grower's self-growth is proven
by recipe rather than by assertion.

## The walk proof, v0

The kit sealed for the first proof carried the three tools at
their idem acceptance hashes - grower fbd7b6e8, broker 27d19a11,
runner bdc57475. The answer component in guests/ was grown into a
store with the kit's own grower. Run from the kit's first home:
result 42, fuel used 2. The whole kit and store were then copied
to a different path and run again: result 42, fuel used 2 -
identical, as the acceptance test demands. One byte flipped in
the copied runner and the shell refused, naming both hashes.

That is two paths on one machine, not yet two machines: the
honest remainder of v0 is the same walk between the owner's real
hosts, which needs his machines. Nothing in the mechanism is
path-shaped, so nothing in it should change when the path becomes
a different computer.

## License

AGPL-3.0.
