# WinUI EchoApp E2E

Windows mirror of [`e2e/swift-echo-app`](../swift-echo-app/), built on the
**actr-cabi** C ABI line instead of the Swift UniFFI line.

## Architecture

The Swift EchoApp implements the local echo handler *in Swift*
(`LocalEchoServiceHandlerImpl` discovers the remote `EchoService` and forwards
each call). The actr-cabi line cannot implement a workload in C# — the C ABI
only exposes `Dest::Workload` (local) calls, with no `Dest::Peer` call path —
so the discover+forward logic lives in a **Rust workload** that the C# host
links by name:

```
WinUI app (C#)  ──Call("cabi.EchoApp.Echo", EchoRequest bytes)──▶  Rust workload
  (ActrCabiNode)        Dest::Workload (local)                    (cabi.EchoApp)
                                                                       │
                                                       discover + call_raw
                                                                       ▼
                                                          remote actrium:EchoService:1.0.0
```

- `workload/` — Rust cdylib, `export_workload!("cabi.EchoApp")`. Its dispatcher
  discovers `actrium:EchoService:1.0.0` and forwards raw bytes to
  `echo.EchoService.Echo`. Pure byte forwarder (no protobuf types).
- `WinUiEchoApp/` — C# WinUI 3 app. `ActrService.cs` drives the node typestate
  chain (`InitRuntime → FromConfig → LinkWorkload → Register → Start → Call`),
  encodes the `EchoRequest` protobuf (with a `winui-local:` prefix mirroring
  Swift's `swift-local:`), and decodes the `EchoResponse` reply. `EchoProto.cs`
  is a hand-rolled protobuf codec (no Google.Protobuf dependency).

## Status — UNVERIFIED

The CI gate builds this fixture on Windows. Running the complete E2E flow
requires a Windows host with the .NET 8 SDK, Windows SDK, and the Rust
`x86_64-pc-windows-msvc` target:

```bash
rustup target add x86_64-pc-windows-msvc
bash e2e/winui-echo-app/run.sh
```

Known risks / assumptions (see also the commit message):

- **build-actr-cabi-dll.sh target dir.** The script expects the cdylib under
  `${WORKSPACE_ROOT}/target/<target>/<profile>`. `run.sh` exports
  `CARGO_TARGET_DIR="$REPO_ROOT/target"` so the standalone workload crate
  (its own `[workspace]`) builds into that location. If the dll is not found,
  check the script's `lib_dir` against the actual build output.
- **`export_workload!` dead-stripping.** If `actr_workload_create("cabi.EchoApp")`
  returns null at runtime, the linker stripped the inventory entry. The
  README at `bindings/cabi/csharp/README.md` documents the
  `/WHOLEARCHIVE:actr_cabi` (MSBuild) / `-Wl,-all_load` (macOS) fix; the
  cdylib build may need it.
- **WinUI exe output path.** `run.sh` assumes
  `bin/Debug/net8.0-windows10.0.19041.0/win-x64/WinUiEchoApp.exe`; a different
  SDK / self-contained setting will move it.
- **Console.Error in a GUI app.** The E2E marker `ACTR_E2E_RESULT:` is written
  to stderr via `Console.Error`; run.sh launches the exe with stderr
  redirected to a log. If the GUI subsystem has no stderr handle, the marker
  won't appear — a direct `GetStdHandle`/`WriteFile` fallback may be needed.
- **ConnectionNotReady detection.** The C ABI surfaces all call errors as a
  generic `-50` `ActrCabiException`; `ActrService` matches the
  `"connection not ready"` message prefix to decide retries (fragile).

## Files

| Path | Purpose |
|------|---------|
| `workload/Cargo.toml`, `workload/src/lib.rs` | Rust `cabi.EchoApp` forwarding workload |
| `WinUiEchoApp/ActrService.cs` | Node typestate chain + echo + retry |
| `WinUiEchoApp/EchoProto.cs` | Manual protobuf codec for EchoRequest/EchoResponse |
| `WinUiEchoApp/MainWindow.xaml`(+`.cs`) | UI: input + Send + reply |
| `WinUiEchoApp/App.xaml`(+`.cs`) | WinUI entry point |
| `WinUiEchoApp/WinUiEchoApp.csproj` | net8.0-windows, Windows App SDK, unpackaged |
| `manifest.toml`, `actr.toml.tpl`, `actr.lock.toml` | Package + runtime config (mirror swift-echo-app) |
| `run.sh` | E2E driver (build → start actrix → service → app → verify) |
