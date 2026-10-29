#!/usr/bin/env python3
"""Test-only HTTPS origins on unprivileged loopback ports.

A restricted CONNECT proxy preserves the real bootstrap URL's hostname and
port 443. Clients explicitly trust the disposable CA; TLS verification stays
enabled. This exercises network transport, not public DNS or public WebPKI.
"""

import argparse
import contextlib
import http.server
import json
import pathlib
import select
import socket
import socketserver
import ssl
import subprocess
import threading


HOSTS = ("localhost", "tenant.realm-e2e.test", "e2e.actrium.test", "packages.realm-e2e.test")


def certificates(directory):
    directory.mkdir(parents=True, exist_ok=True)
    ca = directory / "ca.pem"
    key = directory / "server-key.pem"
    cert = directory / "server.pem"
    extensions = directory / "server.ext"
    extensions.write_text(
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\n"
        "subjectKeyIdentifier=hash\n"
        "authorityKeyIdentifier=keyid,issuer\n"
        "subjectAltName=" + ",".join("DNS:" + host for host in HOSTS) + ",IP:127.0.0.1\n"
    )
    commands = [
        ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-subj", "/CN=Actr disposable E2E CA", "-keyout", str(directory / "ca-key.pem"),
         "-out", str(ca), "-addext", "basicConstraints=critical,CA:TRUE",
         "-addext", "keyUsage=critical,keyCertSign,cRLSign"],
        ["req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
         "-keyout", str(key), "-out", str(directory / "server.csr")],
        ["x509", "-req", "-days", "1", "-in", str(directory / "server.csr"),
         "-CA", str(ca), "-CAkey", str(directory / "ca-key.pem"), "-CAcreateserial",
         "-extfile", str(extensions), "-out", str(cert)],
    ]
    for command in commands:
        subprocess.run(["openssl", *command], check=True, capture_output=True)
    for private in (key, directory / "ca-key.pem"):
        private.chmod(0o600)
    return ca, cert, key


class Gateway:
    def __init__(self, directory, documents, artifacts):
        self.directory = pathlib.Path(directory)
        self.documents = pathlib.Path(documents)
        self.artifacts = pathlib.Path(artifacts)
        self.requests = []
        ca, cert, key = certificates(self.directory)
        gateway = self

        class Documents(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                host = self.headers.get("Host", "")
                if host.endswith(":443"):
                    host = host[:-4]
                if host not in HOSTS or not self.path.startswith("/"):
                    self.send_error(400)
                    return
                url = "https://" + host + self.path
                gateway.requests.append(url)
                documents = json.loads(gateway.documents.read_text())
                artifacts = json.loads(gateway.artifacts.read_text())
                if url in documents:
                    body = json.dumps(documents[url]).encode()
                    content_type = "application/json"
                elif url in artifacts:
                    body = pathlib.Path(artifacts[url]).read_bytes()
                    content_type = "application/zip"
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.https = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Documents)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        self.https.socket = context.wrap_socket(self.https.socket, server_side=True)

        class Tunnel(socketserver.BaseRequestHandler):
            def handle(self):
                with contextlib.suppress(OSError):
                    self.tunnel()

            def tunnel(self):
                self.request.settimeout(10)
                header = b""
                while b"\r\n\r\n" not in header and len(header) < 8192:
                    chunk = self.request.recv(1)
                    if not chunk:
                        return
                    header += chunk
                expected = {b"CONNECT " + h.encode() + b":443 " + version
                            for h in HOSTS for version in (b"HTTP/1.0", b"HTTP/1.1")}
                if header.split(b"\r\n", 1)[0] not in expected:
                    self.request.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                    return
                with socket.create_connection(gateway.https.server_address, timeout=10) as upstream:
                    self.request.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    while True:
                        readable, _, _ = select.select([self.request, upstream], [], [], 30)
                        if not readable:
                            return
                        for source in readable:
                            body = source.recv(65536)
                            if not body:
                                return
                            destination = upstream if source is self.request else self.request
                            destination.sendall(body)

        class Proxy(socketserver.ThreadingTCPServer):
            daemon_threads = True

        self.proxy = Proxy(("127.0.0.1", 0), Tunnel)
        self.config = {"proxy": "http://127.0.0.1:" + str(self.proxy.server_address[1]),
                       "ca_file": str(ca)}

    def __enter__(self):
        self.threads = [threading.Thread(target=s.serve_forever, daemon=True)
                        for s in (self.https, self.proxy)]
        for thread in self.threads:
            thread.start()
        return self

    def __exit__(self, *_):
        for server in (self.proxy, self.https):
            server.shutdown()
            server.server_close()
        for thread in self.threads:
            thread.join(timeout=5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--documents", required=True)
    parser.add_argument("--artifacts", required=True)
    parser.add_argument("--ready", type=pathlib.Path, required=True)
    args = parser.parse_args()
    with Gateway(args.directory, args.documents, args.artifacts) as gateway:
        temporary = args.ready.with_suffix(".new")
        temporary.write_text(json.dumps(gateway.config))
        temporary.replace(args.ready)
        threading.Event().wait()


if __name__ == "__main__":
    with contextlib.suppress(KeyboardInterrupt):
        main()
