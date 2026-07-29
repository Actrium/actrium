package io.actrium.actr.dsl

import io.actrium.actr.CleanupReason
import io.actrium.actr.ReconnectReason
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal interface NetworkMonitorLifecycle {
    fun stopMonitoring()

    fun onAppBackground()

    fun onAppForeground()

    fun cleanupConnections(reason: CleanupReason)

    fun forceReconnect(reason: ReconnectReason)

    fun triggerNetworkCheck()

    fun getCurrentNetworkStatus(): String
}

internal class MobileEventDeliveryGate {
    private val closed = AtomicBoolean(false)

    fun close() {
        closed.set(true)
    }

    suspend fun deliver(block: suspend () -> Unit) {
        if (closed.get()) return
        block()
    }

    fun rotated(): MobileEventDeliveryGate {
        close()
        return MobileEventDeliveryGate()
    }
}

internal class SerializedDeliveryQueue<T>(
    scope: CoroutineScope,
    dispatcher: CoroutineDispatcher = Dispatchers.IO,
    private val deliver: suspend (T) -> Unit,
) {
    private val channel = Channel<T>(Channel.UNLIMITED)

    init {
        val deliveryJob =
            scope.launch(dispatcher) {
                for (value in channel) {
                    deliver(value)
                }
            }
        deliveryJob.invokeOnCompletion { channel.close() }
    }

    fun enqueue(value: T): Boolean = channel.trySend(value).isSuccess

    fun close() {
        channel.close()
    }
}

internal class MonitorEventHandleCache<T : Any>(
    private val allocate: suspend () -> T?,
) {
    private val lock = Mutex()
    private var cached: T? = null

    suspend fun getOrCreate(): T? =
        lock.withLock {
            cached ?: allocate()?.also { cached = it }
        }
}

internal class NetworkMonitorLifecycleAdapter(
    private val monitor: NetworkMonitor,
) : NetworkMonitorLifecycle {
    override fun stopMonitoring() = monitor.stopMonitoring()

    override fun onAppBackground() = monitor.onAppBackground()

    override fun onAppForeground() = monitor.onAppForeground()

    override fun cleanupConnections(reason: CleanupReason) = monitor.cleanupConnections(reason)

    override fun forceReconnect(reason: ReconnectReason) = monitor.forceReconnect(reason)

    override fun triggerNetworkCheck() = monitor.triggerNetworkCheck()

    override fun getCurrentNetworkStatus(): String = monitor.getCurrentNetworkStatus()
}

internal class ManagedNetworkResources(
    val handle: NetworkEventHandle?,
    private val monitor: NetworkMonitorLifecycle?,
) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    override fun close() {
        if (!closed.compareAndSet(false, true)) {
            return
        }

        monitor?.stopMonitoring()
        handle?.close()
    }

    fun onAppBackground() {
        if (closed.get()) return
        monitor?.onAppBackground()
    }

    fun onAppForeground() {
        if (closed.get()) return
        monitor?.onAppForeground()
    }

    fun cleanupConnections(reason: CleanupReason) {
        if (closed.get()) return
        monitor?.cleanupConnections(reason)
    }

    fun forceReconnect(reason: ReconnectReason) {
        if (closed.get()) return
        monitor?.forceReconnect(reason)
    }

    fun triggerNetworkCheck() {
        if (closed.get()) return
        monitor?.triggerNetworkCheck()
    }

    fun getCurrentNetworkStatus(): String? =
        if (closed.get()) {
            null
        } else {
            monitor?.getCurrentNetworkStatus()
        }
}
