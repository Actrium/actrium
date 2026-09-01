# framework-codegen-typescript

TypeScript ACTR protoc plugin for Actor-RTC client stubs.

This plugin generates outbound RPC client helpers only. Generate protobuf
message code with `protoc-gen-es`, then run this plugin for remote service
client stubs such as `*_client.ts`.

TypeScript workloads are package-first. Author workload code with
`@actrium/actr-workload` and build components with `actr-workload-ts`. This
plugin does not generate TypeScript local service dispatchers or workload
runtime entrypoints.

## Build

```bash
npm install
npm run build
npm run bundle
```

## Usage

```bash
protoc \
  --plugin=protoc-gen-es=./node_modules/.bin/protoc-gen-es \
  --es_out=generated \
  --es_opt=target=ts \
  remote/echo.proto

protoc \
  --plugin=protoc-gen-actrframework-typescript=./scripts/protoc-gen-actrframework-typescript \
  --actrframework-typescript_out=generated \
  --actrframework-typescript_opt=target=ts,import_extension=js,dispatch_spec=dispatch_spec.json,dispatch_spec_hash=<sha256> \
  remote/echo.proto
```

## Command Line Parameters

Parameters are passed via `--actrframework-typescript_opt`:

| Parameter           | Default | Description                                                        |
| ------------------- | ------- | ------------------------------------------------------------------ |
| `target`            |         | Code generation target (e.g. `ts`).                                |
| `import_extension`  |         | Import path extension for generated imports (e.g. `js`).           |
| `dispatch_spec`     | (required) | Path to the DispatchSpec v1 JSON file.                          |
| `dispatch_spec_hash` | (required) | SHA-256 of the canonical spec JSON, for integrity validation. |

Local/remote routing comes exclusively from the DispatchSpec. The legacy
`LocalFiles`/`RemoteFiles`/`RemoteFileMapping` parameters are no longer accepted.
`actr gen -l typescript` builds the DispatchSpec and computes its hash
automatically; when invoking `protoc` directly, hash the canonical JSON of the
spec (without `spec_hash`) and pass it as `dispatch_spec_hash`.
