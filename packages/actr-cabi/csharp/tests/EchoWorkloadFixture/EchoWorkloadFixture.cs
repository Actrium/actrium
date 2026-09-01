// ActrSharp.Native.EchoWorkload — TEST-ONLY CI fixture.
//
// The native actr_cabi.dll (built from tests/e2e/actr/winui-echo-app/workload) ships under
// runtimes/win-{x64,arm64}/native/ and is auto-copied by NuGet at restore time.
// This assembly exists only so the fixture package has a managed target; it
// exposes the workload identity and the test-only native host-bridge probe the
// out-of-repo consumer validates against.

using System.Runtime.InteropServices;
using System.Text;

namespace ActrSharp.Native.EchoWorkload;

/// <summary>
/// Marker type and constants for the test-only EchoWorkload fixture. The
/// workload is registered as <c>cabi.EchoApp</c> with bundle id
/// <c>actr.e2e.echo-app</c> inside the bundled actr_cabi.dll.
/// </summary>
public static partial class EchoWorkloadFixture
{
    /// <summary>Workload name registered via <c>export_workload!</c>.</summary>
    public const string WorkloadName = "cabi.EchoApp";

    /// <summary>Bundle id declared via <c>export_bundle!</c>.</summary>
    public const string BundleId = "actr.e2e.echo-app";

    /// <summary>Exact stream ID emitted by the native PR 580 host-bridge probe.</summary>
    public const string HostBridgeStreamId = "actrsharp.fixture.host-bridge.v1";

    /// <summary>Install the test fixture's explicit native stream listener.</summary>
    public static void ListenHostBridgeStream() =>
        Check(Native.Listen(Encoding.UTF8.GetBytes(HostBridgeStreamId), (nuint)HostBridgeStreamId.Length),
            "listen");

    /// <summary>Remove the test fixture's explicit native stream listener.</summary>
    public static void UnlistenHostBridgeStream() =>
        Check(Native.Unlisten(Encoding.UTF8.GetBytes(HostBridgeStreamId), (nuint)HostBridgeStreamId.Length),
            "unlisten");

    /// <summary>
    /// Invoke the native-only PR 580 probe. It forwards one <c>DataChunk</c>
    /// through actr-cabi, requests C# through <c>ask_host</c>, and validates the
    /// response before returning.
    /// </summary>
    public static void TriggerHostBridgeProbe() => Check(Native.Trigger(), "trigger");

    static void Check(int code, string operation)
    {
        if (code != 0)
            throw new InvalidOperationException(
                $"native host-bridge fixture {operation} failed with status {code}; " +
                "verify the exact stream listener and reverse-RPC response are registered.");
    }

    static partial class Native
    {
        [LibraryImport("actr_cabi", EntryPoint = "actr_fixture_host_bridge_listen")]
        internal static partial int Listen(byte[] streamId, nuint streamIdLength);

        [LibraryImport("actr_cabi", EntryPoint = "actr_fixture_host_bridge_unlisten")]
        internal static partial int Unlisten(byte[] streamId, nuint streamIdLength);

        [LibraryImport("actr_cabi", EntryPoint = "actr_fixture_host_bridge_trigger")]
        internal static partial int Trigger();
    }
}
