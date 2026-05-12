//! Load shedding and backpressure for handling server overload.
//!
//! This module provides deterministic response handling when the server is under
//! heavy load. It uses atomic counters to track:
//! - `inflight`: Number of requests currently being processed
//! - `queued`: Number of requests waiting to be processed
//!
//! When limits are exceeded, requests receive a 503 Service Unavailable response
//! with a deterministic JSON error body.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct LoadShedConfig {
    pub max_inflight: Option<u64>,
    pub max_queue: Option<u64>,
}

impl Default for LoadShedConfig {
    fn default() -> Self {
        Self {
            max_inflight: Some(10000),
            max_queue: Some(1000),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadShedder {
    config: LoadShedConfig,
    inflight: Arc<AtomicU64>,
    queued: Arc<AtomicU64>,
}

impl LoadShedder {
    pub fn new(config: LoadShedConfig) -> Self {
        Self {
            config,
            inflight: Arc::new(AtomicU64::new(0)),
            queued: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn should_accept(&self) -> Result<RequestGuard, ()> {
        let queued = self.queued.load(Ordering::Relaxed);
        if let Some(max_queue) = self.config.max_queue
            && queued >= max_queue
        {
            return Err(());
        }

        let inflight = self.inflight.load(Ordering::Relaxed);
        if let Some(max_inflight) = self.config.max_inflight
            && inflight >= max_inflight
        {
            return Err(());
        }

        // Atomically check-and-decrement queue, atomically increment inflight
        // Use compare_exchange to avoid underflow
        loop {
            let current = self.queued.load(Ordering::Relaxed);
            if current == 0 {
                break;
            }
            if self
                .queued
                .compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        self.inflight.fetch_add(1, Ordering::Relaxed);

        Ok(RequestGuard {
            inflight: Arc::clone(&self.inflight),
        })
    }

    pub fn enqueue(&self) {
        self.queued.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inflight(&self) -> u64 {
        self.inflight.load(Ordering::Relaxed)
    }

    pub fn queued(&self) -> u64 {
        self.queued.load(Ordering::Relaxed)
    }

    pub fn is_enabled(&self) -> bool {
        self.config.max_inflight.is_some() || self.config.max_queue.is_some()
    }
}

pub struct RequestGuard {
    inflight: Arc<AtomicU64>,
}

impl RequestGuard {
    pub fn complete(&self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.complete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_accept_under_limits() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        };
        let shedder = LoadShedder::new(config);

        let guard = shedder.should_accept();
        assert!(guard.is_ok());
        assert_eq!(shedder.inflight(), 1);
        assert_eq!(shedder.queued(), 0);
    }

    #[test]
    fn should_reject_when_queue_full() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(0),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        let result = shedder.should_accept();
        assert!(result.is_err());
    }

    #[test]
    fn should_reject_when_inflight_full() {
        let config = LoadShedConfig {
            max_inflight: Some(1),
            max_queue: Some(10),
        };
        let shedder = LoadShedder::new(config);

        let _guard1 = shedder.should_accept().unwrap();
        let result = shedder.should_accept();
        assert!(result.is_err());
    }

    #[test]
    fn should_decrement_after_drop() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        };
        let shedder = LoadShedder::new(config);

        {
            let _guard = shedder.should_accept().unwrap();
            assert_eq!(shedder.inflight(), 1);
        }

        assert_eq!(shedder.inflight(), 0);
    }

    #[test]
    fn should_track_queue_correctly() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        shedder.enqueue();
        assert_eq!(shedder.queued(), 2);

        let _guard = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 1);
        assert_eq!(shedder.inflight(), 1);
    }

    #[test]
    fn should_disable_when_no_limits() {
        let config = LoadShedConfig {
            max_inflight: None,
            max_queue: None,
        };
        let shedder = LoadShedder::new(config);

        assert!(!shedder.is_enabled());

        let guard = shedder.should_accept();
        assert!(guard.is_ok());
    }

    #[test]
    fn should_accept_at_exact_threshold() {
        let config = LoadShedConfig {
            max_inflight: Some(5),
            max_queue: Some(10),
        };
        let shedder = LoadShedder::new(config);

        let mut guards: Vec<RequestGuard> = Vec::new();
        for _ in 0..5 {
            guards.push(shedder.should_accept().unwrap());
        }

        let result = shedder.should_accept();
        assert!(
            result.is_err(),
            "should reject when inflight reaches exact limit"
        );
        assert_eq!(shedder.inflight(), 5);
    }

    #[test]
    fn should_accept_request_after_previous_completes() {
        let config = LoadShedConfig {
            max_inflight: Some(1),
            max_queue: Some(5),
        };
        let shedder = LoadShedder::new(config);

        let guard1 = shedder.should_accept().unwrap();
        assert!(shedder.should_accept().is_err());

        drop(guard1);

        assert!(shedder.should_accept().is_ok());
    }

    #[test]
    fn should_handle_queue_only_limit() {
        let config = LoadShedConfig {
            max_inflight: None,
            max_queue: Some(2),
        };
        let shedder = LoadShedder::new(config);

        assert!(shedder.is_enabled());

        shedder.enqueue();
        shedder.enqueue();
        let result = shedder.should_accept();
        assert!(result.is_err(), "should reject when queue full");
    }

    #[test]
    fn should_handle_inflight_only_limit() {
        let config = LoadShedConfig {
            max_inflight: Some(1),
            max_queue: None,
        };
        let shedder = LoadShedder::new(config);

        assert!(shedder.is_enabled());

        let _guard = shedder.should_accept().unwrap();
        let result = shedder.should_accept();
        assert!(result.is_err(), "should reject when inflight full");
    }

    #[test]
    fn should_decrement_queue_when_request_accepted() {
        let config = LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(100),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        shedder.enqueue();
        shedder.enqueue();
        assert_eq!(shedder.queued(), 3);
        assert_eq!(shedder.inflight(), 0);

        let _guard = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 2);
        assert_eq!(shedder.inflight(), 1);
    }

    #[test]
    fn should_reject_when_inflight_at_limit_even_with_queue_space() {
        let config = LoadShedConfig {
            max_inflight: Some(2),
            max_queue: Some(100),
        };
        let shedder = LoadShedder::new(config);

        let _guard1 = shedder.should_accept().unwrap();
        let _guard2 = shedder.should_accept().unwrap();

        let result = shedder.should_accept();
        assert!(result.is_err(), "should reject when inflight at limit");
        assert_eq!(shedder.inflight(), 2);
    }

    #[test]
    fn should_process_queued_requests_in_fifo_order() {
        let config = LoadShedConfig {
            max_inflight: Some(1),
            max_queue: Some(10),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        shedder.enqueue();

        let _guard1 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 1);
        assert_eq!(shedder.inflight(), 1);

        drop(_guard1);
        assert_eq!(shedder.inflight(), 0);

        let _guard2 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 0);
        assert_eq!(shedder.inflight(), 1);
    }

    #[test]
    fn should_track_multiple_concurrent_requests() {
        let config = LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(5),
        };
        let shedder = LoadShedder::new(config);

        let mut guards: Vec<RequestGuard> = Vec::new();
        for _ in 0..10 {
            guards.push(shedder.should_accept().unwrap());
        }

        assert_eq!(shedder.inflight(), 10);
        assert!(shedder.should_accept().is_err());

        drop(guards.split_off(5));
        assert_eq!(shedder.inflight(), 5);

        guards.push(shedder.should_accept().unwrap());
        assert_eq!(shedder.inflight(), 6);
    }

    #[test]
    fn should_reset_to_zero_after_all_guards_dropped() {
        let config = LoadShedConfig {
            max_inflight: Some(3),
            max_queue: Some(3),
        };
        let shedder = LoadShedder::new(config);

        {
            let _g1 = shedder.should_accept().unwrap();
            let _g2 = shedder.should_accept().unwrap();
            let _g3 = shedder.should_accept().unwrap();
        }

        assert_eq!(shedder.inflight(), 0);
        assert_eq!(shedder.queued(), 0);
        assert!(shedder.should_accept().is_ok());
    }

    #[test]
    fn should_default_to_reasonable_limits() {
        let config = LoadShedConfig::default();
        assert_eq!(config.max_inflight, Some(10000));
        assert_eq!(config.max_queue, Some(1000));
    }
}
