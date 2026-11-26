use crate::{Authenticator, Principal, Queued, RelayLimits};
use axum::extract::ws::Message;
use bytes::Bytes;
use loran_relay_protocol::{CloseReason, ErrorCode, MAX_IDENTIFIER, ServerControl};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

type Key = (String, String);

pub(crate) struct Peer {
    pub principal: Principal,
    pub session: u64,
    pub tx: mpsc::Sender<Queued>,
    pub budget: Arc<Semaphore>,
    pub cancel: CancellationToken,
    pub channels: HashSet<u64>,
}

impl Peer {
    fn enqueue(&self, message: Message) {
        let bytes = match &message {
            Message::Text(text) => text.len(),
            Message::Binary(bytes) => bytes.len(),
            _ => 125,
        };
        let Ok(permit) = self.budget.clone().try_acquire_many_owned(bytes as u32) else {
            self.cancel.cancel();
            return;
        };
        if self
            .tx
            .try_send(Queued {
                message,
                _bytes: permit,
            })
            .is_err()
        {
            self.cancel.cancel();
        }
    }

    fn control(&self, message: ServerControl) {
        self.enqueue(Message::Text(
            serde_json::to_string(&message)
                .expect("serializable relay control")
                .into(),
        ));
    }

    fn error(&self, request: Option<u64>, code: ErrorCode) {
        self.control(ServerControl::Error { request, code });
    }
}

struct Channel {
    source: Key,
    target: Key,
    request: u64,
    target_request: Option<u64>,
    created: Instant,
    accepted: bool,
}

#[derive(Default)]
pub(crate) struct Registry {
    pub peers: HashMap<Key, Peer>,
    channels: HashMap<u64, Channel>,
    next_id: u64,
}

impl Registry {
    fn id(&mut self) -> u64 {
        // Keep JSON channel/session numbers exactly representable by JS too.
        self.next_id = self
            .next_id
            .checked_add(1)
            .filter(|id| *id <= MAX_IDENTIFIER)
            .expect("relay identifier space exhausted");
        self.next_id
    }

    pub fn current(&self, key: &Key, session: u64) -> bool {
        self.peers
            .get(key)
            .is_some_and(|peer| peer.session == session)
    }

    pub fn register(&mut self, mut peer: Peer) -> u64 {
        let key = (
            peer.principal.namespace.clone(),
            peer.principal.endpoint.clone(),
        );
        if let Some(previous) = self.peers.get(&key) {
            let old_session = previous.session;
            previous.cancel.cancel();
            self.remove(&key, old_session);
        }
        let session = self.id();
        peer.session = session;
        peer.control(ServerControl::Welcome { session });
        self.peers.insert(key, peer);
        session
    }

    pub fn remove(&mut self, key: &Key, session: u64) {
        if !self.current(key, session) {
            return;
        }
        if let Some(peer) = self.peers.remove(key) {
            for channel in peer.channels {
                self.finish(channel, CloseReason::PeerLeft);
            }
        }
    }

    pub fn admission_pair(&self, key: &Key, endpoint: &str) -> Option<(Principal, Principal, u64)> {
        let target_key = (key.0.clone(), endpoint.to_owned());
        if target_key == *key {
            return None;
        }
        let source = self.peers.get(key)?;
        let target = self
            .peers
            .get(&target_key)
            .filter(|peer| !peer.cancel.is_cancelled())?;
        Some((
            source.principal.clone(),
            target.principal.clone(),
            target.session,
        ))
    }

    pub fn pending_pair(&self, key: &Key, id: u64) -> Option<(Principal, Principal, u64, u64)> {
        let channel = self
            .channels
            .get(&id)
            .filter(|channel| channel.target == *key && !channel.accepted)?;
        let source = self
            .peers
            .get(&channel.source)
            .filter(|peer| !peer.cancel.is_cancelled())?;
        let target = self
            .peers
            .get(&channel.target)
            .filter(|peer| !peer.cancel.is_cancelled())?;
        Some((
            source.principal.clone(),
            target.principal.clone(),
            source.session,
            target.session,
        ))
    }

    pub fn connect(
        &mut self,
        key: &Key,
        request: u64,
        endpoint: &str,
        limits: &RelayLimits,
        auth: &dyn Authenticator,
        expected_target_session: Option<u64>,
    ) {
        let target_key = (key.0.clone(), endpoint.to_owned());
        let Some(source) = self.peers.get(key) else {
            return;
        };
        let Some(target) = self
            .peers
            .get(&target_key)
            .filter(|peer| !peer.cancel.is_cancelled())
        else {
            source.error(Some(request), ErrorCode::PeerUnavailable);
            return;
        };
        if target_key == *key
            || Some(target.session) != expected_target_session
            || !auth.authorize(&source.principal, &target.principal)
        {
            source.error(Some(request), ErrorCode::PeerUnavailable);
            return;
        }
        // Simultaneous dials share one channel. The receiver still has to
        // install its receive path and Accept; a reverse Connect is not proof
        // of readiness. An established channel can be reused by either end.
        if let Some((&id, existing)) = self.channels.iter_mut().find(|(_, channel)| {
            (channel.source == *key && channel.target == target_key)
                || (channel.target == *key && channel.source == target_key)
        }) {
            if existing.accepted {
                source.control(ServerControl::Connected {
                    channel: id,
                    request: Some(request),
                    peer: endpoint.to_owned(),
                    metadata: target.principal.metadata.clone(),
                });
            } else if existing.target == *key && existing.target_request.is_none() {
                existing.target_request = Some(request);
            } else {
                source.error(Some(request), ErrorCode::InvalidMessage);
            }
            return;
        }
        if source.channels.len() >= limits.max_channels_per_endpoint
            || target.channels.len() >= limits.max_channels_per_endpoint
        {
            source.error(Some(request), ErrorCode::Capacity);
            return;
        }
        if source.channels.iter().any(|id| {
            self.channels
                .get(id)
                .is_some_and(|channel| channel.source == *key && channel.request == request)
        }) {
            source.error(Some(request), ErrorCode::InvalidMessage);
            return;
        }
        let metadata = source.principal.metadata.clone();
        let id = self.id();
        self.channels.insert(
            id,
            Channel {
                source: key.clone(),
                target: target_key.clone(),
                request,
                target_request: None,
                created: Instant::now(),
                accepted: false,
            },
        );
        self.peers.get_mut(key).unwrap().channels.insert(id);
        let target = self.peers.get_mut(&target_key).unwrap();
        target.channels.insert(id);
        target.control(ServerControl::Incoming {
            channel: id,
            peer: key.1.clone(),
            metadata,
        });
    }

    pub fn accept(
        &mut self,
        key: &Key,
        id: u64,
        expected: Option<(u64, u64)>,
        authorized: bool,
        auth: &dyn Authenticator,
    ) {
        let Some((source, target, source_session, target_session)) = self.pending_pair(key, id)
        else {
            if let Some(peer) = self.peers.get(key) {
                peer.error(None, ErrorCode::InvalidChannel);
            }
            return;
        };
        if expected != Some((source_session, target_session))
            || !authorized
            || !auth.authorize(&source, &target)
        {
            self.finish(id, CloseReason::Rejected);
            return;
        }
        let channel = self
            .channels
            .get_mut(&id)
            .expect("pending channel remains registered");
        channel.accepted = true;
        let source = &self.peers[&channel.source];
        let target = &self.peers[&channel.target];
        // Each endpoint receives Connected before any later Data on its queue.
        source.control(ServerControl::Connected {
            channel: id,
            request: Some(channel.request),
            peer: channel.target.1.clone(),
            metadata: target.principal.metadata.clone(),
        });
        target.control(ServerControl::Connected {
            channel: id,
            request: channel.target_request,
            peer: channel.source.1.clone(),
            metadata: source.principal.metadata.clone(),
        });
    }

    pub fn reject(&mut self, key: &Key, id: u64) {
        if self
            .channels
            .get(&id)
            .is_some_and(|channel| channel.target == *key && !channel.accepted)
        {
            self.finish(id, CloseReason::Rejected);
        } else if let Some(peer) = self.peers.get(key) {
            peer.error(None, ErrorCode::InvalidChannel);
        }
    }

    pub fn close_channel(&mut self, key: &Key, id: u64) {
        if self
            .channels
            .get(&id)
            .is_some_and(|channel| channel.source == *key || channel.target == *key)
        {
            self.finish(id, CloseReason::Closed);
        } else if let Some(peer) = self.peers.get(key) {
            peer.error(None, ErrorCode::InvalidChannel);
        }
    }

    pub fn forward(&self, key: &Key, frame: Bytes, auth: &dyn Authenticator) {
        let id = u64::from_be_bytes(frame[..8].try_into().unwrap());
        let Some(channel) = self
            .channels
            .get(&id)
            .filter(|channel| channel.source == *key || channel.target == *key)
        else {
            if let Some(peer) = self.peers.get(key) {
                peer.error(None, ErrorCode::InvalidChannel);
            }
            return;
        };
        if !channel.accepted {
            self.peers[key].error(None, ErrorCode::NotReady);
            return;
        }
        let target = if channel.source == *key {
            &channel.target
        } else {
            &channel.source
        };
        if let (Some(source), Some(peer)) = (self.peers.get(key), self.peers.get(target))
            && auth.active(&source.principal)
            && auth.active(&peer.principal)
        {
            peer.enqueue(Message::Binary(frame));
        }
    }

    fn finish(&mut self, id: u64, reason: CloseReason) {
        if let Some(channel) = self.channels.remove(&id) {
            for key in [&channel.source, &channel.target] {
                if let Some(peer) = self.peers.get_mut(key) {
                    peer.channels.remove(&id);
                    peer.control(ServerControl::Closed {
                        channel: id,
                        reason,
                        request: if channel.accepted {
                            None
                        } else if key == &channel.source {
                            Some(channel.request)
                        } else {
                            channel.target_request
                        },
                    });
                }
            }
        }
    }

    pub fn expire_pending(&mut self, key: &Key, timeout: Duration) {
        let expired: Vec<_> = self
            .peers
            .get(key)
            .into_iter()
            .flat_map(|peer| peer.channels.iter())
            .filter(|id| {
                self.channels.get(id).is_some_and(|channel| {
                    !channel.accepted && channel.created.elapsed() >= timeout
                })
            })
            .copied()
            .collect();
        for id in expired {
            self.finish(id, CloseReason::TimedOut);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthError;
    use async_trait::async_trait;
    struct Auth;
    #[async_trait]
    impl Authenticator for Auth {
        async fn authenticate(&self, _: &str, _: &str) -> Result<Principal, AuthError> {
            unreachable!()
        }
    }
    fn peer(name: &str, queue: usize, bytes: usize) -> (Peer, mpsc::Receiver<Queued>) {
        let (tx, rx) = mpsc::channel(queue);
        (
            Peer {
                principal: Principal {
                    namespace: "test".into(),
                    endpoint: name.into(),
                    metadata: String::new(),
                    authorization_context: String::new(),
                    expires_at: None,
                },
                session: 0,
                tx,
                budget: Arc::new(Semaphore::new(bytes)),
                cancel: CancellationToken::new(),
                channels: HashSet::new(),
            },
            rx,
        )
    }

    #[test]
    fn a_slow_consumer_exhausts_its_byte_budget_without_cancelling_other_peers() {
        let mut registry = Registry::default();
        let (a, mut a_rx) = peer("a", 16, 65536);
        let (b, mut b_rx) = peer("b", 16, 32768);
        let slow_cancel = b.cancel.clone();
        let (c, mut c_rx) = peer("c", 16, 65536);
        let fast_cancel = c.cancel.clone();
        let a_session = registry.register(a);
        let b_session = registry.register(b);
        let c_session = registry.register(c);
        drop(a_rx.try_recv().unwrap());
        drop(b_rx.try_recv().unwrap());
        drop(c_rx.try_recv().unwrap());
        let a_key = ("test".into(), "a".into());
        let b_key = ("test".into(), "b".into());
        registry.connect(
            &a_key,
            1,
            "b",
            &RelayLimits::default(),
            &Auth,
            Some(b_session),
        );
        let incoming = b_rx.try_recv().unwrap();
        let Message::Text(text) = &incoming.message else {
            panic!()
        };
        let ServerControl::Incoming { channel, .. } = serde_json::from_str(text).unwrap() else {
            panic!()
        };
        drop(incoming);
        registry.accept(&b_key, channel, Some((a_session, b_session)), true, &Auth);
        drop(a_rx.try_recv().unwrap());
        drop(b_rx.try_recv().unwrap());
        let frame = loran_relay_protocol::encode_data(channel, &vec![1; 32760]).unwrap();
        registry.forward(&a_key, frame, &Auth);
        registry.forward(
            &a_key,
            loran_relay_protocol::encode_data(channel, b"overflow").unwrap(),
            &Auth,
        );
        assert!(slow_cancel.is_cancelled());
        assert!(!fast_cancel.is_cancelled());
        registry.connect(
            &a_key,
            2,
            "c",
            &RelayLimits::default(),
            &Auth,
            Some(c_session),
        );
        let Message::Text(text) = c_rx.try_recv().unwrap().message else {
            panic!()
        };
        assert!(matches!(
            serde_json::from_str::<ServerControl>(&text).unwrap(),
            ServerControl::Incoming { .. }
        ));
    }
}
