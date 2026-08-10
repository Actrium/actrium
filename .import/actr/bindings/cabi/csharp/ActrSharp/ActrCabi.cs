// ActrSharp — C# wrapper over the actr-cabi C ABI.
//
// The native async API reports completion through callbacks. This wrapper
// roots the callback delegates, copies callback-owned buffers synchronously,
// and completes .NET tasks asynchronously to avoid running managed
// continuations on an actr Tokio worker or inside a reverse P/Invoke frame.

using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using System.Text;

namespace ActrSharp;

public sealed class ActrCabiException(int code, string message) : Exception(message)
{
    public int Code { get; } = code;

    internal static ActrCabiException FromCallback(int code, nint errorPtr, nuint errorLen)
    {
        string message = errorPtr == nint.Zero || errorLen == 0
            ? $"actr-cabi error {code}"
            : PtrToUtf8(errorPtr, errorLen);
        return new ActrCabiException(code, message);
    }

    internal static string PtrToUtf8(nint ptr, nuint len)
    {
        if (len > int.MaxValue)
            throw new ActrCabiException(-50, "native UTF-8 buffer is too large");
        var bytes = new byte[(int)len];
        Marshal.Copy(ptr, bytes, 0, bytes.Length);
        return Encoding.UTF8.GetString(bytes);
    }
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

static class Trampolines
{
    // Static fields keep the delegates and their unmanaged thunks alive for the
    // lifetime of the process.
    public static readonly ActrHandleCallback Handle = HandleImpl;
    public static readonly ActrCallCallback Call = CallImpl;
    public static readonly ActrDiscoverCallback Discover = DiscoverImpl;
    public static readonly ActrVoidCallback Void = VoidImpl;

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
                    ? new ActrCabiException(-21, "native callback returned a null handle")
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
                completion.TrySetException(new ActrCabiException(-10, "null response buffer"));
                return;
            }
            if (responseLen > int.MaxValue)
            {
                completion.TrySetException(new ActrCabiException(-50, "native response is too large"));
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
                completion.TrySetException(new ActrCabiException(-10, "null or empty actor ID"));
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
}

// ── Native imports ──────────────────────────────────────────────────────────

static unsafe partial class Native
{
    [LibraryImport("actr_cabi")]
    internal static partial int actr_init_runtime();

    [LibraryImport("actr_cabi")]
    internal static partial int actr_shutdown_runtime();

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
        byte* ais,
        nuint aisLen,
        [MarshalAs(UnmanagedType.FunctionPtr)] ActrHandleCallback callback,
        nint userData);

    internal static int NodeRegister(
        nint attachedHandle,
        byte[]? ais,
        ActrHandleCallback callback,
        nint userData)
    {
        fixed (byte* aisPtr = ais)
            return NodeRegisterImport(attachedHandle, aisPtr, (nuint)(ais?.Length ?? 0), callback, userData);
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

    public static void InitRuntime()
    {
        int code = Native.actr_init_runtime();
        if (code != 0) throw new ActrCabiException(code, "actr_init_runtime failed");
    }

    public static void ShutdownRuntime()
    {
        int code = Native.actr_shutdown_runtime();
        if (code != 0) throw new ActrCabiException(code, "actr_shutdown_runtime failed");
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
                    throw new ActrCabiException(-20, $"workload not found: {name}");

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

    public async Task Register(string? aisEndpoint = null)
    {
        ActrCabiHandle attachedHandle = BeginWithHandle(NodeState.Attached, NodeState.Registering);
        using (attachedHandle)
        {
            try
            {
                byte[]? ais = aisEndpoint is null ? null : Encoding.UTF8.GetBytes(aisEndpoint);
                nint result = await WithCallback<nint>(userData =>
                {
                    _ = Native.NodeRegister(
                        attachedHandle.Ptr,
                        ais,
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
                ?? throw new ActrCabiException(-21, "native handle is missing");
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
            return _handle ?? throw new ActrCabiException(-21, "actr ref handle is missing");
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
