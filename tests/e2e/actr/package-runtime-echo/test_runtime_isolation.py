#!/usr/bin/env python3
"""Exercise the actual OS boundary before building the expensive E2E fixtures."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


LAUNCHER = Path(__file__).parent / "lib/runtime-isolation.py"


class RuntimeIsolationTests(unittest.TestCase):
    def test_runtime_cannot_read_publisher_keys_but_can_use_realm_material(self):
        with tempfile.TemporaryDirectory(prefix="actr-runtime-isolation-") as temporary:
            root = Path(temporary).resolve()
            signing = root / "publisher signing keys"
            signing.mkdir()
            secret = signing / "package.keychain.json"
            secret.write_text("disposable-signing-canary")
            alias = root / "publisher-key-alias"
            alias.symlink_to(secret)
            public = root / "package-public.json"
            public.write_text("public-package-material")
            realm = root / "realm-client.json"
            realm.write_text("runtime-owned-realm-material")
            ca = root / "test-ca.pem"
            ca.write_text("disposable-public-test-ca")
            runtime = root / "runtime"
            runtime.mkdir()
            program = """
import os, pathlib, sys
for name in sys.argv[1:3]:
    try:
        pathlib.Path(name).read_bytes()
    except OSError:
        pass
    else:
        raise SystemExit('runtime read Publisher signing material')
assert pathlib.Path(sys.argv[3]).read_text() == 'public-package-material'
assert pathlib.Path(sys.argv[4]).read_text() == 'runtime-owned-realm-material'
secret = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[5]) / 'stolen-key'
for operation in [lambda: secret.write_text('overwritten'),
                  lambda: secret.rename(destination),
                  lambda: os.link(secret, destination)]:
    try:
        operation()
    except OSError:
        pass
    else:
        raise SystemExit('runtime modified or reparented Publisher signing material')
assert 'ACTR_TEST_PUBLISHER_PRIVATE_KEY' not in os.environ
assert 'ACTR_REALM_E2E_DOCUMENTS' not in os.environ
assert pathlib.Path(os.environ['SSL_CERT_FILE']).read_text() == 'disposable-public-test-ca'
assert pathlib.Path(os.environ['HOME']) == pathlib.Path(sys.argv[5])
(pathlib.Path(sys.argv[5]) / 'runtime-state').write_text('started')
print('Publisher keys denied; public package and Realm key remain usable')
"""
            result = subprocess.run(
                [sys.executable, str(LAUNCHER), "--private-dir", str(signing), "--",
                 sys.executable, "-c", program, str(secret), str(alias), str(public),
                 str(realm), str(runtime)],
                env={**os.environ, "HOME": str(runtime),
                     "ACTR_TEST_PUBLISHER_PRIVATE_KEY": "disposable-environment-canary",
                     "SSL_CERT_FILE": str(ca),
                     "ACTR_REALM_E2E_DOCUMENTS": "builder-only-documents"},
                capture_output=True, text=True, timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertEqual((runtime / "runtime-state").read_text(), "started")
            self.assertEqual(secret.read_text(), "disposable-signing-canary")

    def test_runtime_failure_is_propagated(self):
        with tempfile.TemporaryDirectory() as signing:
            result = subprocess.run(
                [sys.executable, str(LAUNCHER), "--private-dir", signing, "--",
                 sys.executable, "-c", "raise SystemExit(23)"],
                capture_output=True, text=True, timeout=15,
            )
            self.assertEqual(result.returncode, 23, result.stderr)


if __name__ == "__main__":
    unittest.main()
