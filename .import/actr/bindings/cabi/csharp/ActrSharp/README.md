# ActrSharp

C# wrapper over the `actr-cabi` C ABI — **Rust core + .NET UI** (WinUI 3 / WPF /
Console). Targets **.NET 10**.

ActrSharp is a **managed-only** package: it contains the .NET binding over the
`actr_cabi` native library but **not** the native `actr_cabi.dll` itself. The
native library ships in a separate **workload package** (e.g.
`ActrSharp.Native.<Workload>`) published by each workload's repository, which
bundles a Rust workload statically linked against `actr-cabi`.

## Install

ActrSharp is published to the **Actrium GitHub Packages** NuGet feed. Add the
feed, then install the managed package and one workload native package:

```bash
dotnet nuget add source \
  https://nuget.pkg.github.com/Actrium/index.json \
  -n actrium -u <github-username> -p <github-pat-with-read:packages> \
  --store-password-in-clear-text
dotnet add package ActrSharp --version 0.7.0
dotnet add package ActrSharp.Native.<Workload> --version <workload-version>
```

See the [developer guide](../../../../docs/bindings/csharp-actrsharp.md) for PAT
setup, organization SSO, CI `GITHUB_TOKEN` auth, upgrade, and troubleshooting
(401/403, NU1101, cache, DLL, architecture, ABI/version, workload).

## Use

```csharp
using ActrSharp;

ActrCabiNode.InitRuntime();                       // installs the Tokio runtime
                                                    // and verifies ABI + actr version

var info = ActrCabiNode.ValidateNative(
    "cabi.EchoApp",
    "actr.e2e.echo-app"); // confirms the expected workload bundle is linked
Console.WriteLine($"abi={info.AbiVersion} actr={info.ActrVersion} bundle={info.BundleId}");

var node = new ActrCabiNode();
await node.FromConfig("actr.toml");               // Node<Init>
node.WithActorType("actrium:EchoApp:0.1.0");      // sets the AIS actor identity
await node.LinkWorkload("cabi.EchoApp");          // actr_workload_create → Node::link
await node.Register();                            // Node::register (actrix AIS)
await node.Start();                               // Node::start → ActrRef
byte[] resp = await node.Call("cabi.EchoApp.Echo", payload, timeoutMs: 30_000);

node.Shutdown();
await node.WaitShutdown();
ActrCabiNode.ShutdownRuntime();
```

`InitRuntime` fails fast with an `ActrCabiException` carrying `Expected`,
`Actual`, `Rid`, and `Bundle` if the loaded native library has the wrong ABI
version, a mismatched actr version, a missing/incorrect architecture, or no
registered workload — see the troubleshooting section of the developer guide.

## Relationship to actr-swift (UniFFI)

Two parallel binding lines; `actr-cabi` does **not** replace UniFFI:

| | actr-swift (UniFFI) | ActrSharp (cabi) |
|---|---|---|
| Workload | foreign (DynamicWorkload) | Rust (`export_workload!`) |
| UI drives | foreign callback vtable | typestate chain only |
| Platform | Apple | Windows (.NET) |

## License

Apache-2.0.
