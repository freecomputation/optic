#!/usr/bin/env python3
"""Palimpsest component sync.

Palimpsest is the total system, and its components live in their
own repositories - fixity, coffer, passband, propagule, quine.
This tool mirrors each component's main branch into
components/<name>/ inside the palimpsest repository, so the
total system always carries the latest landed code of its
parts, and records what it carries in
components/MANIFEST.json: the upstream repository and the exact
commit each mirror was taken from.

Run it after any component lands a change, then land the refresh
in palimpsest the same way the component change was landed - branch,
pull request, merge, verify. The mirror is a copy, never an
original: no edit is ever made under components/ by hand. To
change a component, change its own repository and sync again.

A file whose bytes are not valid UTF-8 cannot travel through the
text channels this mirror uses. Such a file is skipped, and the
manifest names it under "excluded" so the omission is a record
and not a silence. Every excluded file so far is regenerable
from sources that are mirrored.

Usage: python3 tools/sync.py [--cache DIR]
The cache holds plain clones of the component repositories and
defaults to ~/.cache/palimpsest-sync; existing clones are pulled,
missing ones are cloned.
"""

import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ORG = "freecomputation"
COMPONENTS = ["fixity", "coffer", "passband", "propagule", "quine"]


def run(cmd, cwd=None):
    out = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"command failed: {' '.join(cmd)}\n{out.stderr.strip()}")
    return out.stdout.strip()


def mirror(name, clone: Path, dest: Path):
    head = run(["git", "rev-parse", "HEAD"], cwd=clone)
    tracked = run(["git", "ls-files"], cwd=clone).splitlines()
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    copied, excluded = [], []
    for rel in tracked:
        src = clone / rel
        data = src.read_bytes()
        try:
            if data.decode("utf-8").encode("utf-8") != data:
                raise ValueError("utf-8 round trip not identical")
        except (UnicodeDecodeError, ValueError):
            excluded.append(rel)
            continue
        out = dest / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
        copied.append(rel)
    print(f"- {name}: {len(copied)} files mirrored from {head[:8]}"
          + (f", {len(excluded)} excluded" if excluded else ""))
    return {"repo": f"{ORG}/{name}", "commit": head,
            "files": len(copied), "excluded": excluded}


def main():
    args = sys.argv[1:]
    cache = Path(os.environ.get(
        "PALIMPSEST_SYNC_CACHE",
        os.environ.get(
            "OPTIC_SYNC_CACHE",
            args[args.index("--cache") + 1] if "--cache" in args
            else Path.home() / ".cache" / "palimpsest-sync")))
    root = Path(__file__).resolve().parent.parent
    comp_root = root / "components"
    comp_root.mkdir(exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)

    manifest = {"format": "palimpsest-components/0",
                "synced_at": datetime.now(timezone.utc)
                .strftime("%Y-%m-%dT%H:%M:%SZ"),
                "components": {}}
    for name in COMPONENTS:
        clone = cache / name
        url = f"https://github.com/{ORG}/{name}.git"
        if clone.exists():
            run(["git", "pull", "--ff-only", "-q"], cwd=clone)
        else:
            run(["git", "clone", "-q", url, str(clone)])
        manifest["components"][name] = mirror(name, clone, comp_root / name)

    (comp_root / "MANIFEST.json").write_text(
        json.dumps(manifest, indent=2) + "\n")
    print(f"manifest: {comp_root / 'MANIFEST.json'}")


if __name__ == "__main__":
    main()
