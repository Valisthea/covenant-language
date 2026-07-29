#!/usr/bin/env python3
"""Build the covenant-mcp bundle that Claude Desktop installs.

The first bundle was assembled by hand. It went out as 0.8.3, kept the V0.8
compiler inside it, and stayed there while the language moved three minor
versions, so `scaffold` produced programs that the bundle's own `check_syntax`
then rejected. Nothing connected the two numbers.

This script takes the version from the workspace and refuses to build if the
binary it is about to package disagrees with it, so the bundle cannot claim a
version it does not contain.

Usage, from the repository root:

    cargo build -p covenant-mcp --release
    python crates/covenant-mcp/mcpb/pack.py [--out DIR]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
CRATE = HERE.parent
ROOT = CRATE.parent.parent


def workspace_version() -> str:
    """Read `workspace.package.version` from the root manifest."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"\[workspace\.package\](.*?)(?=\n\[|\Z)", text, re.S)
    if not section:
        sys.exit("no [workspace.package] section in the root Cargo.toml")
    version = re.search(r'(?m)^version\s*=\s*"([^"]+)"', section.group(1))
    if not version:
        sys.exit("no version in [workspace.package]")
    return version.group(1)


def server_binary() -> pathlib.Path:
    """The release binary, whichever host built it."""
    for name in ("covenant-mcp.exe", "covenant-mcp"):
        candidate = ROOT / "target" / "release" / name
        if candidate.exists():
            return candidate
    sys.exit("no release binary; run `cargo build -p covenant-mcp --release` first")


def binary_speaks_for_version(binary: pathlib.Path, version: str) -> None:
    """Refuse to ship a binary that does not carry the version on the label.

    A stale target/ directory is the exact way the 0.8.3 bundle outlived its
    compiler, so this is checked rather than assumed. The version string is
    compiled into the server, so its absence means the binary predates the
    version being packaged.
    """
    blob = binary.read_bytes()
    if version.encode() not in blob:
        sys.exit(
            f"{binary.name} does not contain the string {version!r}.\n"
            f"It is stale. Rebuild with `cargo build -p covenant-mcp --release`."
        )


def scaffold_templates_still_parse() -> None:
    """The check that would have caught the 0.8.3 breakage before shipping."""
    result = subprocess.run(
        ["cargo", "test", "-p", "covenant-mcp", "--test",
         "templates_track_the_compiler", "--quiet"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        sys.exit("scaffold templates do not match this compiler; not packaging")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(CRATE / "dist"),
                        help="directory to write the bundle into")
    parser.add_argument("--skip-checks", action="store_true",
                        help="package without re-running the template tests")
    args = parser.parse_args()

    version = workspace_version()
    binary = server_binary()
    binary_speaks_for_version(binary, version)
    if not args.skip_checks:
        scaffold_templates_still_parse()

    manifest = json.loads((HERE / "manifest.template.json").read_text(encoding="utf-8"))
    # The template deliberately carries no version. It is injected here so the
    # bundle and the workspace cannot drift apart.
    ordered = {}
    for key, value in manifest.items():
        ordered[key] = value
        if key == "display_name":
            ordered["version"] = version
    manifest = ordered

    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    bundle = out_dir / f"covenant-mcp-{version}.mcpb"

    with zipfile.ZipFile(bundle, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
        archive.write(HERE / "icon.png", "icon.png")
        archive.write(binary, f"server/{binary.name}")

    # Read it back rather than trusting what we just wrote.
    with zipfile.ZipFile(bundle) as archive:
        packed = json.loads(archive.read("manifest.json"))
        entries = set(archive.namelist())
    assert packed["version"] == version, "packed manifest disagrees with the workspace"
    assert {"manifest.json", "icon.png"} <= entries, "bundle is missing a required file"
    assert any(e.startswith("server/") for e in entries), "bundle has no server binary"

    print(f"  {bundle}")
    print(f"  version {version}, {bundle.stat().st_size // 1024} KB, "
          f"{len(entries)} entries")


if __name__ == "__main__":
    main()
