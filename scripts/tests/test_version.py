"""Version synchronization preserves dependency pins and independent app identity."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

SOURCE = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("lanpath_version", SOURCE / "scripts/version.py")
version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(version)


class VersionTests(unittest.TestCase):
    def test_repository_versions_match(self):
        result = subprocess.run([sys.executable, str(SOURCE / "scripts/version.py"), "--check"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_sync_preserves_third_party_versions_and_gpui_identity(self):
        original = version.ROOT
        try:
            with tempfile.TemporaryDirectory() as directory:
                version.ROOT = Path(directory)
                for name in ["Cargo.toml", "Cargo.lock", "apps/desktop/Cargo.toml", "apps/desktop/Cargo.lock", "apps/desktop/nsh.json", "apps/gpui/Cargo.lock"]:
                    target = version.ROOT / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes((SOURCE / name).read_bytes())
                changes = version.synchronized("0.1.5-rc.12")
                import tomllib
                for path, content in changes.items():
                    if path.name != "Cargo.lock":
                        continue
                    before = tomllib.loads(path.read_text())["package"]
                    after = tomllib.loads(content)["package"]
                    self.assertEqual(len(before), len(after))
                    for old, new in zip(before, after):
                        if old["name"] in version.PACKAGES:
                            self.assertEqual(new["version"], "0.1.5-rc.12")
                            old = dict(old, version="0.1.5-rc.12")
                        self.assertEqual(old, new)
        finally:
            version.ROOT = original

    def test_invalid_versions_and_wrong_tag_are_rejected_without_writes(self):
        script = str(SOURCE / "scripts/version.py")
        for candidate in ["0.1.4-rc.01", "0.1.4-rc", "1.0.0", "0.1.4-rc.65536"]:
            result = subprocess.run([sys.executable, script, "--set", candidate], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
        result = subprocess.run([sys.executable, script, "--check", "--tag", "v0.0.0-rc.1"], capture_output=True)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
