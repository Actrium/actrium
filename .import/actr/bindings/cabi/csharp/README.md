# ActrSharp

C# wrapper over the `actr-cabi` C ABI — Rust core + .NET UI (WinUI 3 / WPF / Console).

Parallel binding to `actr-swift` (UniFFI): same "Rust core + native UI" model.
A Rust-authored workload (registered via `export_workload!`) is linked into
`actr_cabi.dll`; the C# app drives only the actr node typestate chain and never
implements a foreign callback.

## Layout

- `ActrSharp.csproj` — .NET 8 library (`AllowUnsafeBlocks`, `LangVersion latest`)
- `ActrCabi.cs` — hand-written wrapper: `LibraryImport` of the C ABI, `SafeHandle`
  for opaque typestate handles, thread-safe `ContinuationMap`, static cdecl
  trampolines, and an `ActrCabiNode` async class.
- `build-actr-cabi-dll.sh` — builds a Rust workload crate (cdylib) into
  `actr_cabi.{dll,dylib,so}` per target (+ `actr-cabi.h`). Mirrors the Swift
  `build-xcframework.sh`.

## Build the native lib

```bash
# A workload crate (impl Workload + export_workload!("cabi.X"), crate-type ["cdylib"])
ACTR_CABI_WORKLOAD_CRATE=path/to/workload \
ACTR_CABI_TARGETS="x86_64-pc-windows-msvc aarch64-apple-darwin" \
  ./build-actr-cabi-dll.sh
# → dist/actr_cabi.dll, dist/libactr_cabi.dylib, dist/actr-cabi.h
```

actr-cabi does not enable actr-hyper's `opentelemetry` feature, so the
Windows build avoids the OTLP compile-time timeout that skipped the actrix
Windows release (#520).

## Use from C#

```xml
<!-- .csproj -->
<ItemGroup>
  <ProjectReference Include="path/to/ActrSharp/ActrSharp.csproj" />
</ItemGroup>
<ItemGroup>
  <None Update="actr_cabi.dll" CopyToOutputDirectory="PreserveNewest" />
</ItemGroup>
<!-- Force-load the static archive so export_workload! inventory entries
     survive dead-stripping (the Swift line uses -Wl,-all_load for this). -->
```

```csharp
using ActrSharp;

ActrCabiNode.InitRuntime();
var node = new ActrCabiNode();
await node.FromConfig("actr.toml");        // Node<Init>
await node.LinkWorkload("cabi.Echo");     // actr_workload_create → Node::link
await node.Register();                     // Node::register (remote actrix AIS)
await node.Start();                        // Node::start → ActrRef
byte[] resp = await node.Call("cabi.Echo", payload, timeoutMs: 30_000);
```

### Async model
Each `extern "C"` async function takes a C callback; the wrapper registers a
`TaskCompletionSource` keyed by an int encoded as `user_data`, and a static
pinned trampoline resumes the awaiting task when the callback fires (same
pattern as the Swift `ContinuationMap` + `@_cdecl` trampolines).

### Force-load inventory (critical)
`export_workload!` entries have no direct reference and are dead-stripped by
default. The Swift line discovered this (`-Wl,-all_load` in project.yml). For
C#/MSBuild, add `/WHOLEARCHIVE:actr_cabi` (Windows) or `-Wl,-all_load`
(macOS) to the native link step — otherwise `actr_workload_create` returns null
("workload not found").

## Relationship to actr-swift (UniFFI)

Two parallel lines; `actr-cabi` does **not** replace UniFFI:

| | actr-swift (UniFFI) | ActrSharp (cabi) |
|---|---|---|
| Workload | C# (foreign DynamicWorkload) | Rust (`export_workload!`) |
| UI drives | foreign callback vtable | typestate chain only |
| Package | NuGet (TODO) | — |

Developers choose: .NET-native workload (UniFFI, TODO) vs Rust core
cross-platform (cabi, this crate).

## Status

- ✅ `ActrCabi.cs` compiles (`dotnet build`, CI verified on Windows)
- ✅ `actr-cabi` compiles on Windows (`cargo build --target x86_64-pc-windows-msvc`)
- ✅ Runtime verified on the Swift line (iOS sim: initRuntime→fromConfig→link→register→start→call, callback fire)
- ⏳ WinUI app + e2e (mirror `swift-cabi-app` in askaway-e2e) — TODO
