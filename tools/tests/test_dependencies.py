"""Exercise the dependency boundary command without fetching packages."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

AUDIT = Path(__file__).resolve().parents[1] / "audit_dependencies.py"


class DependencyBoundary(unittest.TestCase):
    """The command rejects forbidden normal dependencies and failed inspection."""

    def audit(self, lines, status=0):
        with tempfile.TemporaryDirectory() as directory:
            cargo = Path(directory) / "cargo"
            cargo.write_text(
                "#!" + sys.executable + "\n"
                "import sys\n"
                "assert '--locked' in sys.argv and '--offline' in sys.argv\n"
                "assert sys.argv[sys.argv.index('--edges') + 1] == 'normal'\n"
                f"print({lines!r})\nraise SystemExit({status})\n"
            )
            cargo.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(AUDIT)],
                env={**os.environ, "PATH": directory},
                capture_output=True, text=True, check=False,
            )

    def test_native_components_and_gpu_renderer_are_allowed(self):
        result = self.audit("lom v0.1.0\nxilem_masonry v0.4.0\nvello v0.8.0\nwgpu v28.0.0")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_display_runners_are_rejected(self):
        for name in ["winit", "masonry_winit", "gtk4", "wayland-client", "x11rb"]:
            with self.subTest(name=name):
                result = self.audit(f"lom v0.1.0\n{name} v1.0.0 (*)")
                self.assertEqual(result.returncode, 1)
                self.assertIn(name, result.stderr)

    def test_software_test_renderers_cannot_become_runtime_dependencies(self):
        for name in ["masonry_testing", "imaging_vello_cpu", "vello_cpu"]:
            with self.subTest(name=name):
                self.assertEqual(self.audit(f"{name} v1.0.0").returncode, 1)

    def test_cargo_failure_cannot_be_reported_as_success(self):
        self.assertEqual(self.audit("", status=17).returncode, 17)


if __name__ == "__main__":
    unittest.main()
