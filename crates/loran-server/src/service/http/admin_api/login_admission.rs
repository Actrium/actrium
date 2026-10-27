//! Process-wide admission survives router reloads and bounds both queued and
//! running Argon2 jobs. A cancelled HTTP request cannot release a running job's permit.
use platform::config::AdminLoginLimits;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_SOURCES: usize = 4096;
const SOURCE_TTL: Duration = Duration::from_secs(600);

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Bucket {
    fn new(burst: u32, now: Instant) -> Self {
        Self {
            tokens: burst.into(),
            updated: now,
        }
    }
    fn take(&mut self, rate: u32, burst: u32, now: Instant) -> Result<(), u64> {
        let per_second = f64::from(rate) / 60.0;
        self.tokens = (self.tokens + now.duration_since(self.updated).as_secs_f64() * per_second)
            .min(f64::from(burst));
        self.updated = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            Err(((1.0 - self.tokens) / per_second).ceil().max(1.0) as u64)
        }
    }
}

#[derive(Default)]
struct AdmissionState {
    sources: HashMap<IpAddr, Bucket>,
    global: Option<Bucket>,
    running: usize,
}

#[derive(Default)]
pub(super) struct LoginAdmission {
    state: Mutex<AdmissionState>,
}

pub(super) struct Permit {
    admission: Arc<LoginAdmission>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.admission
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .running -= 1;
    }
}

impl LoginAdmission {
    pub(super) fn process() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<LoginAdmission>> = OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(Self::default())).clone()
    }

    pub(super) fn try_acquire(
        self: &Arc<Self>,
        ip: IpAddr,
        limits: &AdminLoginLimits,
    ) -> Result<Permit, u64> {
        self.acquire_at(ip, limits, Instant::now())
    }

    fn acquire_at(
        self: &Arc<Self>,
        ip: IpAddr,
        limits: &AdminLoginLimits,
        now: Instant,
    ) -> Result<Permit, u64> {
        if limits.per_ip_per_minute == 0
            || limits.per_ip_burst == 0
            || limits.global_per_minute == 0
            || limits.global_burst == 0
            || limits.max_concurrent == 0
        {
            return Err(1);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Bound work on arbitrary source addresses before allocating a bucket.
        state
            .global
            .get_or_insert_with(|| Bucket::new(limits.global_burst, now))
            .take(limits.global_per_minute, limits.global_burst, now)?;
        if !state.sources.contains_key(&ip) && state.sources.len() >= MAX_SOURCES {
            state
                .sources
                .retain(|_, bucket| now.duration_since(bucket.updated) < SOURCE_TTL);
            if state.sources.len() >= MAX_SOURCES {
                return Err(1);
            }
        }
        state
            .sources
            .entry(ip)
            .or_insert_with(|| Bucket::new(limits.per_ip_burst, now))
            .take(limits.per_ip_per_minute, limits.per_ip_burst, now)?;
        if state.running >= limits.max_concurrent {
            return Err(1);
        }
        state.running += 1;
        Ok(Permit {
            admission: self.clone(),
        })
    }
}

pub(super) async fn verify(password: String, phc: String, permit: Permit) -> bool {
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        PasswordHash::new(&phc).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ip(n: u8) -> IpAddr {
        IpAddr::from([192, 0, 2, n])
    }

    #[test]
    fn source_and_global_buckets_refill_and_bound_distributed_attempts() {
        let gate = Arc::new(LoginAdmission::default());
        let limits = AdminLoginLimits::default();
        let now = Instant::now();
        for _ in 0..10 {
            drop(gate.acquire_at(ip(1), &limits, now).unwrap());
        }
        assert_eq!(gate.acquire_at(ip(1), &limits, now).err(), Some(2));
        // The denied source attempt also consumes a global token.
        for _ in 0..9 {
            drop(gate.acquire_at(ip(2), &limits, now).unwrap());
        }
        assert_eq!(gate.acquire_at(ip(3), &limits, now).err(), Some(1));
        drop(
            gate.acquire_at(ip(1), &limits, now + Duration::from_secs(2))
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocking_job_holds_capacity_after_request_cancellation() {
        let gate = Arc::new(LoginAdmission::default());
        let limits = AdminLoginLimits {
            max_concurrent: 1,
            ..Default::default()
        };
        let permit = gate.try_acquire(ip(1), &limits).unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        task.abort(); // abort cannot stop an already-running blocking task
        assert!(gate.try_acquire(ip(2), &limits).is_err());
        // This single-thread executor remains responsive while the blocking job waits.
        tokio::time::timeout(Duration::from_secs(1), async {
            tokio::task::yield_now().await
        })
        .await
        .unwrap();
        release.send(()).unwrap();
        task.await.unwrap();
        assert!(gate.try_acquire(ip(2), &limits).is_ok());
    }

    #[test]
    fn source_table_remains_bounded_and_reclaims_idle_entries() {
        let gate = Arc::new(LoginAdmission::default());
        let limits = AdminLoginLimits {
            global_burst: 10000,
            ..Default::default()
        };
        let now = Instant::now();
        for i in 0..MAX_SOURCES {
            drop(
                gate.acquire_at(
                    IpAddr::V4(std::net::Ipv4Addr::from((i as u32) + 1)),
                    &limits,
                    now,
                )
                .unwrap(),
            );
        }
        assert!(gate.acquire_at(ip(1), &limits, now).is_err());
        drop(gate.acquire_at(ip(1), &limits, now + SOURCE_TTL).unwrap());
        assert_eq!(gate.state.lock().unwrap().sources.len(), 1);
    }
}
