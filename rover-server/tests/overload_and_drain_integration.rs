use rover_server::LoadShedConfig;
use rover_server::LoadShedder;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

mod load_shedder_concurrent {
    use super::*;

    #[test]
    fn should_handle_concurrent_requests_under_limit() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        };
        let shedder = Arc::new(LoadShedder::new(config));
        let accepted = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));

        let mut handles = vec![];

        for _ in 0..50 {
            let shedder = Arc::clone(&shedder);
            let accepted = Arc::clone(&accepted);
            let rejected = Arc::clone(&rejected);
            handles.push(thread::spawn(move || {
                shedder.enqueue();
                match shedder.should_accept() {
                    Ok(guard) => {
                        accepted.fetch_add(1, Ordering::Relaxed);
                        drop(guard);
                    }
                    Err(()) => {
                        rejected.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        let accepted_count = accepted.load(Ordering::Relaxed);
        let rejected_count = rejected.load(Ordering::Relaxed);

        assert_eq!(accepted_count, 50, "all 50 requests should be accepted");
        assert_eq!(rejected_count, 0, "no requests should be rejected");
    }

    #[test]
    fn should_reject_requests_when_overloaded() {
        let config = LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(5),
        };
        let shedder = Arc::new(LoadShedder::new(config));
        let accepted = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let shedder = Arc::clone(&shedder);
                let accepted = Arc::clone(&accepted);
                let rejected = Arc::clone(&rejected);
                thread::spawn(move || {
                    shedder.enqueue();
                    match shedder.should_accept() {
                        Ok(guard) => {
                            accepted.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(std::time::Duration::from_millis(50));
                            drop(guard);
                        }
                        Err(()) => {
                            rejected.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        let accepted_count = accepted.load(Ordering::Relaxed);
        let rejected_count = rejected.load(Ordering::Relaxed);

        assert_eq!(
            accepted_count + rejected_count,
            100,
            "total requests should equal attempted"
        );
        assert!(
            rejected_count > 85,
            "most requests should be rejected under overload: accepted={}, rejected={}",
            accepted_count,
            rejected_count
        );
    }

    #[test]
    fn should_accept_again_after_backlog_clears() {
        let config = LoadShedConfig {
            max_inflight: Some(5),
            max_queue: Some(10),
        };
        let shedder = Arc::new(LoadShedder::new(config));

        let guards: Vec<_> = (0..5).map(|_| shedder.should_accept().unwrap()).collect();

        assert!(
            shedder.should_accept().is_err(),
            "should reject when at limit"
        );

        drop(guards);

        assert!(
            shedder.should_accept().is_ok(),
            "should accept after clearing"
        );
    }

    #[test]
    fn should_track_inflight_across_threads() {
        let config = LoadShedConfig {
            max_inflight: Some(20),
            max_queue: Some(100),
        };
        let shedder = Arc::new(LoadShedder::new(config));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let shedder = Arc::clone(&shedder);
                thread::spawn(move || {
                    let guard = shedder.should_accept().expect("should accept under limit");
                    thread::sleep(std::time::Duration::from_millis(10));
                    guard
                })
            })
            .collect();

        let guards: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("thread join"))
            .collect();

        assert_eq!(shedder.inflight(), 10);

        drop(guards);
        assert_eq!(shedder.inflight(), 0);
    }

    #[test]
    fn should_maintain_consistency_under_stress() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(100),
        };
        let shedder = Arc::new(LoadShedder::new(config));
        let iterations = 1000;

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let shedder = Arc::clone(&shedder);
                thread::spawn(move || {
                    for _ in 0..iterations {
                        shedder.enqueue();
                        if let Ok(guard) = shedder.should_accept() {
                            drop(guard);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        assert_eq!(shedder.inflight(), 0, "inflight should return to zero");
        let queued = shedder.queued();
        assert!(
            queued < 100,
            "queued should be low after drain: queued={}",
            queued
        );
    }
}

mod graceful_drain_state_machine {

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum ShutdownState {
        Running = 0,
        Draining = 1,
        Shutdown = 2,
    }

    struct SimulatedServer {
        shutdown_state: ShutdownState,
        active_connections: u64,
        drain_timeout_ms: u64,
        drain_started_ms: Option<u64>,
        current_time_ms: u64,
    }

    impl SimulatedServer {
        fn new(drain_timeout_ms: u64) -> Self {
            Self {
                shutdown_state: ShutdownState::Running,
                active_connections: 0,
                drain_timeout_ms,
                drain_started_ms: None,
                current_time_ms: 0,
            }
        }

        fn handle_signal(&mut self) {
            match self.shutdown_state {
                ShutdownState::Running => {
                    self.shutdown_state = ShutdownState::Draining;
                    self.drain_started_ms = Some(self.current_time_ms);
                }
                ShutdownState::Draining | ShutdownState::Shutdown => {}
            }
        }

        fn tick(&mut self, elapsed_ms: u64) -> bool {
            self.current_time_ms += elapsed_ms;

            match self.shutdown_state {
                ShutdownState::Running => false,
                ShutdownState::Draining => {
                    if self.active_connections == 0 {
                        self.shutdown_state = ShutdownState::Shutdown;
                        return true;
                    }
                    if let Some(started) = self.drain_started_ms {
                        if self.current_time_ms - started >= self.drain_timeout_ms {
                            self.shutdown_state = ShutdownState::Shutdown;
                            return true;
                        }
                    }
                    false
                }
                ShutdownState::Shutdown => true,
            }
        }

        fn accept_request(&mut self) -> bool {
            if self.shutdown_state == ShutdownState::Running {
                self.active_connections += 1;
                return true;
            }
            false
        }

        fn complete_request(&mut self) {
            if self.active_connections > 0 {
                self.active_connections -= 1;
            }
        }

        fn is_shutdown(&self) -> bool {
            self.shutdown_state == ShutdownState::Shutdown
        }

        fn is_draining(&self) -> bool {
            self.shutdown_state == ShutdownState::Draining
        }
    }

    #[test]
    fn should_transition_running_to_draining_on_signal() {
        let mut server = SimulatedServer::new(5000);
        assert!(server.accept_request());
        assert!(!server.is_draining());

        server.handle_signal();
        assert!(server.is_draining());
        assert!(!server.is_shutdown());
    }

    #[test]
    fn should_reject_new_requests_while_draining() {
        let mut server = SimulatedServer::new(5000);
        assert!(server.accept_request());

        server.handle_signal();
        assert!(server.is_draining());

        assert!(!server.accept_request(), "should reject new request");
    }

    #[test]
    fn should_complete_gracefully_when_conns_drain() {
        let mut server = SimulatedServer::new(5000);
        assert!(server.accept_request());
        assert!(server.accept_request());
        assert_eq!(server.active_connections, 2);

        server.handle_signal();
        assert!(server.is_draining());

        server.complete_request();
        server.complete_request();
        assert_eq!(server.active_connections, 0);

        assert!(server.tick(10), "should transition to shutdown");
        assert!(server.is_shutdown());
    }

    #[test]
    fn should_force_shutdown_on_timeout() {
        let mut server = SimulatedServer::new(100);
        assert!(server.accept_request());
        assert!(server.accept_request());

        server.handle_signal();
        assert!(server.is_draining());

        server.tick(150);
        assert!(server.is_shutdown(), "should force shutdown after timeout");
        assert_eq!(server.active_connections, 2);
    }

    #[test]
    fn should_wait_full_timeout_if_connections_remain() {
        let mut server = SimulatedServer::new(500);
        assert!(server.accept_request());

        server.handle_signal();

        server.tick(200);
        assert!(server.is_draining(), "should still draining at 200ms");

        server.tick(200);
        assert!(server.is_draining(), "should still draining at 400ms");

        server.tick(100);
        assert!(server.is_shutdown(), "should shutdown at 500ms");
    }

    #[test]
    fn should_shutdown_immediately_when_no_connections() {
        let mut server = SimulatedServer::new(5000);
        server.handle_signal();

        assert!(
            server.tick(0),
            "should shutdown immediately with no connections"
        );
        assert!(server.is_shutdown());
    }

    #[test]
    fn should_transition_draining_to_shutdown_only_once() {
        let mut server = SimulatedServer::new(100);
        assert!(server.accept_request());

        server.handle_signal();
        server.tick(150);
        assert!(server.is_shutdown());

        server.tick(100);
        assert!(server.is_shutdown(), "should remain shutdown");
    }

    #[test]
    fn should_reach_drain_complete_before_timeout() {
        let mut server = SimulatedServer::new(1000);
        assert!(server.accept_request());

        server.handle_signal();

        server.tick(100);
        assert!(server.is_draining());

        server.complete_request();
        assert!(server.tick(50), "should complete once connections are gone");
        assert!(server.is_shutdown());
    }

    #[test]
    fn should_handle_rapid_fire_signals() {
        let mut server = SimulatedServer::new(100);
        server.handle_signal();
        server.handle_signal();
        server.handle_signal();

        assert!(server.is_draining());
        assert!(!server.is_shutdown());
    }
}

mod load_shedder_edge_cases {
    use super::*;

    #[test]
    fn should_handle_zero_limits() {
        let config = LoadShedConfig {
            max_inflight: Some(0),
            max_queue: Some(0),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        let result = shedder.should_accept();
        assert!(result.is_err(), "should reject with zero limit");
    }

    #[test]
    fn should_recover_from_overload() {
        let config = LoadShedConfig {
            max_inflight: Some(2),
            max_queue: Some(5),
        };
        let shedder = LoadShedder::new(config);

        let g1 = shedder.should_accept().unwrap();
        let g2 = shedder.should_accept().unwrap();
        assert!(shedder.should_accept().is_err());

        drop(g1);
        assert!(
            shedder.should_accept().is_ok(),
            "should accept after releasing one"
        );

        drop(g2);
        assert!(shedder.should_accept().is_ok());
    }

    #[test]
    fn should_count_queue_and_inflight_independently() {
        let config = LoadShedConfig {
            max_inflight: Some(20),
            max_queue: Some(100),
        };
        let shedder = LoadShedder::new(config);

        shedder.enqueue();
        shedder.enqueue();
        shedder.enqueue();
        assert_eq!(shedder.queued(), 3);
        assert_eq!(shedder.inflight(), 0);

        let _g1 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 2);
        assert_eq!(shedder.inflight(), 1);

        let _g2 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 1);
        assert_eq!(shedder.inflight(), 2);

        let _g3 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 0);
        assert_eq!(shedder.inflight(), 3);

        shedder.enqueue();
        shedder.enqueue();
        assert_eq!(shedder.queued(), 2);

        let _g4 = shedder.should_accept().unwrap();
        assert_eq!(shedder.queued(), 1);
        assert_eq!(shedder.inflight(), 4);
    }

    #[test]
    fn should_support_clone_for_shared_state() {
        let config = LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        };
        let shedder1 = LoadShedder::new(config);
        let shedder2 = shedder1.clone();

        shedder1.enqueue();
        let _g1 = shedder1.should_accept().unwrap();
        let _g2 = shedder2.should_accept().unwrap();

        assert_eq!(shedder1.inflight(), 2);
        assert_eq!(shedder2.inflight(), 2);
    }
}
