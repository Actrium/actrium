//! Explicit policy-clock boundary for the per-peer Controller.
//!
//! The reducer never reads this clock: deadlines enter it only as semantic
//! timer facts. Production uses audited Tokio sleeps, while T1 uses a
//! per-fixture [`ManualPolicyClock`] that advances independently of wall time
//! and of every other test.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::timer::TimerDefinition;

pub(crate) type PolicySleep = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Scheduling seam used by the Controller shell. Policy decisions remain in
/// the pure reducer; this trait only turns an explicit duration into a future.
pub(crate) trait PolicyClock: Send + Sync {
    fn now(&self) -> Instant;

    fn sleep(&self, definition: TimerDefinition, duration: Duration) -> PolicySleep;
}

#[derive(Debug, Default)]
pub(crate) struct SystemPolicyClock;

impl PolicyClock for SystemPolicyClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, definition: TimerDefinition, duration: Duration) -> PolicySleep {
        Box::pin(crate::timer::sleep(definition, duration))
    }
}

pub(crate) fn system_policy_clock() -> Arc<dyn PolicyClock> {
    Arc::new(SystemPolicyClock)
}

/// T1-only monotone clock. `advance` wakes every deadline at or before the new
/// instant without changing Tokio's process-wide test clock.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct ManualPolicyClock {
    now_tx: tokio::sync::watch::Sender<Instant>,
}

#[cfg(test)]
impl ManualPolicyClock {
    pub(crate) fn new(start: Instant) -> Self {
        let (now_tx, _) = tokio::sync::watch::channel(start);
        Self { now_tx }
    }

    pub(crate) fn advance(&self, duration: Duration) -> Instant {
        let next = *self.now_tx.borrow() + duration;
        self.now_tx.send_replace(next);
        next
    }
}

#[cfg(test)]
impl Default for ManualPolicyClock {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

#[cfg(test)]
impl PolicyClock for ManualPolicyClock {
    fn now(&self) -> Instant {
        *self.now_tx.borrow()
    }

    fn sleep(&self, _definition: TimerDefinition, duration: Duration) -> PolicySleep {
        let deadline = self.now() + duration;
        let mut now_rx = self.now_tx.subscribe();
        Box::pin(async move {
            loop {
                if *now_rx.borrow_and_update() >= deadline {
                    return;
                }
                if now_rx.changed().await.is_err() {
                    return;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manual_clock_releases_only_matching_deadline() {
        let clock = ManualPolicyClock::default();
        let mut sleep = clock.sleep(
            crate::timer::ids::WEBRTC_ICE_RESTART_WAIT,
            crate::wire::webrtc::rtc::recovery::ICE_RESTART_ATTEMPT_TIMEOUT,
        );

        clock.advance(
            crate::wire::webrtc::rtc::recovery::ICE_RESTART_ATTEMPT_TIMEOUT
                - Duration::from_secs(1),
        );
        let readiness =
            std::future::poll_fn(|cx| std::task::Poll::Ready(sleep.as_mut().poll(cx))).await;
        assert!(
            readiness.is_pending(),
            "deadline must remain pending before the exact instant"
        );

        clock.advance(Duration::from_secs(1));
        sleep.await;
    }
}
