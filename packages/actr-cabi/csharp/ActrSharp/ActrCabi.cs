// ActrSharp — C# wrapper over the actr-cabi C ABI.
//
// The native async API reports completion through callbacks. This wrapper
// roots the callback delegates, copies callback-owned buffers synchronously,
// and completes .NET tasks asynchronously to avoid running managed
// continuations on an actr Tokio worker or inside a reverse P/Invoke frame.

using System.Diagnostics.CodeAnalysis;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Channels;

namespace ActrSharp;

public sealed class ActrCabiException : Exception
{
    // Managed-side validation codes. The validation-specific codes (-30, -31,
    // -32, -33, -40, -41, -42) are distinct from every Rust callback status code so
    // a code alone identifies a managed-validation failure. The codes below
    // intentionally mirror Rust codes (-10 NULL_POINTER, -20 WORKLOAD_NOT_FOUND,
    // -21 HANDLE_INVALID, -22 REQUEST_ID_UNKNOWN,
    // -24 DELIVERY_UNCERTAIN, -50 GENERIC_ERROR). The native surface reports
    // these statuses through callbacks or synchronous validation failures, and
    // the managed wrapper preserves each code for consistent caller handling.
    public const int CodeNullPointer = -10;
    public const int CodeWorkloadNotFound = -20;
    public const int CodeHandleInvalid = -21;
    public const int CodeRequestIdUnknown = -22;
    public const int CodeDeliveryUncertain = -24;
    public const int CodeGenericError = -50;
    public const int CodeAbiMismatch = -30;
    public const int CodeActrMismatch = -31;
    public const int CodeBundleMismatch = -32;
    public const int CodeBundleConflict = -33;
    public const int CodeDllNotFound = -40;
    public const int CodeArchMismatch = -41;
    public const int CodeEntryMissing = -42;

    public int Code { get; }

    /// <summary>What ActrSharp expected (e.g. "abi=1", "actr=0.7.0").</summary>
    public string? Expected { get; init; }

    /// <summary>What the loaded native library reported.</summary>
    public string? Actual { get; init; }

    /// <summary>The .NET process runtime identifier (e.g. "win-x64").</summary>
    public string? Rid { get; init; }

    /// <summary>The bundle ID registered by the workload cdylib, if any.</summary>
    public string? Bundle { get; init; }

    public ActrCabiException(int code, string message) : base(message)
    {
        Code = code;
    }

    public ActrCabiException(int code, string message, Exception inner) : base(message, inner)
    {
        Code = code;
    }

    public override string Message
    {
        get
        {
            var sb = new StringBuilder(base.Message);
            bool haveContext = false;
            Append(ref haveContext, sb, "expected", Expected);
            Append(ref haveContext, sb, "actual", Actual);
            Append(ref haveContext, sb, "rid", Rid);
            Append(ref haveContext, sb, "bundle", Bundle);
            if (haveContext) sb.Append(']');
            return sb.ToString();
        }
    }

    static void Append(ref bool started, StringBuilder sb, string name, string? value)
    {
        if (value is null) return;
        sb.Append(started ? ' ' : " [");
        sb.Append(name).Append('=').Append(value);
        started = true;
    }

    internal static ActrCabiException FromCallback(int code, nint errorPtr, nuint errorLen)
    {
        string message = errorPtr == nint.Zero || errorLen == 0
            ? $"actr-cabi error {code}"
            : PtrToUtf8(errorPtr, errorLen);
        return new ActrCabiException(code, message);
    }

    internal static string PtrToUtf8(nint ptr, nuint len)
    {
        if (len == 0) return string.Empty;
        if (ptr == nint.Zero)
            throw new ActrCabiException(CodeNullPointer, "native UTF-8 buffer is null");
        if (len > int.MaxValue)
            throw new ActrCabiException(CodeGenericError, "native UTF-8 buffer is too large");
        var bytes = new byte[(int)len];
        Marshal.Copy(ptr, bytes, 0, bytes.Length);
        return Encoding.UTF8.GetString(bytes);
    }

    /// <summary>Read a static, process-lifetime UTF-8 string, or null if the
    /// native pointer is null/empty. Unlike callback buffers these pointers
    /// are never freed; we copy into a managed string for safety.</summary>
    internal static string? TryPtrToUtf8(nint ptr, nuint len)
    {
        if (ptr == nint.Zero || len == 0) return null;
        return PtrToUtf8(ptr, len);
    }
}

/// <summary>
/// Diagnostics for the loaded <c>actr_cabi</c> native library, queried from
/// the static introspection entry points. Returned by
/// <see cref="ActrCabiNode.ValidateNative"/>.
/// </summary>
public sealed record ActrNativeInfo(uint AbiVersion, string ActrVersion, string? BundleId)
{
    public override string ToString() =>
        $"abi={AbiVersion} actr={ActrVersion}" +
        (BundleId is null ? " bundle=<none>" : $" bundle={BundleId}");
}

/// <summary>
/// Opaque actr-cabi token. Native typestate transitions consume their inputs;
/// releasing a consumed token is an intentional native no-op, which lets every
/// managed ownership path use ordinary SafeHandle disposal.
/// </summary>
internal sealed class ActrCabiHandle : SafeHandle
{
    readonly bool _isRef;

    public ActrCabiHandle() : base(nint.Zero, ownsHandle: true) { }

    public ActrCabiHandle(nint value, bool isRef = false) : this()
    {
        _isRef = isRef;
        SetHandle(value);
    }

    public override bool IsInvalid => handle == nint.Zero;

    public nint Ptr => DangerousGetHandle();

    protected override bool ReleaseHandle()
    {
        try
        {
            if (_isRef) Native.actr_release_actr_ref(handle);
            else Native.actr_release_handle(handle);
            return true;
        }
        catch
        {
            // SafeHandle finalization must never throw into the finalizer thread.
            return false;
        }
    }
}

// ── C callback delegate types ────────────────────────────────────────────────

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrVoidCallback(int status, nint errorPtr, nuint errorLen, nint userData);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrHandleCallback(
    int status,
    nint resultHandle,
    nint errorPtr,
    nuint errorLen,
    nint userData);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrCallCallback(
    int status,
    nint responsePtr,
    nuint responseLen,
    nint errorPtr,
    nuint errorLen,
    nint userData);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrDiscoverCallback(
    int status,
    nint actorIdPtr,
    nuint actorIdLen,
    nint errorPtr,
    nuint errorLen,
    nint userData);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrStreamSinkCallback(
    nint streamIdPtr, nuint streamIdLen,
    nint payloadPtr, nuint payloadLen,
    nint metadataPtr, nuint metadataLen,
    ulong sequence,
    byte chunkFlags, long timestampMs,
    nint senderIdPtr, nuint senderIdLen,
    nint userData);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate void ActrReverseRpcCallback(
    ulong requestId,
    nint routePtr, nuint routeLen,
    nint payloadPtr, nuint payloadLen,
    nint userData);

// ── Continuation registry ────────────────────────────────────────────────────

static class ContinuationMap
{
    static readonly Dictionary<nint, object> s_map = [];
    static nint s_counter;

    internal static nint Register(object continuation)
    {
        lock (s_map)
        {
            do
            {
                s_counter = s_counter == nint.MaxValue ? 1 : s_counter + 1;
            } while (s_map.ContainsKey(s_counter));
            s_map[s_counter] = continuation;
            return s_counter;
        }
    }

    internal static bool TryTake<T>(
        nint key,
        [NotNullWhen(true)] out T? continuation) where T : class
    {
        lock (s_map)
        {
            if (s_map.Remove(key, out object? value) && value is T typed)
            {
                continuation = typed;
                return true;
            }
        }
        continuation = null;
        return false;
    }
}

// ── Rooted reverse-P/Invoke trampolines ─────────────────────────────────────

static unsafe class Trampolines
{
    // Static fields keep the delegates and their unmanaged thunks alive for the
    // lifetime of the process.
    public static readonly ActrHandleCallback Handle = HandleImpl;
    public static readonly ActrCallCallback Call = CallImpl;
    public static readonly ActrDiscoverCallback Discover = DiscoverImpl;
    public static readonly ActrVoidCallback Void = VoidImpl;
    public static readonly ActrStreamSinkCallback StreamSink = StreamSinkImpl;
    public static readonly ActrReverseRpcCallback ReverseRpc = ReverseRpcImpl;

    static void HandleImpl(int status, nint resultHandle, nint errorPtr, nuint errorLen, nint userData)
    {
        TaskCompletionSource<nint>? completion = null;
        try
        {
            if (!ContinuationMap.TryTake(userData, out completion))
            {
                if (resultHandle != nint.Zero) Native.actr_release_handle(resultHandle);
                return;
            }

            if (status == 0 && resultHandle != nint.Zero)
            {
                completion.TrySetResult(resultHandle);
            }
            else
            {
                if (resultHandle != nint.Zero) Native.actr_release_handle(resultHandle);
                completion.TrySetException(status == 0
                    ? new ActrCabiException(ActrCabiException.CodeHandleInvalid, "native callback returned a null handle")
                    : ActrCabiException.FromCallback(status, errorPtr, errorLen));
            }
        }
        catch (Exception error)
        {
            // Managed exceptions must never unwind across the native callback
            // boundary. Complete the task when possible, then return normally.
            completion?.TrySetException(error);
        }
    }

    static void CallImpl(
        int status,
        nint responsePtr,
        nuint responseLen,
        nint errorPtr,
        nuint errorLen,
        nint userData)
    {
        TaskCompletionSource<byte[]>? completion = null;
        try
        {
            if (!ContinuationMap.TryTake(userData, out completion)) return;
            if (status != 0)
            {
                completion.TrySetException(ActrCabiException.FromCallback(status, errorPtr, errorLen));
                return;
            }
            if (responseLen == 0)
            {
                completion.TrySetResult([]);
                return;
            }
            if (responsePtr == nint.Zero)
            {
                completion.TrySetException(new ActrCabiException(ActrCabiException.CodeNullPointer, "null response buffer"));
                return;
            }
            if (responseLen > int.MaxValue)
            {
                completion.TrySetException(new ActrCabiException(ActrCabiException.CodeGenericError, "native response is too large"));
                return;
            }

            var response = new byte[(int)responseLen];
            Marshal.Copy(responsePtr, response, 0, response.Length);
            completion.TrySetResult(response);
        }
        catch (Exception error)
        {
            completion?.TrySetException(error);
        }
    }

    static void DiscoverImpl(
        int status,
        nint actorIdPtr,
        nuint actorIdLen,
        nint errorPtr,
        nuint errorLen,
        nint userData)
    {
        TaskCompletionSource<string>? completion = null;
        try
        {
            if (!ContinuationMap.TryTake(userData, out completion)) return;
            if (status != 0)
            {
                completion.TrySetException(ActrCabiException.FromCallback(status, errorPtr, errorLen));
            }
            else if (actorIdPtr == nint.Zero || actorIdLen == 0)
            {
                completion.TrySetException(new ActrCabiException(ActrCabiException.CodeNullPointer, "null or empty actor ID"));
            }
            else
            {
                completion.TrySetResult(ActrCabiException.PtrToUtf8(actorIdPtr, actorIdLen));
            }
        }
        catch (Exception error)
        {
            completion?.TrySetException(error);
        }
    }

    static void VoidImpl(int status, nint errorPtr, nuint errorLen, nint userData)
    {
        TaskCompletionSource? completion = null;
        try
        {
            if (!ContinuationMap.TryTake(userData, out completion)) return;
            if (status == 0) completion.TrySetResult();
            else completion.TrySetException(ActrCabiException.FromCallback(status, errorPtr, errorLen));
        }
        catch (Exception error)
        {
            completion?.TrySetException(error);
        }
    }

    static void StreamSinkImpl(
        nint streamIdPtr, nuint streamIdLen,
        nint payloadPtr, nuint payloadLen,
        nint metadataPtr, nuint metadataLen,
        ulong sequence,
        byte chunkFlags, long timestampMs,
        nint senderIdPtr, nuint senderIdLen,
        nint userData)
    {
        try
        {
            // Native buffers are valid only for this call. Copy every field
            // before putting the event on the managed bounded queue.
            var chunk = new ActrStreamChunk(
                ActrCabiException.PtrToUtf8(streamIdPtr, streamIdLen),
                CopyBuffer(payloadPtr, payloadLen),
                sequence,
                (chunkFlags & ActrStreamChunk.HasTimestampFlag) != 0 ? timestampMs : null,
                ActrCabiException.PtrToUtf8(senderIdPtr, senderIdLen),
                CopyMetadata(metadataPtr, metadataLen));
            ActrHostBridge.OnStreamChunk(chunk);
        }
        catch
        {
            // Managed exceptions must never unwind across a native callback.
        }
    }

    static void ReverseRpcImpl(
        ulong requestId,
        nint routePtr, nuint routeLen,
        nint payloadPtr, nuint payloadLen,
        nint userData)
    {
        try
        {
            ActrHostBridge.OnReverseRpc(
                requestId,
                ActrCabiException.PtrToUtf8(routePtr, routeLen),
                CopyBuffer(payloadPtr, payloadLen));
        }
        catch
        {
            // Copying/dispatch failed before RespondAsync could map it. Reject
            // immediately instead of retaining the Rust request until its
            // deadline. The native side treats a raced late response as a
            // harmless unknown id.
            ActrHostBridge.RejectReverseRpc(requestId);
        }
    }

    static byte[] CopyBuffer(nint ptr, nuint len)
    {
        if (len == 0) return [];
        if (ptr == nint.Zero)
            throw new ActrCabiException(ActrCabiException.CodeNullPointer, "native buffer is null");
        if (len > int.MaxValue)
            throw new ActrCabiException(ActrCabiException.CodeGenericError, "native buffer is too large");
        var bytes = new byte[(int)len];
        Marshal.Copy(ptr, bytes, 0, bytes.Length);
        return bytes;
    }

    static ActrStreamMetadata[] CopyMetadata(nint ptr, nuint len)
    {
        if (len == 0) return [];
        if (ptr == nint.Zero)
            throw new ActrCabiException(ActrCabiException.CodeNullPointer, "native metadata is null");
        if (len > int.MaxValue)
            throw new ActrCabiException(ActrCabiException.CodeGenericError, "native metadata has too many entries");

        var native = (NativeMetadataEntry*)ptr;
        var metadata = new ActrStreamMetadata[(int)len];
        for (int i = 0; i < metadata.Length; i++)
        {
            NativeMetadataEntry entry = native[i];
            metadata[i] = new ActrStreamMetadata(
                ActrCabiException.PtrToUtf8(entry.KeyPtr, entry.KeyLen),
                ActrCabiException.PtrToUtf8(entry.ValuePtr, entry.ValueLen));
        }
        return metadata;
    }
}

[StructLayout(LayoutKind.Sequential)]
readonly struct NativeMetadataEntry
{
    internal readonly nint KeyPtr;
    internal readonly nuint KeyLen;
    internal readonly nint ValuePtr;
    internal readonly nuint ValueLen;
}

// ── Native imports ──────────────────────────────────────────────────────────

static unsafe partial class Native
{
    [LibraryImport("actr_cabi")]
    internal static partial int actr_init_runtime();

    [LibraryImport("actr_cabi")]
    internal static partial int actr_shutdown_runtime();

    // ── native introspection (static results — do NOT free) ────────────────

    [LibraryImport("actr_cabi")]
    internal static partial uint actr_cabi_abi_version();

    [LibraryImport("actr_cabi", EntryPoint = "actr_cabi_actr_version")]
    private static partial nint ActrVersionImport(out nuint len);

    internal static string? ActrVersion() =>
        ActrCabiException.TryPtrToUtf8(ActrVersionImport(out nuint len), len);

    [LibraryImport("actr_cabi", EntryPoint = "actr_cabi_bundle_id")]
    private static partial nint BundleIdImport(out nuint len);

    internal static string? BundleId() =>
        ActrCabiException.TryPtrToUtf8(BundleIdImport(out nuint len), len);

    [LibraryImport("actr_cabi")]
    internal static partial uint actr_cabi_bundle_count();

    [LibraryImport("actr_cabi")]
    internal static partial void actr_release_handle(nint handle);

    [LibraryImport("actr_cabi")]
    internal static partial void actr_release_actr_ref(nint handle);

    [LibraryImport("actr_cabi", EntryPoint = "actr_node_from_config")]
    private static partial int NodeFromConfigImport(
        byte* path,
        nuint pathLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrHandleCallback callback,
        nint userData);

    internal static int NodeFromConfig(byte[] path, ActrHandleCallback callback, nint userData)
    {
        fixed (byte* pathPtr = path)
            return NodeFromConfigImport(pathPtr, (nuint)path.Length, callback, userData);
    }

    [LibraryImport("actr_cabi", EntryPoint = "actr_node_with_actor_type")]
    private static partial nint NodeWithActorTypeImport(
        nint initHandle,
        byte* actorType,
        nuint actorTypeLen);

    internal static nint NodeWithActorType(nint initHandle, byte[] actorType)
    {
        fixed (byte* actorTypePtr = actorType)
            return NodeWithActorTypeImport(initHandle, actorTypePtr, (nuint)actorType.Length);
    }

    [LibraryImport("actr_cabi", EntryPoint = "actr_workload_create")]
    private static partial nint WorkloadCreateImport(byte* name, nuint nameLen);

    internal static nint WorkloadCreate(byte[] name)
    {
        fixed (byte* namePtr = name)
            return WorkloadCreateImport(namePtr, (nuint)name.Length);
    }

    [LibraryImport("actr_cabi")]
    internal static partial int actr_node_link(
        nint initHandle,
        nint workloadHandle,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrHandleCallback callback,
        nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_node_register")]
    private static partial int NodeRegisterImport(
        nint attachedHandle,
        byte* issuer,
        nuint issuerLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrHandleCallback callback,
        nint userData);

    internal static int NodeRegister(
        nint attachedHandle,
        byte[]? issuer,
        ActrHandleCallback callback,
        nint userData)
    {
        fixed (byte* issuerPtr = issuer)
            return NodeRegisterImport(attachedHandle, issuerPtr, (nuint)(issuer?.Length ?? 0), callback, userData);
    }

    [LibraryImport("actr_cabi")]
    internal static partial int actr_node_start(
        nint registeredHandle,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrHandleCallback callback,
        nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_ref_call")]
    private static partial int RefCallImport(
        nint refHandle,
        byte* routeKey,
        nuint routeKeyLen,
        byte* payload,
        nuint payloadLen,
        long timeoutMs,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrCallCallback callback,
        nint userData);

    internal static int RefCall(
        nint refHandle,
        byte[] routeKey,
        byte[] payload,
        long timeoutMs,
        ActrCallCallback callback,
        nint userData)
    {
        fixed (byte* routeKeyPtr = routeKey)
        fixed (byte* payloadPtr = payload)
            return RefCallImport(
                refHandle,
                routeKeyPtr,
                (nuint)routeKey.Length,
                payloadPtr,
                (nuint)payload.Length,
                timeoutMs,
                callback,
                userData);
    }

    [LibraryImport("actr_cabi", EntryPoint = "actr_ref_discover")]
    private static partial int RefDiscoverImport(
        nint refHandle,
        byte* targetType,
        nuint targetTypeLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrDiscoverCallback callback,
        nint userData);

    internal static int RefDiscover(
        nint refHandle,
        byte[] targetType,
        ActrDiscoverCallback callback,
        nint userData)
    {
        fixed (byte* targetTypePtr = targetType)
            return RefDiscoverImport(
                refHandle,
                targetTypePtr,
                (nuint)targetType.Length,
                callback,
                userData);
    }

    [LibraryImport("actr_cabi")]
    internal static partial int actr_ref_shutdown(nint refHandle);

    [LibraryImport("actr_cabi")]
    internal static partial int actr_ref_wait_shutdown(
        nint refHandle,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrVoidCallback callback,
        nint userData);

    // ── host bridge: stream sink + reverse RPC ─────────────────────────────

    [LibraryImport("actr_cabi", EntryPoint = "actr_register_stream_sink")]
    internal static partial int RegisterStreamSink(
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrStreamSinkCallback callback,
        nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_register_stream_sink")]
    internal static partial int RegisterStreamSinkNull(nint callback, nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_register_reverse_rpc_sink")]
    internal static partial int RegisterReverseRpcSink(
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrReverseRpcCallback callback,
        nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_register_reverse_rpc_sink")]
    internal static partial int RegisterReverseRpcSinkNull(nint callback, nint userData);

    [LibraryImport("actr_cabi", EntryPoint = "actr_host_respond_reverse_rpc")]
    private static partial int HostRespondImport(
        ulong requestId,
        int status,
        byte* payload,
        nuint payloadLen);

    internal static int HostRespond(ulong requestId, int status, byte[]? payload)
    {
        if (payload is null || payload.Length == 0)
            return HostRespondImport(requestId, status, null, 0);
        fixed (byte* payloadPtr = payload)
            return HostRespondImport(requestId, status, payloadPtr, (nuint)payload.Length);
    }

    [LibraryImport("actr_cabi", EntryPoint = "actr_stream_listen")]
    private static partial int StreamListenImport(
        nint refHandle,
        byte* streamId,
        nuint streamIdLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrVoidCallback callback,
        nint userData);

    internal static int StreamListen(nint refHandle, byte[] streamId, ActrVoidCallback callback, nint userData)
    {
        fixed (byte* streamIdPtr = streamId)
            return StreamListenImport(refHandle, streamIdPtr, (nuint)streamId.Length, callback, userData);
    }

    [LibraryImport("actr_cabi", EntryPoint = "actr_stream_unlisten")]
    private static partial int StreamUnlistenImport(
        nint refHandle,
        byte* streamId,
        nuint streamIdLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrVoidCallback callback,
        nint userData);

    internal static int StreamUnlisten(nint refHandle, byte[] streamId, ActrVoidCallback callback, nint userData)
    {
        fixed (byte* streamIdPtr = streamId)
            return StreamUnlistenImport(refHandle, streamIdPtr, (nuint)streamId.Length, callback, userData);
    }
}

// ── Host bridge data ─────────────────────────────────────────────────────────

/// <summary>A copied key/value metadata entry from a forwarded data chunk.</summary>
public sealed record ActrStreamMetadata(string Key, string Value);

/// <summary>
/// One forwarded data-stream chunk. All fields, including metadata, are copied
/// before the native callback returns.
/// </summary>
public sealed record ActrStreamChunk(
    string StreamId,
    byte[] Payload,
    ulong Sequence,
    long? TimestampMs,
    string SenderId,
    IReadOnlyList<ActrStreamMetadata> Metadata)
{
    /// <summary>Creates a chunk with no metadata (source-compatible helper).</summary>
    public ActrStreamChunk(
        string streamId,
        byte[] payload,
        ulong sequence,
        long? timestampMs,
        string senderId)
        : this(streamId, payload, sequence, timestampMs, senderId, [])
    {
    }

    /// <summary>Bit set when the producer annotated <see cref="TimestampMs"/>.</summary>
    public const byte HasTimestampFlag = 0x01;
}

/// <summary>An inbound request a Rust workload explicitly delegated to its host.</summary>
public sealed record ActrReverseRpcRequest(ulong RequestId, string Route, byte[] Payload);

static class ActrHostBridge
{
    // Bounded, best-effort host dispatch. The native runtime can preserve its
    // own reliable lane semantics, but a synchronous C callback cannot wait on
    // arbitrary UI work without stalling that runtime worker.
    internal const int StreamQueueCapacity = 256;

    sealed class StreamRegistration(Action<ActrStreamChunk> handler)
    {
        internal Action<ActrStreamChunk> Handler { get; } = handler;
    }

    sealed record StreamQueueItem(StreamRegistration Registration, ActrStreamChunk Chunk);

    static readonly Channel<StreamQueueItem> s_streamQueue =
        Channel.CreateBounded<StreamQueueItem>(new BoundedChannelOptions(StreamQueueCapacity)
        {
            SingleReader = true,
            FullMode = BoundedChannelFullMode.Wait,
        });

    static readonly object s_registrationLock = new();
    static long s_droppedStreamChunkCount;
    static StreamRegistration? s_streamRegistration;
    static Func<ActrReverseRpcRequest, Task<byte[]>>? s_reverseHandler;

    static ActrHostBridge()
    {
        _ = Task.Run(ProcessStreamQueueAsync);
    }

    internal static long DroppedStreamChunkCount => Interlocked.Read(ref s_droppedStreamChunkCount);

    internal static int RegisterStreamHandler(Action<ActrStreamChunk>? handler, Func<int> nativeUpdate)
    {
        lock (s_registrationLock)
        {
            // Stop managed delivery before a native clear waits for any
            // in-flight callback. Queued chunks observe this null handler too.
            StreamRegistration? previous = Volatile.Read(ref s_streamRegistration);
            if (handler is null) SetStreamHandler(null);
            int code;
            try
            {
                code = nativeUpdate();
            }
            catch
            {
                if (handler is null) Volatile.Write(ref s_streamRegistration, previous);
                throw;
            }
            if (code != 0)
            {
                if (handler is null) Volatile.Write(ref s_streamRegistration, previous);
                return code;
            }
            if (code == 0 && handler is not null) SetStreamHandler(handler);
            return code;
        }
    }

    internal static int RegisterReverseHandler(
        Func<ActrReverseRpcRequest, Task<byte[]>>? handler,
        Func<int> nativeUpdate)
    {
        lock (s_registrationLock)
        {
            Func<ActrReverseRpcRequest, Task<byte[]>>? previous = Volatile.Read(ref s_reverseHandler);
            if (handler is null) SetReverseHandler(null);
            int code;
            try
            {
                code = nativeUpdate();
            }
            catch
            {
                if (handler is null) Volatile.Write(ref s_reverseHandler, previous);
                throw;
            }
            if (code != 0)
            {
                if (handler is null) Volatile.Write(ref s_reverseHandler, previous);
                return code;
            }
            if (code == 0 && handler is not null) SetReverseHandler(handler);
            return code;
        }
    }

    internal static void SetStreamHandler(Action<ActrStreamChunk>? handler) =>
        Volatile.Write(
            ref s_streamRegistration,
            handler is null ? null : new StreamRegistration(handler));

    internal static void SetReverseHandler(Func<ActrReverseRpcRequest, Task<byte[]>>? handler) =>
        Volatile.Write(ref s_reverseHandler, handler);

    internal static void OnStreamChunk(ActrStreamChunk chunk)
    {
        StreamRegistration? registration = Volatile.Read(ref s_streamRegistration);
        if (registration is null) return;
        if (!s_streamQueue.Writer.TryWrite(new StreamQueueItem(registration, chunk)))
            Interlocked.Increment(ref s_droppedStreamChunkCount);
    }

    static async Task ProcessStreamQueueAsync()
    {
        await foreach (StreamQueueItem item in s_streamQueue.Reader.ReadAllAsync().ConfigureAwait(false))
        {
            // A clear or replacement invalidates queued work from the old
            // registration. The queued item retains its delegate until it is
            // discarded, so an in-flight native callback can never observe a
            // collected delegate or dispatch an old chunk to a new handler.
            if (!ReferenceEquals(item.Registration, Volatile.Read(ref s_streamRegistration))) continue;
            try { item.Registration.Handler(item.Chunk); }
            catch { /* One bad event cannot stop later dispatches. */ }
        }
    }

    internal static void OnReverseRpc(ulong requestId, string route, byte[] payload) =>
        _ = RespondAsync(requestId, route, payload);

    internal static void RejectReverseRpc(ulong requestId)
    {
        try
        {
            _ = Native.HostRespond(requestId, ActrCabiException.CodeGenericError, null);
        }
        catch
        {
            // This runs on the native callback boundary; never allow a
            // marshalling or loader failure to escape it.
        }
    }

    static async Task RespondAsync(ulong requestId, string route, byte[] payload)
    {
        int status = 0;
        byte[]? answer = null;
        try
        {
            Func<ActrReverseRpcRequest, Task<byte[]>>? handler = Volatile.Read(ref s_reverseHandler);
            if (handler is null)
                status = ActrCabiException.CodeGenericError;
            else
            {
                answer = await handler(new ActrReverseRpcRequest(requestId, route, payload)).ConfigureAwait(false);
                if (answer is null) status = ActrCabiException.CodeGenericError;
            }
        }
        catch
        {
            status = ActrCabiException.CodeGenericError;
        }

        // A late/duplicate id is already resolved natively. It is intentionally
        // ignored here: a managed handler must not turn an expired request into
        // an unobserved task failure.
        try
        {
            _ = Native.HostRespond(requestId, status, answer);
        }
        catch
        {
            // Never leave an unobserved exception on the fire-and-forget
            // callback task. The native deadline remains authoritative.
        }
    }
}

// ── ActrCabiNode ────────────────────────────────────────────────────────────

public sealed class ActrCabiNode : IDisposable
{
    enum NodeState
    {
        Empty,
        Initializing,
        Initialized,
        Linking,
        Attached,
        Registering,
        Registered,
        Starting,
        Started,
        WaitingForShutdown,
        Disposed,
    }

    readonly object _stateLock = new();
    NodeState _state;
    ActrCabiHandle? _handle;

    // ── native validation ──────────────────────────────────────────────────

    /// <summary>
    /// The ABI version ActrSharp was compiled against. Must match the value
    /// returned by the native <c>actr_cabi_abi_version</c> entry point.
    /// Bump in lockstep with <c>crates/actr-cabi</c>'s <c>ABI_VERSION</c>.
    /// </summary>
    internal const uint ExpectedAbiVersion = 1;

    /// <summary>
    /// The actr version ActrSharp expects from the native library. Read from
    /// the assembly's <see cref="AssemblyInformationalVersionAttribute"/>,
    /// which the .csproj pins to the package version (== Cargo workspace
    /// version).
    /// </summary>
    internal static string ExpectedActrVersion { get; } = ReadExpectedActrVersion();

    /// <summary>The .NET process runtime identifier, e.g. "win-x64".</summary>
    internal static string CurrentRid => RuntimeInformation.RuntimeIdentifier;

    static string ReadExpectedActrVersion()
    {
        var attr = typeof(ActrCabiNode).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>();
        if (attr is null) return "0.0.0";
        // The .csproj pins InformationalVersion to $(Version) (no +commit
        // suffix), but defend against the SDK appending SourceRevisionId in
        // non-deterministic local builds.
        var v = attr.InformationalVersion;
        int plus = v.IndexOf('+');
        return plus >= 0 ? v[..plus] : v;
    }

    /// <summary>
    /// Verify the loaded native library's ABI and actr version match
    /// ActrSharp's expectations, then initialize the process-global Tokio
    /// runtime.
    /// A stale or mismatched <c>actr_cabi</c> fails here with an actionable
    /// <see cref="ActrCabiException"/> instead of an opaque callback failure
    /// later. Safe to call multiple times — only the first call installs the
    /// runtime (subsequent calls still re-verify the versions).
    /// </summary>
    public static void InitRuntime()
    {
        VerifyNativeVersions();
        int code = SafeNative(() => Native.actr_init_runtime(), "actr_init_runtime");
        if (code != 0) throw new ActrCabiException(code, "actr_init_runtime failed");
    }

    public static void ShutdownRuntime()
    {
        int code = SafeNative(() => Native.actr_shutdown_runtime(), "actr_shutdown_runtime");
        if (code != 0) throw new ActrCabiException(code, "actr_shutdown_runtime failed");
    }

    /// <summary>
    /// Validate that the loaded <c>actr_cabi</c> native library is compatible
    /// with ActrSharp and that <paramref name="workloadName"/> is registered
    /// in its bundle. When <paramref name="expectedBundleId"/> is supplied,
    /// verifies that the linked bundle matches it. Checks ABI version, actr
    /// version, single-provider bundle, and workload registration. Returns the
    /// native diagnostics on success; throws <see cref="ActrCabiException"/>
    /// (with Expected/Actual/Rid/Bundle context) on any mismatch.
    /// </summary>
    public static ActrNativeInfo ValidateNative(string workloadName, string? expectedBundleId = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(workloadName);
        if (expectedBundleId is not null)
            ArgumentException.ThrowIfNullOrWhiteSpace(expectedBundleId);

        (uint abi, string actr) = VerifyNativeVersions();

        string? bundle = SafeNative(() => Native.BundleId(), "actr_cabi_bundle_id");
        uint bundleCount = SafeNative(() => Native.actr_cabi_bundle_count(), "actr_cabi_bundle_count");
        if (bundleCount > 1)
            throw new ActrCabiException(ActrCabiException.CodeBundleConflict,
                $"Multiple bundle IDs ({bundleCount}) are registered in this process; " +
                "only one actr_cabi provider is allowed. Statically combine workloads into a single cdylib.")
            { Actual = $"bundles={bundleCount}", Rid = CurrentRid, Bundle = bundle };

        if (expectedBundleId is not null && !string.Equals(bundle, expectedBundleId, StringComparison.Ordinal))
            throw new ActrCabiException(ActrCabiException.CodeBundleMismatch,
                $"Bundle ID mismatch: actr_cabi reports '{bundle ?? "<none>"}', " +
                $"but the host expects '{expectedBundleId}'.")
            {
                Expected = $"bundle={expectedBundleId}",
                Actual = $"bundle={bundle ?? "<none>"}",
                Rid = CurrentRid,
                Bundle = bundle,
            };

        byte[] nameBytes = Encoding.UTF8.GetBytes(workloadName);
        nint workloadToken = SafeNative(() => Native.WorkloadCreate(nameBytes), "actr_workload_create");
        if (workloadToken == nint.Zero)
            throw new ActrCabiException(ActrCabiException.CodeWorkloadNotFound,
                $"Workload '{workloadName}' is not registered in this actr_cabi bundle. " +
                "Ensure the native package was built with this workload and linked with " +
                "/WHOLEARCHIVE:actr_cabi (Windows) or -Wl,-all_load (macOS) so " +
                "export_workload! inventory entries survive dead-stripping.")
            { Expected = $"workload={workloadName}", Rid = CurrentRid, Bundle = bundle };
        // using-var releases the probe handle (a SafeHandle) at scope exit even
        // if ActrNativeInfo construction throws.
        using var probe = new ActrCabiHandle(workloadToken);
        return new ActrNativeInfo(abi, actr, bundle);
    }

    /// <summary>
    /// Query and verify the ABI + actr version of the loaded native library.
    /// Throws <see cref="ActrCabiException"/> on mismatch. Returns the values
    /// so callers can populate <see cref="ActrNativeInfo"/>.
    /// </summary>
    static (uint Abi, string Actr) VerifyNativeVersions()
    {
        uint actualAbi = SafeNative(() => Native.actr_cabi_abi_version(), "actr_cabi_abi_version");
        if (actualAbi != ExpectedAbiVersion)
            throw new ActrCabiException(ActrCabiException.CodeAbiMismatch,
                $"ABI version mismatch: actr_cabi reports ABI {actualAbi}, " +
                $"but ActrSharp {ExpectedActrVersion} expects ABI {ExpectedAbiVersion}.")
            { Expected = $"abi={ExpectedAbiVersion}", Actual = $"abi={actualAbi}", Rid = CurrentRid };

        string? actualActr = SafeNative(() => Native.ActrVersion(), "actr_cabi_actr_version");
        if (!string.Equals(actualActr, ExpectedActrVersion, StringComparison.Ordinal))
            throw new ActrCabiException(ActrCabiException.CodeActrMismatch,
                $"Actr version mismatch: actr_cabi was built from actr " +
                $"'{actualActr ?? "<null>"}', but ActrSharp is '{ExpectedActrVersion}'.")
            { Expected = $"actr={ExpectedActrVersion}", Actual = $"actr={actualActr ?? "<null>"}", Rid = CurrentRid };

        return (actualAbi, actualActr ?? "");
    }

    /// <summary>
    /// Run a native entry point, converting loader failures (missing DLL,
    /// wrong architecture, missing entry point) into an
    /// <see cref="ActrCabiException"/> with the process RID so the failure is
    /// actionable instead of an opaque <see cref="DllNotFoundException"/>.
    /// </summary>
    static T SafeNative<T>(Func<T> action, string entry)
    {
        try
        {
            return action();
        }
        catch (DllNotFoundException ex)
        {
            throw new ActrCabiException(ActrCabiException.CodeDllNotFound,
                $"The actr_cabi native library could not be loaded ({entry}). " +
                "Install a workload native package (ActrSharp.Native.<Workload>) that " +
                "matches this process architecture.",
                ex)
            { Actual = CurrentRid, Rid = CurrentRid };
        }
        catch (BadImageFormatException ex)
        {
            throw new ActrCabiException(ActrCabiException.CodeArchMismatch,
                $"The actr_cabi native library has an incompatible architecture ({entry}): {ex.Message}",
                ex)
            { Actual = CurrentRid, Rid = CurrentRid };
        }
        catch (EntryPointNotFoundException ex)
        {
            throw new ActrCabiException(ActrCabiException.CodeEntryMissing,
                $"The actr_cabi native library is missing the expected entry point '{entry}'. " +
                "The loaded actr_cabi is older than ActrSharp expects.",
                ex)
            { Actual = CurrentRid, Rid = CurrentRid };
        }
    }

    public async Task FromConfig(string path)
    {
        BeginWithoutHandle(NodeState.Empty, NodeState.Initializing);
        try
        {
            byte[] pathBytes = Encoding.UTF8.GetBytes(path);
            nint result = await WithCallback<nint>(userData =>
                _ = Native.NodeFromConfig(pathBytes, Trampolines.Handle, userData));
            Complete(NodeState.Initialized, new ActrCabiHandle(result));
        }
        catch
        {
            FailTransition();
            throw;
        }
    }

    /// <summary>
    /// Set the concrete identity used when a linked workload registers with
    /// Issuer. Call after <see cref="FromConfig"/> and before
    /// <see cref="LinkWorkload"/> using
    /// <c>publisher/program@version</c>.
    /// </summary>
    public void WithActorType(string actorType)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(actorType);

        ActrCabiHandle initHandle = BeginWithHandle(NodeState.Initialized, NodeState.Initializing);
        using (initHandle)
        {
            try
            {
                byte[] actorTypeBytes = Encoding.UTF8.GetBytes(actorType);
                nint result = SafeNative(
                    () => Native.NodeWithActorType(initHandle.Ptr, actorTypeBytes),
                    "actr_node_with_actor_type");
                if (result == nint.Zero)
                {
                    throw new ActrCabiException(
                        ActrCabiException.CodeGenericError,
                        $"invalid actor type or init handle: {actorType}");
                }
                Complete(NodeState.Initialized, new ActrCabiHandle(result));
            }
            catch
            {
                FailTransition();
                throw;
            }
        }
    }

    public async Task LinkWorkload(string name)
    {
        ActrCabiHandle initHandle = BeginWithHandle(NodeState.Initialized, NodeState.Linking);
        using (initHandle)
        {
            try
            {
                byte[] nameBytes = Encoding.UTF8.GetBytes(name);
                nint workloadToken = Native.WorkloadCreate(nameBytes);
                if (workloadToken == nint.Zero)
                    throw new ActrCabiException(ActrCabiException.CodeWorkloadNotFound, $"workload not found: {name}");

                using var workloadHandle = new ActrCabiHandle(workloadToken);
                nint result = await WithCallback<nint>(userData =>
                {
                    _ = Native.actr_node_link(
                        initHandle.Ptr,
                        workloadHandle.Ptr,
                        Trampolines.Handle,
                        userData);
                });
                Complete(NodeState.Attached, new ActrCabiHandle(result));
            }
            catch
            {
                FailTransition();
                throw;
            }
        }
    }

    public async Task Register(string? issuerEndpoint = null)
    {
        ActrCabiHandle attachedHandle = BeginWithHandle(NodeState.Attached, NodeState.Registering);
        using (attachedHandle)
        {
            try
            {
                byte[]? issuer = issuerEndpoint is null ? null : Encoding.UTF8.GetBytes(issuerEndpoint);
                nint result = await WithCallback<nint>(userData =>
                {
                    _ = Native.NodeRegister(
                        attachedHandle.Ptr,
                        issuer,
                        Trampolines.Handle,
                        userData);
                });
                Complete(NodeState.Registered, new ActrCabiHandle(result));
            }
            catch
            {
                FailTransition();
                throw;
            }
        }
    }

    public async Task Start()
    {
        ActrCabiHandle registeredHandle = BeginWithHandle(NodeState.Registered, NodeState.Starting);
        using (registeredHandle)
        {
            try
            {
                nint result = await WithCallback<nint>(userData =>
                {
                    _ = Native.actr_node_start(
                        registeredHandle.Ptr,
                        Trampolines.Handle,
                        userData);
                });
                Complete(NodeState.Started, new ActrCabiHandle(result, isRef: true));
            }
            catch
            {
                FailTransition();
                throw;
            }
        }
    }

    public Task<byte[]> Call(string routeKey, byte[] payload, long timeoutMs = 30_000)
    {
        ArgumentNullException.ThrowIfNull(payload);
        ActrCabiHandle refHandle = GetStartedHandle();
        byte[] routeKeyBytes = Encoding.UTF8.GetBytes(routeKey);
        return WithCallback<byte[]>(userData => WithDangerousRef(refHandle, ptr =>
            _ = Native.RefCall(
                ptr,
                routeKeyBytes,
                payload,
                timeoutMs,
                Trampolines.Call,
                userData)));
    }

    public Task<string> Discover(string targetType)
    {
        ActrCabiHandle refHandle = GetStartedHandle();
        byte[] targetTypeBytes = Encoding.UTF8.GetBytes(targetType);
        return WithCallback<string>(userData => WithDangerousRef(refHandle, ptr =>
            _ = Native.RefDiscover(
                ptr,
                targetTypeBytes,
                Trampolines.Discover,
                userData)));
    }

    // ── host bridge (process-global sinks + per-node stream listen) ────────

    /// <summary>
    /// The maximum number of copied stream chunks awaiting the managed
    /// handler. On overflow ActrSharp drops the newest chunk and increments
    /// <see cref="DroppedStreamChunkCount"/>; this bridge is therefore not a
    /// reliable-delivery acknowledgement, even for a reliable native stream.
    /// </summary>
    public const int StreamQueueCapacity = ActrHostBridge.StreamQueueCapacity;

    /// <summary>Total chunks dropped by ActrSharp's bounded managed queue.</summary>
    public static long DroppedStreamChunkCount => ActrHostBridge.DroppedStreamChunkCount;

    /// <summary>
    /// Register the process-global handler for explicitly listened stream
    /// chunks. Passing null clears the native sink and stops managed dispatch.
    /// Delegates are rooted by ActrSharp; the handler runs serially on a
    /// managed consumer task, never on a native runtime worker.
    /// </summary>
    public static void RegisterStreamSink(Action<ActrStreamChunk>? handler)
    {
        int code = ActrHostBridge.RegisterStreamHandler(handler, () => handler is null
            ? SafeNative(() => Native.RegisterStreamSinkNull(nint.Zero, nint.Zero), "actr_register_stream_sink")
            : SafeNative(() => Native.RegisterStreamSink(Trampolines.StreamSink, nint.Zero), "actr_register_stream_sink"));
        if (code != 0) throw new ActrCabiException(code, "actr_register_stream_sink failed");
    }

    /// <summary>
    /// Register the process-global handler for requests a Rust workload
    /// explicitly delegates with <c>ask_host</c>. Return the response payload;
    /// a throw maps to a host failure and a late response is ignored natively.
    /// Passing null clears the native sink.
    /// </summary>
    public static void RegisterReverseRpcHandler(Func<ActrReverseRpcRequest, Task<byte[]>>? handler)
    {
        int code = ActrHostBridge.RegisterReverseHandler(handler, () => handler is null
            ? SafeNative(() => Native.RegisterReverseRpcSinkNull(nint.Zero, nint.Zero), "actr_register_reverse_rpc_sink")
            : SafeNative(() => Native.RegisterReverseRpcSink(Trampolines.ReverseRpc, nint.Zero), "actr_register_reverse_rpc_sink"));
        if (code != 0) throw new ActrCabiException(code, "actr_register_reverse_rpc_sink failed");
    }

    /// <summary>
    /// Subscribe this started node to exactly <paramref name="streamId"/>.
    /// Only chunks for that id are forwarded to <see cref="RegisterStreamSink"/>.
    /// </summary>
    public Task ListenStream(string streamId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(streamId);
        ActrCabiHandle refHandle = GetStartedHandle();
        byte[] streamIdBytes = Encoding.UTF8.GetBytes(streamId);
        return WithCallback(userData => WithDangerousRef(refHandle, ptr =>
            _ = Native.StreamListen(ptr, streamIdBytes, Trampolines.Void, userData)));
    }

    /// <summary>Remove this node's explicit listener for <paramref name="streamId"/>.</summary>
    public Task UnlistenStream(string streamId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(streamId);
        ActrCabiHandle refHandle = GetStartedHandle();
        byte[] streamIdBytes = Encoding.UTF8.GetBytes(streamId);
        return WithCallback(userData => WithDangerousRef(refHandle, ptr =>
            _ = Native.StreamUnlisten(ptr, streamIdBytes, Trampolines.Void, userData)));
    }

    public void Shutdown()
    {
        ActrCabiHandle refHandle = GetStartedHandle();
        int code = 0;
        WithDangerousRef(refHandle, ptr => code = Native.actr_ref_shutdown(ptr));
        if (code != 0) throw new ActrCabiException(code, "actr_ref_shutdown failed");
    }

    public async Task WaitShutdown()
    {
        ActrCabiHandle refHandle = BeginWithHandle(NodeState.Started, NodeState.WaitingForShutdown);
        try
        {
            await WithCallback(userData => WithDangerousRef(refHandle, ptr =>
                _ = Native.actr_ref_wait_shutdown(ptr, Trampolines.Void, userData)));
            refHandle.Dispose();
            Complete(NodeState.Empty, null);
        }
        catch
        {
            Restore(NodeState.Started, refHandle);
            throw;
        }
    }

    public void Dispose()
    {
        ActrCabiHandle? handle;
        lock (_stateLock)
        {
            if (_state == NodeState.Disposed) return;
            _state = NodeState.Disposed;
            handle = _handle;
            _handle = null;
        }
        handle?.Dispose();
        GC.SuppressFinalize(this);
    }

    static TaskCompletionSource<T> NewCompletion<T>() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    static TaskCompletionSource NewCompletion() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    static Task<T> WithCallback<T>(Action<nint> fire)
    {
        var completion = NewCompletion<T>();
        nint key = ContinuationMap.Register(completion);
        try
        {
            fire(key);
        }
        catch (Exception error)
        {
            if (ContinuationMap.TryTake(key, out TaskCompletionSource<T>? pending))
                pending.TrySetException(error);
        }
        return completion.Task;
    }

    static Task WithCallback(Action<nint> fire)
    {
        var completion = NewCompletion();
        nint key = ContinuationMap.Register(completion);
        try
        {
            fire(key);
        }
        catch (Exception error)
        {
            if (ContinuationMap.TryTake(key, out TaskCompletionSource? pending))
                pending.TrySetException(error);
        }
        return completion.Task;
    }

    static void WithDangerousRef(ActrCabiHandle handle, Action<nint> action)
    {
        bool addedRef = false;
        try
        {
            handle.DangerousAddRef(ref addedRef);
            action(handle.DangerousGetHandle());
        }
        finally
        {
            if (addedRef) handle.DangerousRelease();
        }
    }

    void BeginWithoutHandle(NodeState expected, NodeState inProgress)
    {
        lock (_stateLock)
        {
            EnsureState(expected);
            _state = inProgress;
        }
    }

    ActrCabiHandle BeginWithHandle(NodeState expected, NodeState inProgress)
    {
        lock (_stateLock)
        {
            EnsureState(expected);
            ActrCabiHandle handle = _handle
                ?? throw new ActrCabiException(ActrCabiException.CodeHandleInvalid, "native handle is missing");
            _handle = null;
            _state = inProgress;
            return handle;
        }
    }

    ActrCabiHandle GetStartedHandle()
    {
        lock (_stateLock)
        {
            EnsureState(NodeState.Started);
            return _handle ?? throw new ActrCabiException(ActrCabiException.CodeHandleInvalid, "actr ref handle is missing");
        }
    }

    void EnsureState(NodeState expected)
    {
        if (_state != expected)
            throw new InvalidOperationException($"Expected actr-cabi state {expected}, found {_state}");
    }

    void Complete(NodeState newState, ActrCabiHandle? newHandle)
    {
        ActrCabiHandle? disposeAfter = null;
        lock (_stateLock)
        {
            if (_state == NodeState.Disposed)
            {
                disposeAfter = newHandle;
            }
            else
            {
                _state = newState;
                _handle = newHandle;
            }
        }
        disposeAfter?.Dispose();
    }

    void Restore(NodeState restoredState, ActrCabiHandle handle)
    {
        bool disposeAfter = false;
        lock (_stateLock)
        {
            if (_state == NodeState.Disposed)
            {
                disposeAfter = true;
            }
            else
            {
                _state = restoredState;
                _handle = handle;
            }
        }
        if (disposeAfter) handle.Dispose();
    }

    void FailTransition()
    {
        lock (_stateLock)
        {
            if (_state != NodeState.Disposed)
            {
                _state = NodeState.Empty;
                _handle = null;
            }
        }
    }
}
