"""Collect dependency notices from the exact Cargo packages used for the target."""
import json
import os
import re
from pathlib import Path
import subprocess
import sys

target, destination = sys.argv[1:]
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target
]))
with Path(destination).open("w", encoding="utf-8") as out:
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if not package["source"]:
            continue
        manifest = package["manifest_path"]
        if os.name == "posix" and re.match(r"^[A-Za-z]:", manifest):
            manifest = subprocess.check_output(["cygpath", "-u", manifest], text=True).strip()
        root = Path(manifest).parent
        files = {p for p in root.iterdir() if p.is_file() and
                 p.name.lower().startswith(("license", "licence", "copying", "copyright", "notice"))}
        if package["license_file"]:
            files.add(root / package["license_file"])
        if package["name"] == "epaint_default_fonts":
            files.update((root / "fonts").glob("*.txt"))
        print(package["name"], package["version"], package["license"] or "", file=out)
        print(package["repository"] or "", file=out)
        for file in sorted(files):
            print(file.read_text(encoding="utf-8", errors="replace"), file=out)
        print(file=out)
