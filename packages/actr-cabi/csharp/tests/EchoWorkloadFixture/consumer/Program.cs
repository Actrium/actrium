// ActrSharp out-of-repo consumer — CI validation program.
//
// Used by ci-csharp.yml's real win-x64 consumer job to prove a clean net10.0
// console app that references ONLY the fixture package (transitively
// ActrSharp) loads and calls the NuGet-copied actr_cabi.dll. It validates the
// native host-stream and explicit ask_host reverse-RPC bridges, not merely
// managed records or a missing-DLL error path.

using ActrSharp;
using ActrSharp.Native.EchoWorkload;
using System.Runtime.InteropServices;
using System.Text;

Console.WriteLine("rid=" + RuntimeInformation.RuntimeIdentifier);
ActrCabiNode.InitRuntime();
try
{
    ActrNativeInfo info = ActrCabiNode.ValidateNative(
        EchoWorkloadFixture.WorkloadName,
        EchoWorkloadFixture.BundleId);
    Console.WriteLine("native=" + info);
    if (info.AbiVersion != 1)
        throw new InvalidOperationException("ABI version mismatch: " + info.AbiVersion);

    const string ExpectedSender = "1a2b3c.test@example.com";
    byte[] expectedStreamPayload = Encoding.UTF8.GetBytes("native-stream-payload-v1");
    byte[] expectedRequestPayload = Encoding.UTF8.GetBytes("native-reverse-request-v1");
    byte[] expectedResponsePayload = Encoding.UTF8.GetBytes("csharp-reverse-response-v1");
    var streamReceived = new TaskCompletionSource<ActrStreamChunk>(
        TaskCreationOptions.RunContinuationsAsynchronously);
    var reverseRequestReceived = new TaskCompletionSource<ActrReverseRpcRequest>(
        TaskCreationOptions.RunContinuationsAsynchronously);
    var responseSent = new TaskCompletionSource<byte[]>(
        TaskCreationOptions.RunContinuationsAsynchronously);
    bool fixtureListening = false;
    try
    {
        ActrCabiNode.RegisterStreamSink(chunk =>
        {
            if (chunk.StreamId == EchoWorkloadFixture.HostBridgeStreamId)
                streamReceived.TrySetResult(chunk);
        });
        ActrCabiNode.RegisterReverseRpcHandler(request =>
        {
            reverseRequestReceived.TrySetResult(request);
            responseSent.TrySetResult(expectedResponsePayload);
            return Task.FromResult(expectedResponsePayload);
        });

        // The fixture listener deliberately accepts this exact ID before its
        // Rust export forwards the chunk through actr-cabi's stream sink.
        EchoWorkloadFixture.ListenHostBridgeStream();
        fixtureListening = true;
        EchoWorkloadFixture.TriggerHostBridgeProbe();

        ActrStreamChunk chunk = await WaitFor(
            streamReceived.Task, "native stream callback for the fixture stream ID");
        ActrReverseRpcRequest request = await WaitFor(
            reverseRequestReceived.Task, "native reverse-RPC request");
        byte[] response = await WaitFor(responseSent.Task, "C# reverse-RPC response dispatch");

        if (chunk.StreamId != EchoWorkloadFixture.HostBridgeStreamId)
            throw new InvalidOperationException("stream callback carried the wrong stream ID");
        if (!chunk.Payload.SequenceEqual(expectedStreamPayload))
            throw new InvalidOperationException("stream callback payload bytes differ");
        if (chunk.Sequence != 580)
            throw new InvalidOperationException("stream callback sequence differs: " + chunk.Sequence);
        if (chunk.TimestampMs != 1_735_689_600_123)
            throw new InvalidOperationException("stream callback timestamp differs: " + chunk.TimestampMs);
        if (chunk.SenderId != ExpectedSender)
            throw new InvalidOperationException("stream callback sender differs: " + chunk.SenderId);
        if (chunk.Metadata.Count != 2 ||
            chunk.Metadata[0] != new ActrStreamMetadata("content-type", "application/octet-stream") ||
            chunk.Metadata[1] != new ActrStreamMetadata("fixture", "pr-580"))
            throw new InvalidOperationException("stream callback metadata differs");
        if (request.RequestId == 0)
            throw new InvalidOperationException("native reverse-RPC request ID must be non-zero");
        if (request.Route != "fixture.host-bridge.ask.v1")
            throw new InvalidOperationException("unexpected native reverse-RPC route: " + request.Route);
        if (!request.Payload.SequenceEqual(expectedRequestPayload))
            throw new InvalidOperationException("reverse-RPC request bytes changed before assertion");
        if (!response.SequenceEqual(expectedResponsePayload))
            throw new InvalidOperationException("reverse-RPC response bytes differ");

        Console.WriteLine("ActrSharp native host-bridge consumption OK");
    }
    finally
    {
        // The native fixture listener is idempotent, and ActrSharp clears wait
        // for in-flight callbacks before returning. Nesting ensures handlers
        // are cleared even when one cleanup operation reports a failure.
        try
        {
            if (fixtureListening)
                EchoWorkloadFixture.UnlistenHostBridgeStream();
        }
        finally
        {
            try
            {
                ActrCabiNode.RegisterReverseRpcHandler(null);
            }
            finally
            {
                ActrCabiNode.RegisterStreamSink(null);
            }
        }
    }
}
finally
{
    ActrCabiNode.ShutdownRuntime();
}

static async Task<T> WaitFor<T>(Task<T> task, string operation)
{
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
    try
    {
        return await task.WaitAsync(timeout.Token);
    }
    catch (OperationCanceledException) when (timeout.IsCancellationRequested)
    {
        throw new TimeoutException($"timed out after 10 seconds waiting for {operation}");
    }
}
