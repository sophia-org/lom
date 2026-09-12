#!/usr/bin/env python3
"""Keep conventional display runners and test renderers out of Lom's normal graph."""

import subprocess
import sys
from pathlib import Path

FORBIDDEN = {
    "gtk", "gtk4", "gdk4", "winit", "masonry_winit", "wayland-client", "x11rb",
    "masonry_testing", "imaging_vello_cpu", "vello_cpu",
}


def main():
    """Inspect the resolved normal-dependency graph; tooling errors fail the gate."""
    root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["cargo", "tree", "--locked", "--offline", "--edges", "normal", "--prefix", "none", "--format", "{p}"],
        cwd=root, capture_output=True, text=True, check=False,
    )
    if result.returncode:
        sys.stderr.write(result.stderr)
        return result.returncode
    names = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
    forbidden = sorted(names & FORBIDDEN)
    if forbidden:
        print("Forbidden production dependencies: " + ", ".join(forbidden), file=sys.stderr)
        return 1
    print("Production dependency boundary passed (no display runner or CPU test renderer).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
