import assert from "node:assert/strict";
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
// Repo protocol protos, so fixtures can `import "actr/options.proto"`.
const actrProtoRoot = join(projectDir, "..", "..", "..", "core", "protocol", "proto");

function generate(protoSource, options, extraProtoPaths = []) {
  const root = mkdtempSync(join(tmpdir(), "actr-typescript-plugin-"));
  const output = join(root, "generated");
  const protoPath = join(root, "service.proto");
  mkdirSync(output);
  writeFileSync(protoPath, protoSource);

  const result = spawnSync(
    "protoc",
    [
      `--proto_path=${root}`,
      ...extraProtoPaths.map((p) => `--proto_path=${p}`),
      `--plugin=protoc-gen-actrframework-typescript=${pluginPath}`,
      `--actrframework-typescript_opt=target=ts,${options}`,
      `--actrframework-typescript_out=${output}`,
      protoPath,
    ],
    { encoding: "utf8" },
  );

  return { output, result };
}

// Extract one `export const <Name> = { ... } as const;` companion block.
function companionBlock(code, name) {
  const marker = `export const ${name} = {`;
  const start = code.indexOf(marker);
  assert.notEqual(start, -1, `companion ${name} not found in:\n${code}`);
  const end = code.indexOf("} as const;", start);
  assert.notEqual(end, -1, `companion ${name} not terminated`);
  return code.slice(start, end);
}

test("local services may reuse one request type across RPC methods", () => {
  const { result } = generate(
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
    "LocalFiles=service.proto",
  );

  assert.equal(result.status, 0, result.stderr);
});

test("remote service metadata preserves the complete actor type", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package remote;

message Request {}
message Response {}

service RemoteService {
  rpc Call(Request) returns (Response);
}
`,
    "RemoteFiles=service.proto,RemoteFileMapping=service.proto=acme:RemoteService:1.2.3",
  );

  assert.equal(result.status, 0, result.stderr);
  const metadata = JSON.parse(
    readFileSync(join(output, "actr-gen-meta.json"), "utf8"),
  );
  assert.equal(
    metadata.remote_services[0].actr_type,
    "acme:RemoteService:1.2.3",
  );
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
    "LocalFiles=./service",
  );

  assert.equal(result.status, 0, result.stderr);
  const metadata = JSON.parse(
    readFileSync(join(output, "actr-gen-meta.json"), "utf8"),
  );
  assert.equal(metadata.local_services[0].proto_file, "service.proto");
  assert.equal(metadata.local_services[0].methods[0].snake_name, "http_server");
});

test("companion carries the declared payload_type as a runtime-neutral number", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package remote;
import "actr/options.proto";

message SignalRequest {}
message SignalResponse {}
message PlainRequest {}
message PlainResponse {}

service LaneService {
  rpc SignalEcho(SignalRequest) returns (SignalResponse) {
    option (actr.payload_type) = RPC_SIGNAL;
  }
  rpc PlainEcho(PlainRequest) returns (PlainResponse);
}
`,
    "RemoteFiles=service.proto,RemoteFileMapping=service.proto=acme:LaneService:1.0.0",
    [actrProtoRoot],
  );

  assert.equal(result.status, 0, result.stderr);
  const client = readFileSync(join(output, "service_client.ts"), "utf8");

  const signalBlock = companionBlock(client, "SignalRequest");
  assert.match(signalBlock, /payloadType: 1,/);
  assert.match(signalBlock, /routeKey: "remote\.LaneService\.SignalEcho",/);

  // Unannotated unary method keeps the RPC_RELIABLE fallback.
  const plainBlock = companionBlock(client, "PlainRequest");
  assert.match(plainBlock, /payloadType: 0,/);

  // The lane value must not import any runtime package's PayloadType.
  assert.doesNotMatch(client, /PayloadType/);
});

test("companion generation works when no proto imports actr/options.proto", () => {
  const { output, result } = generate(
    `syntax = "proto3";
package remote;

message Request {}
message Response {}

service RemoteService {
  rpc Call(Request) returns (Response);
}
`,
    "RemoteFiles=service.proto,RemoteFileMapping=service.proto=acme:RemoteService:1.0.0",
  );

  assert.equal(result.status, 0, result.stderr);
  const client = readFileSync(join(output, "service_client.ts"), "utf8");
  assert.match(companionBlock(client, "Request"), /payloadType: 0,/);
});
