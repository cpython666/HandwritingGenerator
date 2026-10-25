import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PyQt6.QtGui import QImage

ROOT = Path(__file__).resolve().parents[1]


class CLITests(unittest.TestCase):
    def run_cli(self, *args, cwd=None, input=None):
        return subprocess.run(
            [sys.executable, str(ROOT / "cli.py"), *args],
            cwd=cwd,
            input=input,
            text=True,
            capture_output=True,
            timeout=30,
        )

    def test_external_directory_stdin_preset_and_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "letter.png"
            preset = Path(directory) / "layout.json"
            result = self.run_cli(
                "render",
                "--text-file",
                "-",
                "--output",
                str(output),
                "--save-preset",
                str(preset),
                cwd=directory,
                input="你好 <世界> & 朋友\n第二行",
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["path"], str(output.resolve()))
            self.assertEqual(
                (QImage(str(output)).width(), QImage(str(output)).height()), (724, 1024)
            )
            initial = output.read_bytes()
            blocked = self.run_cli(
                "render", "--text", "替换", "--output", str(output), cwd=directory
            )
            self.assertNotEqual(blocked.returncode, 0)
            self.assertEqual(output.read_bytes(), initial)
            restored = self.run_cli(
                "render", "--preset", str(preset), "--output", str(output), "--force", cwd=directory
            )
            self.assertEqual(restored.returncode, 0, restored.stderr)
            self.assertEqual(output.read_bytes(), initial)

    def test_errors_and_assets(self):
        assets = self.run_cli("assets")
        self.assertEqual(assets.returncode, 0, assets.stderr)
        self.assertTrue(all(Path(font).is_file() for font in json.loads(assets.stdout)["fonts"]))
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "invalid.png"
            result = self.run_cli(
                "render", "--text", "test", "--width", "-1", "--output", str(output)
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
