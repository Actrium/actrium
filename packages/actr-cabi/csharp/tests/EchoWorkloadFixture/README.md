# ActrSharp.Native.EchoWorkload (TEST-ONLY)

**This package is a CI-only fixture. It is never published to any NuGet feed.**

It bundles the `actr_cabi.dll` built from [`tests/e2e/actr/winui-echo-app/workload`](../../../../../tests/e2e/actr/winui-echo-app/workload).
The blocking PR gate validates `win-x64`; the scheduled/manual full-matrix run
also validates `win-arm64`. Together they cover:

- NuGet RID asset resolution (`runtimes/win-{x64,arm64}/native/actr_cabi.dll`),
- native DLL auto-copy into a consumer's output directory (no manual copy),
- `ActrCabiNode.InitRuntime` + `ValidateNative` against a real bundled workload,
- real Rust → C ABI → ActrSharp host-bridge coverage on win-x64: an explicit
  fixture listener receives a native `DataChunk` (stream ID, payload, metadata,
  sequence, timestamp, and sender), while `ask_host` receives and validates a
  C# reverse-RPC response,
- the strict `ActrSharp [X.Y.Z]` dependency a real workload package declares,
- PE architecture of the bundled DLLs.

A real workload native package is owned by the workload's own repository — see
the [workload package author contract](../../../../../docs/bindings/csharp-actrsharp.md#workload-package-author-contract).
This fixture only mirrors that shape for CI consumption validation.

## Constants

| | |
|---|---|
| Workload name | `cabi.EchoApp` (`export_workload!`) |
| Bundle id | `actr.e2e.echo-app` (`export_bundle!`) |
| Host-bridge stream | `actrsharp.fixture.host-bridge.v1` |

## Never publish

`ci-csharp.yml` includes a structural check that asserts no `dotnet nuget push`
path in the repository references this package, and the publish artifact set
contains only `ActrSharp.*nupkg`/`*.snupkg`. The fixture is packed into a
temporary local feed that lives only for the CI run.
