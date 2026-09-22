using ActrSharp;

namespace WinUiEchoApp;

// Mirrors tests/e2e/actr/swift-echo-app/EchoApp/ActrService.swift's linked-runtime flow,
// adapted to the actr-cabi C ABI typestate chain.
//
// Key difference from the Swift app: Swift implements the local EchoService
// handler in-language (LocalEchoServiceHandlerImpl discovers + forwards to the
// remote EchoService). The actr-cabi line cannot implement a workload in C#,
// so the C# host drives only the node typestate chain and calls the LOCAL
// linked Rust workload ("cabi.EchoApp"), which forwards to the remote
// EchoService. See tests/e2e/actr/winui-echo-app/workload/src/lib.rs.
internal sealed class ActrService
{
    private const string WorkloadName = "cabi.EchoApp";
    private const string BundleId = "actr.e2e.echo-app";
    private const string ActorType = "actrium/EchoApp@0.1.0";
    private const string LocalRouteKey = "cabi.EchoApp.Echo";
    private const string LocalPrefix = "winui-local:";   // mirrors Swift's "swift-local:"
    private const long CallTimeoutMs = 30_000;

    private ActrCabiNode? _node;
    private bool _hasAutoSent;

    public string Status { get; private set; } = "Starting ACTR node...";
    public bool IsReady => _node != null;

    public async Task StartIfNeededAsync(string configPath)
    {
        if (_node != null) return;
        try
        {
            RuntimeLog.Info("ACTR InitRuntime");
            ActrCabiNode.InitRuntime();
            RuntimeLog.Info($"ACTR ValidateNative {WorkloadName}");
            ActrNativeInfo native = ActrCabiNode.ValidateNative(WorkloadName, BundleId);
            RuntimeLog.Info($"ACTR native validated {native}");
            var node = new ActrCabiNode();
            RuntimeLog.Info("ACTR FromConfig");
            await node.FromConfig(configPath);
            RuntimeLog.Info($"ACTR WithActorType {ActorType}");
            node.WithActorType(ActorType);
            RuntimeLog.Info($"ACTR LinkWorkload {WorkloadName}");
            await node.LinkWorkload(WorkloadName);
            RuntimeLog.Info("ACTR Register");
            await node.Register(null);    // local mode: use the node's configured Issuer endpoint
            RuntimeLog.Info("ACTR Start");
            await node.Start();
            _node = node;
            Status = "Ready: actrium/EchoApp@0.1.0 (cabi.EchoApp workload)";
            RuntimeLog.Info("ACTR node ready");
        }
        catch (Exception e)
        {
            RuntimeLog.Error("ACTR startup failed", e);
            Status = $"ACTR startup failed: {e.Message}";
            throw;
        }
    }

    public bool ShouldAutoSend() =>
        Environment.GetEnvironmentVariable("ACTR_ECHOAPP_AUTO_SEND") == "1" && !_hasAutoSent;

    public async Task<string> SendEchoAsync(string input)
    {
        if (_node is null) throw new InvalidOperationException("actor unavailable");

        // Mirror Swift's local transform: the forwarded message is prefixed
        // so the remote EchoService reply carries the local provenance.
        var forwarded = $"{LocalPrefix}{input}";
        var payload = EchoProto.EncodeEchoRequest(forwarded);
        var resp = await _node.Call(LocalRouteKey, payload, CallTimeoutMs);
        _hasAutoSent = true;
        return EchoProto.DecodeEchoReply(resp);
    }

    public async Task ShutdownAsync()
    {
        if (_node is null) return;
        try { _node.Shutdown(); await _node.WaitShutdown(); }
        catch { /* best-effort shutdown */ }
        _node = null;
    }

    /// Emit the E2E result marker on stderr (parsed by run.sh).
    internal static void EmitResult(string result) =>
        EmitResultCore(result);

    private static void EmitResultCore(string result)
    {
        RuntimeLog.Info($"E2E result emitted; length={result.Length}");
        Console.Error.WriteLine($"ACTR_E2E_RESULT:{result}");
    }
}
