import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

const projectDir = join(dirname(fileURLToPath(import.meta.url)), "..");
const pluginPath = join(
  projectDir,
  "scripts",
  "protoc-gen-actrframework-typescript",
);

// RFC 8785-ish canonical JSON: object keys sorted recursively, no whitespace.
function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value && typeof value === "object") {
    const entries = Object.keys(value)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`);
    return `{${entries.join(",")}}`;
  }
  return JSON.stringify(value);
}

function sha256Hex(s) {
  return createHash("sha256").update(s, "utf8").digest("hex");
}

function generate(protoSource, routes) {
  const root = mkdtempSync(join(tmpdir(), "actr-typescript-plugin-"));
  const output = join(root, "generated");
  const protoPath = join(root, "service.proto");
  mkdirSync(output);
  writeFileSync(protoPath, protoSource);

  // Build the DispatchSpec v1 the way the CLI does: hash the canonical JSON
  // of the spec *without* `spec_hash`, then add the hash.
  const spec = { schema_version: "v1", routes };
  const hash = sha256Hex(canonicalJson(spec));
  const specPath = join(root, "dispatch_spec.json");
  writeFileSync(specPath, JSON.stringify({ ...spec, spec_hash: hash }));

  const result = spawnSync(
    "protoc",
    [
      `--proto_path=${root}`,
      `--plugin=protoc-gen-actrframework-typescript=${pluginPath}`,
      `--actrframework-typescript_opt=target=ts,import_extension=js,dispatch_spec=${specPath},dispatch_spec_hash=${hash}`,
      `--actrframework-typescript_out=${output}`,
      protoPath,
    ],
    { encoding: "utf8" },
  );

  return { output, result };
}

test("local services may reuse one request type across RPC methods", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package local;

message SharedRequest {}
message FirstResponse {}
message SecondResponse {}

service LocalService {
  rpc First(SharedRequest) returns (FirstResponse);
  rpc Second(SharedRequest) returns (SecondResponse);
}
`,
    [
      {
        route_key: "local.LocalService.First",
        proto_file: "service.proto",
        proto_service: "local.LocalService",
        method: "First",
        input_type: "local.SharedRequest",
        output_type: "local.FirstResponse",
        payload_type: "rpcReliable",
      },
      {
        route_key: "local.LocalService.Second",
        proto_file: "service.proto",
        proto_service: "local.LocalService",
        method: "Second",
        input_type: "local.SharedRequest",
        output_type: "local.SecondResponse",
        payload_type: "rpcReliable",
      },
    ],
  );

  assert.equal(result.status, 0, result.stderr);
  const dispatcher = readFileSync(join(output, "actr_dispatch.ts"), "utf8");
  assert.match(dispatcher, /fromBinary/);
  assert.match(dispatcher, /SharedRequestSchema/);
  assert.match(dispatcher, /FirstResponseSchema/);
  assert.match(dispatcher, /SecondResponseSchema/);
  assert.match(dispatcher, /from "\.\/service_pb\.js"/);
});

test("remote service metadata carries the dependency alias", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package remote;

message Request {}
message Response {}

service RemoteService {
  rpc Call(Request) returns (Response);
}
`,
    [
      {
        route_key: "remote.RemoteService.Call",
        proto_file: "service.proto",
        proto_service: "remote.RemoteService",
        method: "Call",
        input_type: "remote.Request",
        output_type: "remote.Response",
        payload_type: "rpcSignal",
        dependency_alias: "remote-service",
      },
    ],
  );

  assert.equal(result.status, 0, result.stderr);
  const metadata = JSON.parse(
    readFileSync(join(output, "actr-gen-meta.json"), "utf8"),
  );
  assert.equal(metadata.remote_services[0].dependency_alias, "remote-service");
  assert.equal(metadata.remote_services[0].actr_type, undefined);
  assert.equal(metadata.remote_services[0].client_type, "RemoteServiceClient");
  assert.equal(metadata.schema_version, "v1");
  assert.equal(metadata.language, "typescript");
  assert.deepEqual(metadata.generated_remote_routes, [
    "remote.RemoteService.Call",
  ]);
  assert.ok(metadata.generated_files.includes("actr_dispatch.ts"));

  const dispatcher = readFileSync(join(output, "actr_dispatch.ts"), "utf8");
  assert.match(dispatcher, /resolveDependency\("remote-service"\)/);
  assert.match(dispatcher, /discover\(actrType\)/);
  assert.match(
    dispatcher,
    /callRaw\(target, routeKey, payload, timeoutMs, PayloadType\.RpcSignal\)/,
  );
  assert.doesNotMatch(dispatcher, /remoteTargets|manufacturer|ActrType\s*\{/);
});

test("metadata uses canonical acronym method names and proto paths", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package local;

message Request {}
message Response {}

service LocalService {
  rpc HTTPServer(Request) returns (Response);
}
`,
    [
      {
        route_key: "local.LocalService.HTTPServer",
        proto_file: "service.proto",
        proto_service: "local.LocalService",
        method: "HTTPServer",
        input_type: "local.Request",
        output_type: "local.Response",
        payload_type: "rpcReliable",
      },
    ],
  );

  assert.equal(result.status, 0, result.stderr);
  const metadata = JSON.parse(
    readFileSync(join(output, "actr-gen-meta.json"), "utf8"),
  );
  assert.equal(metadata.local_services[0].proto_file, "service.proto");
  assert.equal(
    metadata.local_services[0].workload_type,
    "LocalServiceWorkload",
  );
  assert.equal(metadata.local_services[0].methods[0].snake_name, "http_server");
  const dispatcher = readFileSync(join(output, "actr_dispatch.ts"), "utf8");
  assert.match(dispatcher, /localServiceHttpServer/);
});

test("duplicate DispatchSpec route keys are rejected", () => {
  const route = {
    route_key: "local.LocalService.Call",
    proto_file: "service.proto",
    proto_service: "local.LocalService",
    method: "Call",
    input_type: "local.Request",
    output_type: "local.Response",
    payload_type: "rpcReliable",
  };
  const { result } = generate(
    `syntax = "proto3";
package local;
message Request {}
message Response {}
service LocalService { rpc Call(Request) returns (Response); }
`,
    [route, route],
  );

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicate route_key/);
});
