#!/usr/bin/env python3
"""Exercise real nested bundle signing without reading the user's Keychain."""
from __future__ import annotations

import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest


MACOS = Path(__file__).resolve().parents[2]
SIGN_APP = MACOS / "scripts/sign-app.sh"
BUILD_APP = MACOS / "scripts/build-app.sh"


@unittest.skipUnless(sys.platform == "darwin", "requires macOS codesign")
class SigningTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture_directory = tempfile.TemporaryDirectory(prefix="actrium-signing-fixture-")
        cls.root = Path(cls.fixture_directory.name)
        cls.template = cls.root / "Actrium.app"
        source = cls.root / "fixture.c"
        source.write_text("int main(void) { return 0; }\n", encoding="utf-8")

        def bundle(path: Path, executable: str, identifier: str) -> None:
            (path / "Contents/MacOS").mkdir(parents=True)
            resources = path / "Contents/Resources"
            resources.mkdir()
            (resources / "fixture.txt").write_text("original\n", encoding="utf-8")
            (path / "Contents/Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": identifier,
                "CFBundleExecutable": executable,
                "CFBundlePackageType": "APPL",
                "CFBundleVersion": "1",
            }))
            subprocess.run(["xcrun", "clang", str(source), "-o", str(path / "Contents/MacOS" / executable)], check=True)

        bundle(cls.template, "Actrium", "io.actrium.signing-fixture")
        helper = cls.template / "Contents/Helpers/ActriumRenderer.app"
        bundle(helper, "ActriumRenderer", "io.actrium.signing-fixture.renderer")
        for parent in (cls.template, helper):
            frameworks = parent / "Contents/Frameworks"
            frameworks.mkdir()
            subprocess.run([
                "xcrun", "clang", "-dynamiclib", str(source), "-o", str(frameworks / "libFixture.dylib")
            ], check=True)
            framework = frameworks / "Fixture.framework"
            version = framework / "Versions/A"
            (version / "Resources").mkdir(parents=True)
            (version / "Resources/Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": "io.actrium.fixture",
                "CFBundleExecutable": "Fixture",
                "CFBundlePackageType": "FMWK",
                "CFBundleVersion": "1",
            }))
            subprocess.run(["xcrun", "clang", "-dynamiclib", str(source), "-o", str(version / "Fixture")], check=True)
            (framework / "Versions/Current").symlink_to("A")
            (framework / "Fixture").symlink_to("Versions/Current/Fixture")
            (framework / "Resources").symlink_to("Versions/Current/Resources")

    @classmethod
    def tearDownClass(cls) -> None:
        cls.fixture_directory.cleanup()

    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="actrium signing test ")
        self.addCleanup(self.directory.cleanup)
        self.app = Path(self.directory.name) / "Actrium.app"
        shutil.copytree(self.template, self.app, symlinks=True)
        self.environment = dict(os.environ)
        for name in ("IDENTITY_NAME", "KEYCHAIN_PATH", "CODESIGN_TIMESTAMP", "APP_ENTITLEMENTS", "RENDERER_ENTITLEMENTS"):
            self.environment.pop(name, None)

    def run_signer(self, **environment: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", str(SIGN_APP), str(self.app)],
            env={**self.environment, **environment}, capture_output=True, text=True,
        )

    def sign_fixture(self) -> None:
        # Ad hoc identities are confined to these disposable test bundles.
        result = self.run_signer(IDENTITY_NAME="-", CODESIGN_TIMESTAMP="none")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def verify(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["codesign", "--verify", "--deep", "--strict", str(self.app)], capture_output=True, text=True,
        )

    def test_identity_is_required_before_signing(self) -> None:
        binary = self.app / "Contents/MacOS/Actrium"
        original = binary.read_bytes()
        result = self.run_signer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("signing requires IDENTITY_NAME", result.stderr)
        self.assertEqual(binary.read_bytes(), original)

    def test_interactive_build_rejects_missing_and_ad_hoc_identities(self) -> None:
        for identity in ("", "-"):
            with self.subTest(identity=identity):
                result = subprocess.run(
                    ["bash", str(BUILD_APP)], env={**self.environment, "IDENTITY_NAME": identity},
                    capture_output=True, text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("interactive builds require a fixed IDENTITY_NAME", result.stderr)

    def test_signs_nested_code_and_preserves_each_process_entitlements(self) -> None:
        self.sign_fixture()
        self.assertEqual(self.verify().returncode, 0)
        for relative, sandbox in (("", False), ("Contents/Helpers/ActriumRenderer.app", True)):
            with self.subTest(bundle=relative):
                result = subprocess.run(
                    ["codesign", "-d", "--entitlements", ":-", str(self.app / relative)],
                    capture_output=True, check=True,
                )
                entitlements = plistlib.loads(result.stdout)
                self.assertEqual(entitlements["com.apple.security.app-sandbox"], sandbox)
                self.assertTrue(entitlements["com.apple.security.cs.allow-jit"])

    def test_seals_nested_resources_and_can_resign_an_updated_build(self) -> None:
        self.sign_fixture()
        resource = self.app / "Contents/Helpers/ActriumRenderer.app/Contents/Resources/fixture.txt"
        resource.write_text("updated build\n", encoding="utf-8")
        self.assertNotEqual(self.verify().returncode, 0, "Modified helper resources must invalidate the outer seal")
        self.sign_fixture()
        self.assertEqual(self.verify().returncode, 0)

    def test_nested_signing_failure_is_not_ignored(self) -> None:
        helper = self.app / "Contents/Helpers/ActriumRenderer.app/Contents/MacOS/ActriumRenderer"
        helper.unlink()
        result = self.run_signer(IDENTITY_NAME="-", CODESIGN_TIMESTAMP="none")
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
