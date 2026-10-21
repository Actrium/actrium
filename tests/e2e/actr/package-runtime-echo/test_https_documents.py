#!/usr/bin/env python3
"""Exercise the actual TLS fixture and its restricted CONNECT tunnel."""

import json
import pathlib
import socket
import ssl
import sys
import tempfile
import unittest
import urllib.error
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).parent / "fixtures"))
from https_documents import Gateway


class HttpsDocumentsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="actr-https-e2e-")
        root = pathlib.Path(cls.temporary.name)
        documents = root / "documents.json"
        documents.write_text(json.dumps({
            "https://tenant.realm-e2e.test/.well-known/actr": {
                "authority": "actr://tenant.realm-e2e.test"},
        }))
        package = root / "package.actr"
        package.write_bytes(b"downloaded package bytes")
        artifacts = root / "artifacts.json"
        artifacts.write_text(json.dumps({"https://packages.realm-e2e.test/example.actr": str(package)}))
        cls.gateway = Gateway(root / "tls", documents, artifacts).__enter__()

    @classmethod
    def tearDownClass(cls):
        cls.gateway.__exit__()
        cls.temporary.cleanup()

    def opener(self, trusted=True):
        context = ssl.create_default_context(cafile=self.gateway.config["ca_file"]) if trusted else ssl.create_default_context()
        return urllib.request.build_opener(
            urllib.request.ProxyHandler({"https": self.gateway.config["proxy"]}),
            urllib.request.HTTPSHandler(context=context),
        )

    def test_bootstrap_keeps_hostname_and_port_443(self):
        url = "https://tenant.realm-e2e.test/.well-known/actr"
        with self.opener().open(url, timeout=5) as reply:
            self.assertEqual(json.load(reply)["authority"], "actr://tenant.realm-e2e.test")
        self.assertIn(url, self.gateway.requests)

    def test_package_bytes_are_downloaded_over_tls(self):
        with self.opener().open("https://packages.realm-e2e.test/example.actr", timeout=5) as reply:
            self.assertEqual(reply.headers["Content-Type"], "application/zip")
            self.assertEqual(reply.read(), b"downloaded package bytes")

    def test_untrusted_ca_is_rejected(self):
        with self.assertRaises(urllib.error.URLError) as raised:
            self.opener(trusted=False).open("https://tenant.realm-e2e.test/.well-known/actr", timeout=5)
        self.assertIsInstance(raised.exception.reason, ssl.SSLCertVerificationError)

    def test_wrong_tls_hostname_is_rejected(self):
        context = ssl.create_default_context(cafile=self.gateway.config["ca_file"])
        with socket.create_connection(self.gateway.https.server_address, timeout=5) as connection:
            with self.assertRaises(ssl.SSLCertVerificationError):
                context.wrap_socket(connection, server_hostname="wrong.realm-e2e.test")

    def test_proxy_cannot_reach_arbitrary_destinations(self):
        with socket.create_connection(self.gateway.proxy.server_address, timeout=5) as connection:
            connection.sendall(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n")
            self.assertIn(b"403 Forbidden", connection.recv(1024))

    def test_unpublished_routes_do_not_expose_files(self):
        with self.assertRaises(urllib.error.HTTPError) as raised:
            self.opener().open("https://packages.realm-e2e.test/../server-key.pem", timeout=5)
        self.assertEqual(raised.exception.code, 404)
        raised.exception.close()


if __name__ == "__main__":
    unittest.main()
