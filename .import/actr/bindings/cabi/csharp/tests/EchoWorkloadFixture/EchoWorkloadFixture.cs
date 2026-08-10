// ActrSharp.Native.EchoWorkload — TEST-ONLY CI fixture.
//
// The native actr_cabi.dll (built from e2e/winui-echo-app/workload) ships under
// runtimes/win-{x64,arm64}/native/ and is auto-copied by NuGet at restore time.
// This assembly exists only so the fixture package has a managed target; it
// also exposes the workload name and bundle ID the consumer validates against.

namespace ActrSharp.Native.EchoWorkload;

/// <summary>
/// Marker type and constants for the test-only EchoWorkload fixture. The
/// workload is registered as <c>cabi.EchoApp</c> with bundle id
/// <c>actr.e2e.echo-app</c> inside the bundled actr_cabi.dll.
/// </summary>
public static class EchoWorkloadFixture
{
    /// <summary>Workload name registered via <c>export_workload!</c>.</summary>
    public const string WorkloadName = "cabi.EchoApp";

    /// <summary>Bundle id declared via <c>export_bundle!</c>.</summary>
    public const string BundleId = "actr.e2e.echo-app";
}
