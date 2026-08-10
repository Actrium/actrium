// ActrSharp out-of-repo consumer — CI validation program.
//
// Used by ci-csharp.yml (win-x64 gate + win-arm64 cross + ARM64 signal) to
// prove a clean net10.0 console app that references ONLY the fixture package
// (transitively ActrSharp) can: load actr_cabi.dll via NuGet RID auto-copy,
// InitRuntime (ABI + actr version check), ValidateNative (workload + bundle),
// read ActrNativeInfo, and shut down — with no manual DLL copy.

using ActrSharp;
using ActrSharp.Native.EchoWorkload;
using System.Runtime.InteropServices;

Console.WriteLine("rid=" + RuntimeInformation.RuntimeIdentifier);
ActrCabiNode.InitRuntime();
ActrNativeInfo info = ActrCabiNode.ValidateNative(
    EchoWorkloadFixture.WorkloadName,
    EchoWorkloadFixture.BundleId);
Console.WriteLine("native=" + info);
if (info.AbiVersion != 1)
    throw new InvalidOperationException("ABI version mismatch: " + info.AbiVersion);
ActrCabiNode.ShutdownRuntime();
Console.WriteLine("ActrSharp consumption OK");
