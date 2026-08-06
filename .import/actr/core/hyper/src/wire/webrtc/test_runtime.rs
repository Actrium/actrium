use std::{
    future::Future,
    io::{self, IoSliceMut},
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    TokioRuntime, Transmit,
};

#[cfg(feature = "test-utils")]
pub(crate) fn packet_filter_runtime(blocked: Arc<AtomicBool>) -> Arc<dyn Runtime> {
    Arc::new(PacketFilterRuntime {
        inner: TokioRuntime,
        blocked: Some(blocked),
        #[cfg(test)]
        reactor_spawn_probe: None,
    })
}

#[cfg(test)]
pub(crate) fn reactor_probe_runtime(probe: Arc<AtomicBool>) -> Arc<dyn Runtime> {
    Arc::new(PacketFilterRuntime {
        inner: TokioRuntime,
        blocked: None,
        reactor_spawn_probe: Some(probe),
    })
}

#[derive(Debug)]
struct PacketFilterRuntime {
    inner: TokioRuntime,
    blocked: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    reactor_spawn_probe: Option<Arc<AtomicBool>>,
}

impl Runtime for PacketFilterRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        #[cfg(test)]
        if let Some(probe) = &self.reactor_spawn_probe {
            probe.store(true, Ordering::Release);
        }
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        let inner = self.inner.wrap_udp_socket(socket)?;
        Ok(match &self.blocked {
            Some(blocked) => Arc::new(PacketFilterUdpSocket {
                inner,
                blocked: Arc::clone(blocked),
            }),
            None => inner,
        })
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        self.inner.wrap_tcp_listener(listener)
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        self.inner.connect_tcp(remote_addr)
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        self.inner.resolve_host(host)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        self.inner.block_on(future);
    }

    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.yield_now()
    }

    fn name(&self) -> &'static str {
        "tokio-packet-filter"
    }
}

#[derive(Debug)]
struct PacketFilterUdpSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    blocked: Arc<AtomicBool>,
}

impl AsyncUdpSocket for PacketFilterUdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        if self.blocked.load(Ordering::Acquire) {
            Poll::Ready(Ok(transmit.contents.len()))
        } else {
            self.inner.poll_send(cx, transmit)
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        if !self.blocked.load(Ordering::Acquire) {
            return self.inner.poll_recv(cx, bufs, meta);
        }

        match self.inner.poll_recv(cx, bufs, meta) {
            Poll::Ready(Ok(_)) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn max_gso_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.inner.max_gro_segments()
    }
}
