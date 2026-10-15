# ActrSharp — .NET developer guide

ActrSharp is the C# wrapper over the `actr-cabi` C ABI: a **Rust core + .NET UI**
model for Windows (WinUI 3 / WPF / Console). It targets **.NET 10** and is
published as a **managed-only** NuGet package to the Actrium GitHub Packages
feed. The native `actr_cabi.dll` ships separately in a **workload package**
owned by each workload's repository.

> Scope: Actrium owns and publishes only `ActrSharp`. Its NuGet package is
> published to GitHub Packages, while the companion symbol package is attached
> to the matching GitHub Release because GitHub Packages does not expose a
> NuGet symbol-package endpoint. Actrium does **not** publish any
> `ActrSharp.Native.<Workload>` package. Workload authors own their native
> packages, versioning, and access. This guide documents the contract for both
> consumers and workload authors, but provides no code-generation templates,
> packaging scripts, or `actr` CLI commands — workload packages are
> hand-authored.

---

## Install

### 1. Add the Actrium NuGet source

ActrSharp lives on GitHub Packages, not nuget.org. Authenticate with a classic
personal access token (PAT) that has the **`read:packages`** scope (a fine-grained
token cannot be used as a NuGet password — GitHub Packages requires a classic
PAT for `dotnet` / `nuget` auth).

```bash
# Per-machine (stored in clear text in the user NuGet.config — use a PAT, not
# your password). Replace <github-username> with your GitHub login.
dotnet nuget add source \
  https://nuget.pkg.github.com/Actrium/index.json \
  -n actrium -u <github-username> -p <github-classic-pat-with-read:packages> \
  --store-password-in-clear-text
```

The PAT must be authorized for the **Actrium organization** via SSO. In the
GitHub UI: your profile → Settings → Developer settings → Personal access
tokens (classic) → the token → Configure SSO → Authorize for **Actrium**. A
401 on restore almost always means SSO was not authorized.

### 2. Install the packages

```bash
dotnet add package ActrSharp --version 0.7.0
dotnet add package ActrSharp.Native.<Workload> --version <workload-version>
```

`ActrSharp` provides the managed binding (`ActrCabiNode`, `ActrNativeInfo`,
`ActrCabiException`). The workload native package provides `actr_cabi.dll`
under `runtimes/win-{x64,arm64}/native/`. NuGet copies the native DLL into
your output directory automatically at restore time — **no manual
`<None CopyToOutputDirectory>` is needed**, and no `LibraryImport` path
configuration is required (`LibraryImport("actr_cabi")` resolves from the
output directory).

### 3. Use it

```csharp
using ActrSharp;

ActrCabiNode.InitRuntime();
ActrNativeInfo info = ActrCabiNode.ValidateNative(
    "cabi.MyWorkload",
    "actr.example.my-workload"); // workload + expected bundle check
Console.WriteLine(info);

var node = new ActrCabiNode();
await node.FromConfig("actr.toml");
node.WithActorType("example:MyWorkload:1.0.0");
await node.LinkWorkload("cabi.MyWorkload");
await node.Register();
await node.Start();
byte[] resp = await node.Call("cabi.MyWorkload.Echo", payload, timeoutMs: 30_000);
node.Shutdown();
await node.WaitShutdown();
ActrCabiNode.ShutdownRuntime();
```

## Outbound Call deadline migration

`ActrCabiNode.Call` keeps its existing signature. An ordinary runtime timeout
has no evidence that the local workload never received or executed the request.
It therefore returns `ActrCabiException.CodeDeliveryUncertain` (`-24`), as does
an explicit uncertain-delivery error from a peer Call. Do not replay either
outcome automatically.

`CodeCallTimedOut` (`-23`) is reserved for an expiry with explicit proof that
nothing was submitted. The current C ABI Call path does not have that proof;
its plain runtime `TimedOut` never produces `-23` or `IsSafeToRetry = true`.
When an uncertain error includes an operation ID, use it for reconciliation;
a local workload timeout may not include one. Application-level idempotency or
a status check is required before deciding whether to issue another operation.

A direct WebSocket session that stops answering liveness probes is retired
within a bounded probe interval (15 seconds to send Ping plus 10 seconds for
Pong); a later Call can establish a replacement route.

---

## Host stream sink & reverse RPC

Streaming responses and host-answered reverse RPC need data that arrives
*after* the one-shot `Call` completes. Two process-global registrations plus
a per-node subscribe cover both directions:

```csharp
// 1) Receive forwarded stream chunks (call once, before ListenStream).
ActrCabiNode.RegisterStreamSink(chunk =>
{
    // chunk.StreamId / chunk.Payload (copied byte[]) / chunk.Sequence
    // chunk.TimestampMs (long? — null when the producer set none)
    // chunk.SenderId (sending actor's string repr)
    // chunk.Metadata (copied key/value entries)
    SendStreamEventToRn(chunk); // your RN bridge / UI dispatch
});

// 2) Subscribe this started node to a stream id (e.g. result_stream_id
//    from an earlier RPC reply).
await node.ListenStream(streamId);
// ... later:
await node.UnlistenStream(streamId);

// 3) Host-answered reverse RPC (e.g. AskUserQuestion).
ActrCabiNode.RegisterReverseRpcHandler(async request =>
{
    var answer = await AskUserInUiAsync(request.Route, request.Payload);
    return answer; // returned bytes resolve the Rust caller's ask_host
});
```

Semantics and sharp edges:

- **Opt-in, exact-id streams only**: `ListenStream("id")` registers the
  runtime callback for exactly `id`. This bridge does not forward ordinary
  `Call`, `Tell`, or unrelated data-stream routes, and an id that was never
  listened is never delivered to ActrSharp.
- **Ordering and bounded overflow**: accepted chunks are dispatched to your
  handler serially in arrival order on a dedicated consumer task, never on an
  actr Tokio worker. The managed queue is bounded at
  `ActrCabiNode.StreamQueueCapacity` (currently 256). When full, ActrSharp
  drops the newest copied chunk and increments
  `ActrCabiNode.DroppedStreamChunkCount`. Therefore this host bridge is
  best-effort after the native callback: a reliable native stream does **not**
  become an end-to-end managed delivery acknowledgement. Keep handlers fast,
  observe the counter, or use an application-level acknowledgement/retry
  protocol when every chunk matters.
- **Registration is process-global**: the native sink replaces any previous
  one; passing `null` unregisters. Native clear/replace waits for a currently
  executing native callback before returning. On clear, ActrSharp invalidates
  queued managed dispatch; on replacement, queued chunks from the old
  registration are not delivered to the new handler. A managed handler that
  is already running (or races the clear) is allowed to finish. Chunks
  arriving with no handler registered are dropped (not buffered).
- **Listener lifecycle**: `UnlistenStream(id)` removes that exact runtime
  listener. The runtime may finish callbacks for chunks it accepted before
  removal; it never forwards a newly received chunk from an unregistered id.
- **Reverse RPC deadlines**: `ask_host` (the Rust-side call your workload's
  dispatcher makes) carries its own timeout — answer within it or the Rust
  caller resolves with `TimedOut`. Cancelling the Rust caller removes its
  pending request immediately. A handler that throws or cannot copy its
  callback buffers answers with a *generic error* instead.
  This process-local reverse-RPC timeout is separate from the outbound
  `ActrCabiNode.Call` deadline behavior above and retains its existing
  `TimedOut` result.
  `ActrCabiException.CodeRequestIdUnknown` (`-22`) means a respond referenced
  an unknown, already-answered, cancelled, or timed-out request id.
- **Fidelity and delegate lifetime**: `StreamId`, payload, sequence,
  timestamp annotation, sender id, and every `DataChunk.metadata` key/value
  entry are copied before the native callback returns. ActrSharp roots its
  native trampolines statically and holds your active handler reference — you
  do not need to keep delegates alive to prevent GC collection.

Workload authors: the Rust side is `actr_cabi::ask_host(route, payload,
timeout_ms)` — call it only from a dispatcher route that explicitly delegates
to the host. It is not automatic forwarding for ordinary RPC/tell routes.
`actr_stream_listen` is the supported opt-in stream path; workloads that call
`actr_cabi::forward_chunk_to_host(&chunk, &sender)` themselves take ownership
of the same best-effort sink contract.

---

## Authentication reference

| Consumer | Identity | Notes |
|---|---|---|
| Local dev | classic PAT (`read:packages`) + Actrium SSO | Stored in user `NuGet.config`. Fine-grained tokens do **not** work as NuGet passwords. |
| GitHub Actions (repository granted package access) | `GITHUB_TOKEN` | Add the consuming repository under the package's **Manage Actions access** settings, then grant the job `packages: read`. |
| GitHub Actions (other org/fork) | classic PAT (`read:packages`) + SSO | Fork PRs cannot read Actrium packages with `GITHUB_TOKEN`. |

### GitHub Actions restore

```yaml
# Restore ActrSharp from GitHub Packages using the workflow GITHUB_TOKEN.
- name: Setup .NET
  uses: actions/setup-dotnet@v4
  with:
    dotnet-version: "10.0.x"
- name: Configure Actrium feed
  run: |
    dotnet nuget add source https://nuget.pkg.github.com/Actrium/index.json \
      -n actrium -u ${{ github.actor }} -p ${{ secrets.GITHUB_TOKEN }} \
      --store-password-in-clear-text
- run: dotnet restore
```

The `GITHUB_TOKEN` needs `packages: read` (default `contents: read` is not
enough — add `packages: read` to the job `permissions:`). For a workflow in a
different repository, an ActrSharp package administrator must first add that
repository under **Package settings → Manage Actions access**. Otherwise use a
classic PAT with `read:packages` and Actrium SSO authorization.

---

## Upgrade

Bump both packages together. `ActrSharp` and the workload native package are
versioned independently (see the [workload contract](#workload-package-author-contract)),
but a workload package declares an **exact** `ActrSharp [X.Y.Z]` dependency, so
NuGet will reject a mismatched ActrSharp version at restore time. To upgrade:

1. Check the workload package's `ActrSharp` dependency for the ActrSharp version
   it was built against.
2. Update `ActrSharp` to that version (or to a newer one if the workload package
   was republished against it).
3. `dotnet restore` — a `NU1102`/`NU1103` (version not found) means the workload
   package was not republished for the ActrSharp version you selected.

The ActrSharp version always equals the actr Cargo workspace version
(e.g. ActrSharp `0.7.0` ↔ actr `0.7.0`). `InitRuntime` verifies at runtime that
the loaded `actr_cabi` reports the same actr version; a mismatch throws an
`ActrCabiException` with `Expected`/`Actual`/`Rid`/`Bundle` context.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| **401 Unauthorized** on restore | PAT missing `read:packages`, or SSO not authorized for Actrium | Recreate the PAT with `read:packages`; authorize SSO for Actrium in the token's SSO settings. |
| **403 Forbidden** on restore | PAT authorized for a different org, or the package is internal and you are not an Actrium member | Request Actrium membership, or use a PAT whose owner is an Actrium member with SSO authorized. |
| **NU1101 / NU1102** (package not found) | Actrium source not configured, or source name typo | `dotnet nuget list source` must show the Actrium feed; the URL must be `https://nuget.pkg.github.com/Actrium/index.json`. |
| **NU1103** (version not found) | Workload package not republished for your ActrSharp version | Use the ActrSharp version the workload package declares, or wait for the workload author to republish. |
| **Stale package in cache** | NuGet HTTP cache holds an old version | `dotnet nuget locals all --clear`, or delete `~/.nuget/packages/actrsharp*`. |
| **`ActrCabiException` (-40) `CodeDllNotFound`** (wraps `DllNotFoundException`) | No workload native package installed, or the DLL is not on the load path | Install `ActrSharp.Native.<Workload>`; ensure your app targets `win-x64` or `win-arm64` (see [DLL / architecture](#dll--architecture)). |
| **`ActrCabiException` (-41) `CodeArchMismatch`** (wraps `BadImageFormatException`) | 32-bit process loading a 64-bit DLL, or x64/arm64 mismatch | Build the app as `win-x64`/`win-arm64` (set `<RuntimeIdentifier>` or pass `--runtime`). |
| **`ActrCabiException` (-42) `CodeEntryMissing`** (wraps `EntryPointNotFoundException`) | Loaded an older `actr_cabi` missing an entry point ActrSharp expects | The workload native package is older than ActrSharp; upgrade it. |
| **ABI version mismatch** (`-30`) | `actr_cabi` ABI is incompatible with ActrSharp | The native library and ActrSharp are from incompatible generations; align them. |
| **Actr version mismatch** (`-31`) | `actr_cabi` built from a different actr version than ActrSharp | Use a workload package built against the matching actr version. |
| **Bundle conflict** (`-33`) | Two workload cdylibs linked into one process | Only one `actr_cabi` provider per process; statically combine workloads into one cdylib. |
| **Reverse-RPC unknown request** (`-22`) | `actr_host_respond_reverse_rpc` got an unknown, already-answered, or timed-out request id | Expected for late/duplicate answers; the Rust caller already resolved — no action needed. |
| **Workload not found** (`-20`) | The workload name is not registered in the loaded bundle | Check the `export_workload!` name; ensure the cdylib was linked with `/WHOLEARCHIVE` (Windows) so inventory survives dead-stripping. |

### DLL / architecture

`actr_cabi.dll` is architecture-specific. NuGet resolves
`runtimes/<RID>/native/actr_cabi.dll` based on the restore RID. Set the RID
explicitly so the right DLL is copied:

```xml
<!-- .csproj -->
<PropertyGroup>
  <TargetFramework>net10.0</TargetFramework>
  <RuntimeIdentifier>win-x64</RuntimeIdentifier>
</PropertyGroup>
```

or `dotnet publish -r win-x64`. Do not leave the app as `AnyCPU` without a RID
on a 64-bit host expecting the native DLL — NuGet copies the RID-matched asset,
and `LibraryImport` loads it from the output directory.

### ABI / version

`InitRuntime` queries `actr_cabi_abi_version` and `actr_cabi_actr_version` and
compares them against ActrSharp's compiled-in expectations. On mismatch it
throws `ActrCabiException` with `Expected`, `Actual`, `Rid`, and `Bundle`
fields (also surfaced in the exception `Message`). `ValidateNative` checks the
workload registration and, when given an expected bundle ID, rejects an
unexpected native package. Call `ValidateNative` once at startup to fail fast
with an actionable error instead of an opaque callback failure later. Call
`WithActorType` after `FromConfig` and before `LinkWorkload` when the linked
workload must register under a concrete actor identity.

---

## Workload package author contract

A workload native package bundles a Rust workload (registered via
`export_workload!`) statically linked against `actr-cabi`, producing
`actr_cabi.dll` for each supported Windows architecture. Actrium does **not**
provide a template, packaging script, or `actr` CLI command for this — the
package is hand-authored by the workload's repository. The contract:

1. **Package id**: `ActrSharp.Native.<Workload>` (e.g. `ActrSharp.Native.EchoApp`).
   Actrium never publishes a package with this id prefix.

2. **Versioning**: independent SemVer, owned by the workload repository. It is
   **not** tied to the actr/ActrSharp version. Bump it when the workload changes
   or when it is rebuilt against a new ActrSharp.

3. **Exact ActrSharp dependency**: declare
   `<PackageReference Include="ActrSharp" Version="[X.Y.Z]" />` (square brackets
   = exact). The workload package must pin the ActrSharp version its
   `actr_cabi.dll` was built against, so consumers cannot accidentally mix
   incompatible managed/native versions.

4. **Native assets**: place the DLL under
   `runtimes/win-x64/native/actr_cabi.dll` and
   `runtimes/win-arm64/native/actr_cabi.dll`. NuGet copies the RID-matched DLL
   into the consumer output at restore time. The cdylib must export the
   `actr_cabi_*` C surface (it does, because it links `actr-cabi`).

5. **Single provider**: a process may load **one** `actr_cabi.dll`. Multiple
   workloads must be statically combined into a single cdylib (one
   `export_bundle!`, multiple `export_workload!` submissions). Linking two
   workload cdylibs into one app produces a bundle conflict (`-33`).

6. **Bundle id**: declare `actr_cabi::export_bundle!("com.yourorg.yourworkload")`
   in the workload crate so hosts can identify the loaded provider via
   `ActrNativeInfo.BundleId`.

7. **Symbols (optional)**: if you ship a symbols package, its version **must
   equal** the workload package version, and it should contain the Windows
   MSVC PDB for the workload's native DLL. Publish it as a regular optional
   NuGet package rather than treating it as a portable managed PDB.

8. **Linker**: build the workload cdylib with `/WHOLEARCHIVE:actr_cabi` (Windows
   MSVC) or `-Wl,-all_load` (macOS) so `export_workload!` / `export_bundle!`
   inventory entries survive dead-stripping. Without this,
   `actr_workload_create` returns null ("workload not found"). The
   `build-actr-cabi-dll.sh` helper in this repo shows the build invocation.

### Manual csproj example

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <PackageId>ActrSharp.Native.EchoApp</PackageId>
    <Version>1.2.0</Version>                       <!-- independent SemVer -->
    <Description>EchoApp workload native package for ActrSharp.</Description>
    <PackageReadmeFile>README.md</PackageReadmeFile>
    <IsPackable>true</IsPackable>
  </PropertyGroup>
  <ItemGroup>
    <!-- Exact dependency on the ActrSharp version this DLL was built against. -->
    <PackageReference Include="ActrSharp" Version="[0.7.0]" />
  </ItemGroup>
  <ItemGroup>
    <None Include="runtimes\**\*" Pack="true" PackagePath="runtimes" />
    <None Include="README.md" Pack="true" PackagePath="\" />
  </ItemGroup>
</Project>
```

The DLLs are produced by building the workload crate (cdylib) and renaming the
output to `actr_cabi.dll` per RID — see
[`packages/actr-cabi/csharp/build-actr-cabi-dll.sh`](../../../packages/actr-cabi/csharp/build-actr-cabi-dll.sh).

### Manual .nuspec alternative

If you package without an SDK project (e.g. from a build script), the nuspec
must declare the exact dependency and native files explicitly:

```xml
<package xmlns="http://schemas.microsoft.com/packaging/2013/05/nuspec.xsd">
  <metadata>
    <id>ActrSharp.Native.EchoApp</id>
    <version>1.2.0</version>
    <description>EchoApp workload native package for ActrSharp.</description>
    <dependencies>
      <dependency id="ActrSharp" version="[0.7.0]" />
    </dependencies>
  </metadata>
  <files>
    <file src="runtimes\win-x64\native\actr_cabi.dll" target="runtimes\win-x64\native\" />
    <file src="runtimes\win-arm64\native\actr_cabi.dll" target="runtimes\win-arm64\native\" />
  </files>
</package>
```

Publishing a workload package to the Actrium feed (or your own feed) is the
workload author's responsibility; Actrium does not publish or mirror workload
packages.

---

## Relationship to actr-swift (UniFFI)

Two parallel binding lines; `actr-cabi` does **not** replace UniFFI:

| | actr-swift (UniFFI) | ActrSharp (cabi) |
|---|---|---|
| Workload | foreign (DynamicWorkload) | Rust (`export_workload!`) |
| UI drives | foreign callback vtable | typestate chain only |
| Platform | Apple | Windows (.NET) |

Developers choose: a .NET-native workload (UniFFI, not yet available for C#) vs
a Rust core cross-platform workload (cabi, this package).

## License

Apache-2.0.
