//! Integration tests for reload/shutdown interactions with in-flight jobs
//!
//! Tests verify:
//! - Graceful shutdown waits for in-flight jobs to complete
//! - Reload properly handles in-flight jobs
//! - New requests are rejected during shutdown/reload phases
//! - In-flight requests can complete during draining phase
//! - Drain timeout handling with in-flight jobs

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rover_server::{
    LifecycleConfig, LifecycleManager, LifecyclePhase, LoadShedConfig, LoadShedder, RequestGuard,
};

/// Simulates a server that combines lifecycle management with load shedding
struct SimulatedServer {
    lifecycle: LifecycleManager,
    shedder: Arc<LoadShedder>,
    drain_timeout_ms: u64,
    drain_started_at: Option<Instant>,
    shutdown_complete: Arc<AtomicBool>,
    active_requests: Arc<AtomicU64>,
}

impl SimulatedServer {
    fn new(drain_timeout_ms: u64, shedder: Arc<LoadShedder>) -> Self {
        let config = LifecycleConfig {
            enabled: true,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: drain_timeout_ms / 1000,
            reload_on_signal: false,
        };

        let mut server = Self {
            lifecycle: LifecycleManager::with_config(config),
            shedder,
            drain_timeout_ms,
            drain_started_at: None,
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            active_requests: Arc::new(AtomicU64::new(0)),
        };

        server.lifecycle.transition_to(LifecyclePhase::Running);
        server
    }

    fn accept_request(&self) -> Option<RequestHandle> {
        if !self.lifecycle.current_phase().can_accept_connections() {
            return None;
        }

        self.shedder.enqueue();
        match self.shedder.should_accept() {
            Ok(guard) => {
                self.active_requests.fetch_add(1, Ordering::Relaxed);
                Some(RequestHandle {
                    _guard: guard,
                    active_requests: Arc::clone(&self.active_requests),
                })
            }
            Err(()) => None,
        }
    }

    fn request_shutdown(&mut self) {
        self.lifecycle.request_shutdown();
        self.lifecycle.transition_to(LifecyclePhase::Draining);
        self.drain_started_at = Some(Instant::now());
    }

    fn request_reload(&mut self) {
        self.lifecycle.request_reload();
        self.lifecycle.transition_to(LifecyclePhase::Reloading);
    }

    fn complete_reload(&mut self) {
        self.lifecycle.clear_reload_request();
        self.lifecycle.transition_to(LifecyclePhase::Running);
    }

    fn is_shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::Relaxed)
    }

    fn is_draining(&self) -> bool {
        self.lifecycle.current_phase() == LifecyclePhase::Draining
    }

    fn is_reloading(&self) -> bool {
        self.lifecycle.current_phase() == LifecyclePhase::Reloading
    }

    fn active_request_count(&self) -> u64 {
        self.active_requests.load(Ordering::Relaxed)
    }

    fn inflight_count(&self) -> u64 {
        self.shedder.inflight()
    }

    fn tick(&mut self) -> bool {
        match self.lifecycle.current_phase() {
            LifecyclePhase::Draining => {
                // Check if all requests are complete
                if self.active_request_count() == 0 {
                    self.lifecycle.transition_to(LifecyclePhase::ShuttingDown);
                    self.lifecycle.transition_to(LifecyclePhase::Shutdown);
                    self.shutdown_complete.store(true, Ordering::Relaxed);
                    return true;
                }

                // Check drain timeout
                if let Some(started) = self.drain_started_at
                    && started.elapsed() > Duration::from_millis(self.drain_timeout_ms)
                {
                    self.lifecycle.transition_to(LifecyclePhase::ShuttingDown);
                    self.lifecycle.transition_to(LifecyclePhase::Shutdown);
                    self.shutdown_complete.store(true, Ordering::Relaxed);
                    return true;
                }
                false
            }
            LifecyclePhase::Shutdown => true,
            _ => false,
        }
    }

    fn wait_for_shutdown(&mut self, timeout_ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            if self.tick() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

struct RequestHandle {
    _guard: RequestGuard,
    active_requests: Arc<AtomicU64>,
}

impl Drop for RequestHandle {
    fn drop(&mut self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

#[test]
fn should_reject_new_requests_when_shutdown_initiated() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(100),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept a request while running
    let handle = server.accept_request();
    assert!(handle.is_some(), "should accept request while running");

    // Initiate shutdown
    server.request_shutdown();

    // Should reject new requests during draining
    let new_handle = server.accept_request();
    assert!(
        new_handle.is_none(),
        "should reject new requests while draining"
    );

    // Original request should still be tracked
    assert_eq!(server.active_request_count(), 1);
}

#[test]
fn should_reject_new_requests_when_reload_initiated() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(100),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept a request while running
    let handle = server.accept_request();
    assert!(handle.is_some(), "should accept request while running");

    // Initiate reload
    server.request_reload();

    // Should reject new requests during reloading
    let new_handle = server.accept_request();
    assert!(
        new_handle.is_none(),
        "should reject new requests while reloading"
    );

    // Complete reload
    server.complete_reload();

    // Should accept new requests again
    let after_reload = server.accept_request();
    assert!(
        after_reload.is_some(),
        "should accept requests after reload complete"
    );
}

#[test]
fn should_allow_inflight_requests_to_complete_during_drain() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept multiple requests
    let mut handles: Vec<_> = (0..5).filter_map(|_| server.accept_request()).collect();
    assert_eq!(handles.len(), 5, "should accept 5 requests");
    assert_eq!(server.inflight_count(), 5);

    // Initiate shutdown
    server.request_shutdown();

    // Complete some requests
    handles.drain(0..3);

    // Should have fewer active requests but still draining
    assert_eq!(server.active_request_count(), 2);
    assert!(server.is_draining());
}

#[test]
fn should_complete_shutdown_when_all_requests_finish() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept some requests
    let handles: Vec<_> = (0..3).filter_map(|_| server.accept_request()).collect();
    assert_eq!(handles.len(), 3);

    // Initiate shutdown
    server.request_shutdown();

    // Complete all requests
    drop(handles);

    // Wait for shutdown to complete
    assert!(
        server.wait_for_shutdown(1000),
        "should complete shutdown when all requests finish"
    );
    assert!(server.is_shutdown_complete());
    assert_eq!(server.active_request_count(), 0);
}

#[test]
fn should_force_shutdown_on_drain_timeout() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(100, shedder); // 100ms timeout

    // Accept requests that will take a while
    let _handles: Vec<_> = (0..3).filter_map(|_| server.accept_request()).collect();

    // Initiate shutdown
    server.request_shutdown();

    // Wait for timeout
    assert!(
        server.wait_for_shutdown(500),
        "should force shutdown after timeout"
    );
    assert!(server.is_shutdown_complete());
}

#[test]
fn should_track_inflight_count_correctly_through_shutdown() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept and complete requests before shutdown
    for i in 0..5 {
        let handle = server.accept_request();
        assert!(handle.is_some(), "should accept request {}", i);
        drop(handle);
        // Give time for drop to complete
        thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(server.inflight_count(), 0, "inflight should be 0");

    // Accept requests that persist during shutdown
    let mut persistent: Vec<_> = (0..3).filter_map(|_| server.accept_request()).collect();
    assert_eq!(persistent.len(), 3);
    assert_eq!(server.inflight_count(), 3);

    // Initiate shutdown
    server.request_shutdown();
    assert_eq!(
        server.inflight_count(),
        3,
        "inflight unchanged during drain"
    );

    // Complete one request
    persistent.swap_remove(0);
    drop(persistent);
    thread::sleep(Duration::from_millis(10));
    assert_eq!(server.inflight_count(), 0);
}

#[test]
fn should_handle_concurrent_requests_during_shutdown() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(100),
        max_queue: Some(100),
    }));
    let server = Arc::new(std::sync::Mutex::new(SimulatedServer::new(2000, shedder)));

    // Spawn threads to make requests
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let srv = Arc::clone(&server);
            thread::spawn(move || {
                let guard = srv.lock().unwrap();
                let handle = guard.accept_request();
                drop(guard);

                if let Some(req) = handle {
                    // Simulate work
                    thread::sleep(Duration::from_millis(50));
                    drop(req);
                    true
                } else {
                    false
                }
            })
        })
        .collect();

    // Small delay then initiate shutdown
    thread::sleep(Duration::from_millis(20));
    {
        let mut guard = server.lock().unwrap();
        guard.request_shutdown();
    }

    // Wait for all threads
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("thread should complete"))
        .collect();

    // Most should have been accepted (before shutdown)
    let accepted_count = results.iter().filter(|&&x| x).count();
    assert!(
        accepted_count >= 5,
        "at least 5 requests should have been accepted, got {}",
        accepted_count
    );
}

#[test]
fn should_prevent_new_requests_during_reload_then_resume() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(100),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept a request
    let handle = server.accept_request();
    assert!(handle.is_some());

    // Initiate reload
    server.request_reload();
    assert!(server.is_reloading());

    // Try to accept during reload
    let during_reload = server.accept_request();
    assert!(
        during_reload.is_none(),
        "should reject requests during reload"
    );

    // Complete reload
    server.complete_reload();
    assert!(!server.is_reloading());

    // Should accept again
    let after_reload = server.accept_request();
    assert!(after_reload.is_some(), "should accept after reload");

    // Cleanup
    drop(handle);
    drop(after_reload);
}

#[test]
fn should_handle_reload_while_requests_in_flight() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept multiple requests
    let handles: Vec<_> = (0..5).filter_map(|_| server.accept_request()).collect();
    assert_eq!(handles.len(), 5);
    assert_eq!(server.inflight_count(), 5);

    // Initiate reload
    server.request_reload();

    // Requests are still in flight
    assert_eq!(server.inflight_count(), 5);

    // Complete reload
    server.complete_reload();

    // Requests still tracked
    assert_eq!(server.inflight_count(), 5);

    // Drop all
    drop(handles);
    thread::sleep(Duration::from_millis(10));
    assert_eq!(server.inflight_count(), 0);
}

#[test]
fn should_maintain_queue_limits_during_shutdown() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(2),
        max_queue: Some(2),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Fill up inflight
    let handle1 = server.accept_request();
    let handle2 = server.accept_request();
    assert!(handle1.is_some());
    assert!(handle2.is_some());

    // Fill queue
    server.shedder.enqueue();
    server.shedder.enqueue();
    assert_eq!(server.shedder.queued(), 2);

    // Initiate shutdown
    server.request_shutdown();

    // Should reject any new attempts
    let handle3 = server.accept_request();
    assert!(
        handle3.is_none(),
        "should reject when inflight full and draining"
    );

    // Release one
    drop(handle1);
    thread::sleep(Duration::from_millis(5));

    // Still draining, so can't accept new
    let handle4 = server.accept_request();
    assert!(handle4.is_none(), "still rejecting during drain");
}

#[test]
fn should_handle_rapid_shutdown_reload_sequence() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Accept a request
    let handle = server.accept_request();
    assert!(handle.is_some());

    // Rapid sequence: reload then immediately shutdown
    server.request_reload();
    assert!(server.is_reloading());

    server.request_shutdown();
    assert!(server.is_draining());
    assert!(!server.is_reloading());

    // Should reject requests
    let new_req = server.accept_request();
    assert!(new_req.is_none());

    // Complete
    drop(handle);
    assert!(
        server.wait_for_shutdown(1000),
        "should complete after rapid sequence"
    );
}

#[test]
fn should_handle_shutdown_with_zero_inflight() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // No active requests
    assert_eq!(server.active_request_count(), 0);

    // Initiate shutdown
    server.request_shutdown();

    // Should complete immediately
    assert!(
        server.wait_for_shutdown(100),
        "should complete immediately with zero inflight"
    );
    assert!(server.is_shutdown_complete());
}

#[test]
fn should_not_accept_requests_in_terminal_shutdown_phase() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(100),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Fast shutdown
    server.request_shutdown();

    // Force quick completion
    while !server.tick() {
        thread::sleep(Duration::from_millis(1));
    }

    assert!(server.is_shutdown_complete());

    // Should not accept in shutdown phase
    let handle = server.accept_request();
    assert!(
        handle.is_none(),
        "should never accept requests after shutdown"
    );
}

#[test]
fn should_track_lifecycle_phases_during_shutdown_with_inflight() {
    let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
        max_inflight: Some(10),
        max_queue: Some(50),
    }));
    let mut server = SimulatedServer::new(5000, shedder);

    // Start in running
    assert_eq!(server.lifecycle.current_phase(), LifecyclePhase::Running);

    // Accept request
    let handle = server.accept_request();
    assert!(handle.is_some());

    // Initiate shutdown -> Draining
    server.request_shutdown();
    assert_eq!(server.lifecycle.current_phase(), LifecyclePhase::Draining);

    // Complete the request
    drop(handle);

    // Wait for completion
    assert!(server.wait_for_shutdown(1000), "should complete shutdown");

    // Should be Shutdown
    assert_eq!(server.lifecycle.current_phase(), LifecyclePhase::Shutdown);
}
