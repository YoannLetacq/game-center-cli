use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sliding-window auth rate limiter keyed by `(peer_ip, username_lowercase)`.
///
/// Window: `WINDOW_SECS` seconds, max `MAX_ATTEMPTS` attempts per window.
/// Entries older than `GC_SECS` seconds are pruned on every check (piggy-backed GC).
const WINDOW_SECS: u64 = 60;
const MAX_ATTEMPTS: usize = 10;
const GC_SECS: u64 = 300; // prune entries silent for 5 minutes

type RateLimitMap = HashMap<(IpAddr, String), VecDeque<Instant>>;

#[derive(Clone, Default)]
pub struct AuthRateLimiter {
    inner: Arc<Mutex<RateLimitMap>>,
}

impl AuthRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the attempt is allowed (within the window limit).
    /// Returns `false` if the caller should reject with "too many attempts".
    pub fn check_and_record(&self, ip: IpAddr, username: &str) -> bool {
        let key = (ip, username.to_lowercase());
        let now = Instant::now();
        let window = Duration::from_secs(WINDOW_SECS);
        let gc_horizon = Duration::from_secs(GC_SECS);

        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Piggy-backed GC: remove entries that have been idle for GC_SECS.
        map.retain(|_, deque| {
            deque
                .back()
                .is_some_and(|t| now.duration_since(*t) < gc_horizon)
        });

        let deque = map.entry(key).or_default();

        // Drop timestamps outside the sliding window.
        while deque
            .front()
            .is_some_and(|t| now.duration_since(*t) >= window)
        {
            deque.pop_front();
        }

        if deque.len() >= MAX_ATTEMPTS {
            return false;
        }

        deque.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    #[test]
    fn allows_up_to_limit() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            assert!(limiter.check_and_record(ip(), "alice"));
        }
    }

    #[test]
    fn blocks_after_limit() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            limiter.check_and_record(ip(), "alice");
        }
        assert!(!limiter.check_and_record(ip(), "alice"));
    }

    #[test]
    fn different_users_independent() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            limiter.check_and_record(ip(), "alice");
        }
        // "bob" has its own counter — must still be allowed
        assert!(limiter.check_and_record(ip(), "bob"));
    }

    #[test]
    fn case_insensitive_username() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            limiter.check_and_record(ip(), "Alice");
        }
        // "alice" and "Alice" are the same key
        assert!(!limiter.check_and_record(ip(), "alice"));
    }
}
