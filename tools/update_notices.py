"""Regenerate the offline notices for the locked Windows dependency graph."""

import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
OVERRIDES = {
    "accesskit": "accesskit-MIT.txt",
    "clipboard-win": "clipboard-win-BSL.txt",
    "gl_generator": "gl-rs-APACHE.txt",
    "khronos_api": "gl-rs-APACHE.txt",
    "profiling": "profiling-MIT.txt",
}
EGUI = {"ecolor", "eframe", "egui", "egui-winit", "egui_glow", "emath", "epaint", "epaint_default_fonts"}


def generate():
    result = subprocess.run(
        ["cargo", "metadata", "--offline", "--locked", "--format-version", "1",
         "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=ROOT, check=True, capture_output=True, encoding="utf-8",
    )
    metadata = json.loads(result.stdout)
    root = metadata["resolve"]["root"]
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = [root]
    included = set()
    while pending:
        key = pending.pop()
        if key in included:
            continue
        included.add(key)
        pending.extend(dep["pkg"] for dep in nodes[key]["deps"]
                       if any(kind["kind"] != "dev" for kind in dep["dep_kinds"]))
    packages = sorted(
        (p for p in metadata["packages"] if p["id"] in included and p["id"] != root),
        key=lambda p: (p["name"], p["version"]),
    )
    lines = ["SLX Viewer third-party notices", "",
             "Generated from Cargo.lock for the Windows x64 runtime and build dependency graph.",
             "Includes bundled font notices. Dependency authors retain their respective rights.", ""]
    for package in packages:
        folder = Path(package["manifest_path"]).parent
        files = sorted(p for p in folder.rglob("*") if p.is_file() and (
            p.name.lower().startswith(("license", "copying", "notice", "unlicense", "copyright"))
            or p.name in ("OFL.txt", "UFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt")
        ))
        override = "egui-MIT.txt" if package["name"] in EGUI else OVERRIDES.get(package["name"])
        if override:
            files.insert(0, ROOT / "notices" / override)
        if not files:
            raise RuntimeError(f"No license text found for {package['name']}")
        lines.extend(["=" * 72, f"{package['name']} {package['version']}",
                      f"License expression: {package['license']}",
                      f"Source: {package['repository'] or package['homepage'] or 'Cargo registry'}",
                      f"Authors: {', '.join(package['authors'])}", ""])
        for file in files:
            lines.extend([f"--- {file.name} ---", file.read_text(encoding="utf-8-sig").rstrip(), ""])
        if package["name"] in ("gl_generator", "khronos_api"):
            lines.extend(["Copyright 2015 Brendan Zabarauskas and the gl-rs developers.", ""])
    (ROOT / "THIRD_PARTY_NOTICES.txt").write_text("\n".join(lines), encoding="utf-8")
    print(f"Bundled notices for {len(packages)} packages.")


if __name__ == "__main__":
    generate()
