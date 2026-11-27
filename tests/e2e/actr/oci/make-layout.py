#!/usr/bin/env python3
"""Build the test Guest's single-image OCI tar and package manifest."""

import argparse
import hashlib
import io
import json
import pathlib
import tarfile


def build(guest: pathlib.Path, dest: pathlib.Path, architecture: str) -> None:
    (dest / "bin").mkdir(parents=True)
    layout = {}
    files = []

    def add_to_tar(archive, path, data):
        member = tarfile.TarInfo(path)
        member.size = len(data)
        member.mode = 0o644
        archive.addfile(member, io.BytesIO(data))

    def declare(path: str, media_type: str, data: bytes, role: str) -> None:
        digest = hashlib.sha256(data).hexdigest()
        (dest / path).write_bytes(data)
        files.append((path, role, media_type, len(data), digest))

    def blob(media_type: str, value: bytes | dict) -> dict:
        data = value if isinstance(value, bytes) else json.dumps(value).encode()
        digest = hashlib.sha256(data).hexdigest()
        layout[f"blobs/sha256/{digest}"] = data
        return {"mediaType": media_type, "digest": f"sha256:{digest}", "size": len(data)}

    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w") as layer:
        runner = layer.gettarinfo(str(guest), "app/echo")
        runner.uid = runner.gid = 0
        runner.uname = runner.gname = "root"
        runner.mode = 0o755
        with guest.open("rb") as executable:
            layer.addfile(runner, executable)
    tar = blob("application/vnd.oci.image.layer.v1.tar", archive.getvalue())
    config = blob("application/vnd.oci.image.config.v1+json", {
        "architecture": architecture,
        "os": "linux",
        "rootfs": {"type": "layers", "diff_ids": [tar["digest"]]},
    })
    image = blob("application/vnd.oci.image.manifest.v1+json", {
        "schemaVersion": 2, "config": config, "layers": [tar],
    })
    layout["oci-layout"] = b'{"imageLayoutVersion":"1.0.0"}'
    layout["index.json"] = json.dumps({
        "schemaVersion": 2, "manifests": [image],
    }).encode()
    image_tar = io.BytesIO()
    with tarfile.open(fileobj=image_tar, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for path, data in sorted(layout.items()):
            add_to_tar(archive, path, data)
    declare("bin/echo.oci.tar", "application/x-tar", image_tar.getvalue(), "workload")
    (dest / "contracts").mkdir()
    proto = pathlib.Path(__file__).with_name("echo.proto")
    declare("contracts/echo.proto", "text/plain", proto.read_bytes(), "contract")

    manifest = f"""format = "actr-package/2"

[actor]
publisher = "local"
name = "OciEcho"
version = "1.0.0"

[workload]
profile = "actr.oci/1"
entry = "bin/echo.oci.tar"
rpc-contract = "contracts/echo.proto"

[workload.oci]
digest = "{image["digest"]}"
platform = "linux/{architecture}"
ipc = "actr.oci-ipc/1"
args = ["/app/echo"]

[host]
profile = "actr.oci-host/1"
api = "1.0.0"
required-capabilities = ["actor.receive"]

[signature]
algorithm = "ed25519"
key-id = "key-id-replaced-at-pack"
"""
    for path, role, media_type, size, digest in files:
        manifest += f"""
[[files]]
path = "{path}"
role = "{role}"
media-type = "{media_type}"
size = {size}
sha256 = "{digest}"
"""
    (dest / "manifest.toml").write_text(manifest)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("guest", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--architecture", choices=("amd64", "arm64"), default="amd64")
    args = parser.parse_args()
    build(args.guest, args.output, args.architecture)
