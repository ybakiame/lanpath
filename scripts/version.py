#!/usr/bin/env python3
"""Synchronize Lanpath product versions without resolving or updating dependencies."""
import argparse
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
PATTERN = r"0\.[1-9][0-9]*\.(?:0|[1-9][0-9]*)-rc\.[1-9][0-9]*"
PACKAGES = {"lanpath-client", "lanpath-common", "lanpath-engine", "lanpath-mitm", "lanpath-script", "lanpath-desktop-nsh"}


def synchronized(version):
    changes = {}
    for name, section in [("Cargo.toml", "workspace.package"), ("apps/desktop/Cargo.toml", "package")]:
        path = ROOT / name
        content = path.read_text()
        pattern = rf'(\[{re.escape(section)}\]\n(?:[^\[]*?))(^version\s*=\s*)"[^"]+"'
        updated, count = re.subn(pattern, lambda m: m[1] + m[2] + f'"{version}"', content, count=1, flags=re.M)
        if count != 1:
            raise ValueError(f"Missing version in {name}")
        changes[path] = updated
    path = ROOT / "apps/desktop/nsh.json"
    document = json.loads(path.read_text())
    document["version"] = version
    changes[path] = json.dumps(document, ensure_ascii=False, indent=2) + "\n"
    for path in [ROOT / "Cargo.lock", ROOT / "apps/desktop/Cargo.lock"]:
        content = path.read_text()
        for package in PACKAGES:
            pattern = rf'(\[\[package\]\]\nname = "{re.escape(package)}"\nversion = )"[^"]+"'
            content = re.sub(pattern, lambda m: m[1] + f'"{version}"', content)
        changes[path] = content
    return changes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--set", metavar="VERSION", help="set an explicit 0.x.y-rc.N version and synchronize manifests/lock entries")
    parser.add_argument("--check", action="store_true", help="verify consistency without writing files")
    parser.add_argument("--tag", help="also verify the proposed vVERSION Git tag (does not create it)")
    args = parser.parse_args()
    if args.set and args.check:
        parser.error("--set and --check are mutually exclusive")
    canonical = ROOT / "VERSION"
    version = args.set or canonical.read_text().strip()
    if not re.fullmatch(PATTERN, version):
        parser.error("current release series requires 0.x.y-rc.N with no leading zeros")
    # Windows VS_FIXEDFILEINFO has four unsigned 16-bit fields.
    numbers = re.split(r"[.-]", version)
    if any(int(numbers[i]) > 65535 for i in (0, 1, 2, 4)):
        parser.error("Windows version fields must fit unsigned 16-bit values")
    if args.tag and args.tag != f"v{version}":
        parser.error(f"tag must be v{version}")
    changes = synchronized(version)
    changes[canonical] = version + "\n"
    drift = [str(p.relative_to(ROOT)) for p, value in changes.items() if not p.exists() or p.read_text() != value]
    if args.check or not args.set:
        if drift:
            print("Version mismatch: " + ", ".join(drift), file=sys.stderr)
            return 1
    else:
        for path, value in changes.items():
            if not path.exists() or path.read_text() != value:
                path.write_text(value)
    print(version)
    return 0


if __name__ == "__main__":
    sys.exit(main())
