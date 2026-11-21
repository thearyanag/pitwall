#!/usr/bin/env python3
"""Offline installer regression tests using local release fixtures (macOS)."""
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="pitwall-install-test-")
        self.base = Path(self.tmp.name)
        self.fixture = self.base / "fixture"
        self.fixture.mkdir()
        self.home = self.base / "install with ' spaces"
        self.bin = self.base / "commands"
        fake = self.base / "fake-tools"
        fake.mkdir()
        self.fake = fake
        (fake / "curl").write_text(
            """#!/bin/sh
out=; url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
case "$url" in
 */releases/latest) printf 'https://github.com/thearyanag/pitwall/releases/tag/%s' "$FIXTURE_VERSION" ;;
 *) cp "$FIXTURE_DIR/${url##*/}" "$out" ;;
esac
"""
        )
        (fake / "curl").chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": str(fake) + os.pathsep + os.environ["PATH"],
            "PITWALL_INSTALL_DIR": str(self.home),
            "PITWALL_BIN_DIR": str(self.bin),
            "FIXTURE_DIR": str(self.fixture),
        }
        for key in ("PITWALL_VERSION", "SPROUT_STATE_DIR", "PITWALL_CONFIG"):
            self.env.pop(key, None)

    def tearDown(self):
        self.tmp.cleanup()

    def release(self, version):
        bundle = self.base / "bundle"
        shutil.rmtree(bundle, ignore_errors=True)
        bundle.mkdir()
        files = {
            "VERSION": version + "\n",
            "sprout": f"#!/bin/sh\nprintf 'Pitwall {version[1:]}\\n'\n",
            "runtimes/node/bin/node": "#!/bin/sh\nprintf 'v22.22.3\\n'\n",
            "runtime.sh": (ROOT / "runtime.sh").read_text(),
            "scripts/pitwall-launcher.sh": (
                ROOT / "scripts/pitwall-launcher.sh"
            ).read_text(),
            "board-runtime/adapter.mjs": "// fixture",
            "worker/listener.py": "# fixture",
            "assets/brand/pitwall-mark-on-dark.png": "fixture",
            "README.md": "release source",
        }
        for name, content in files.items():
            p = bundle / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
            if content.startswith("#!/bin/sh"):
                p.chmod(0o755)
        for arch in ("arm64", "x64"):
            with tarfile.open(
                self.fixture / f"pitwall-darwin-{arch}.tar.gz", "w:gz"
            ) as tar:
                tar.add(bundle, arcname="pitwall")
        self.checksums()
        self.env["FIXTURE_VERSION"] = version

    def checksums(self):
        (self.fixture / "SHA256SUMS").write_text(
            "".join(
                f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n"
                for p in sorted(self.fixture.glob("*.tar.gz"))
            )
        )

    def install(self, succeeds=True):
        r = subprocess.run(
            ["sh", str(ROOT / "install.sh")],
            env=self.env,
            text=True,
            capture_output=True,
        )
        self.assertEqual(r.returncode == 0, succeeds, r.stdout + r.stderr)
        return r

    def test_unsupported_macos_is_rejected(self):
        self.release("v0.2.0")
        tool = self.fake / "sw_vers"
        tool.write_text("#!/bin/sh\necho 13.4\n")
        tool.chmod(0o755)
        self.assertIn("macOS 13.5", self.install(False).stderr)
        self.assertFalse(self.home.exists())

    def test_fresh_install_reinstall_and_upgrade_preserve_data(self):
        self.release("v0.2.0")
        self.install()
        first = (self.home / "current").resolve()
        config = self.home / "config.env"
        config.write_text('export SPROUT_TEAM="test"\n')
        draft = self.home / "state/drafts/task.json"
        draft.parent.mkdir()
        draft.write_text('{"text":"keep this draft"}')
        (first / "README.md").write_text("local worker changes")
        self.install()
        self.assertEqual((first / "README.md").read_text(), "local worker changes")
        self.release("v0.2.1")
        self.install()
        self.assertNotEqual((self.home / "current").resolve(), first)
        self.assertTrue(first.exists())
        self.assertEqual(config.read_text(), 'export SPROUT_TEAM="test"\n')
        self.assertEqual(draft.read_text(), '{"text":"keep this draft"}')
        version = subprocess.check_output(
            [str(self.bin / "pitwall"), "--version"], env=self.env, text=True
        )
        self.assertEqual(version.strip(), "Pitwall 0.2.1")

    def test_checksum_failure_keeps_current_release(self):
        self.release("v0.2.0")
        self.install()
        current = os.readlink(self.home / "current")
        for p in self.fixture.glob("*.tar.gz"):
            p.write_bytes(p.read_bytes() + b"changed")
        self.assertIn("Checksum mismatch", self.install(False).stderr)
        self.assertEqual(os.readlink(self.home / "current"), current)

    def test_unrelated_command_is_not_overwritten(self):
        self.release("v0.2.0")
        self.bin.mkdir()
        command = self.bin / "pitwall"
        command.write_text("not ours")
        self.install(False)
        self.assertEqual(command.read_text(), "not ours")

    def test_unsafe_archive_is_rejected_before_switching(self):
        self.release("v0.2.0")
        self.install()
        current = os.readlink(self.home / "current")
        for p in self.fixture.glob("*.tar.gz"):
            with tarfile.open(p, "w:gz") as tar:
                info = tarfile.TarInfo("pitwall/../../escape")
                info.size = 1
                tar.addfile(info, io.BytesIO(b"x"))
        self.checksums()
        self.install(False)
        self.assertEqual(os.readlink(self.home / "current"), current)
        self.assertFalse((self.base / "escape").exists())


if __name__ == "__main__":
    unittest.main()
