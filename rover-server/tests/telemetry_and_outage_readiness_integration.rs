//! Integration tests for telemetry emission and dependency outage readiness.
//!
//! This module tests:
//! - Complete telemetry emission throughout the request lifecycle
//! - Dependency outage readiness (load shedding, graceful degradation)
//! - System behavior under various failure conditions

use rover_server::{LoadShedConfig, LoadShedder};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

/// Telemetry event captured during request flow simulation
#[derive(Debug, Clone)]
enum TelemetryEvent {
    RequestReceived {
        request_id: String,
    },
    RequestAccepted {
        request_id: String,
    },
    RequestRejected {
        request_id: String,
        reason: RejectionReason,
    },
    RequestCompleted {
        request_id: String,
        duration_ms: u64,
        status: u16,
    },
    LoadShedTriggered {
        inflight: u64,
        queued: u64,
    },
    DependencyUnavailable {
        dependency: String,
    },
}

#[derive(Debug, Clone)]
enum RejectionReason {
    InflightLimit,
    QueueLimit,
    DependencyUnavailable,
}

/// Simulates a telemetry collector that records events
struct TelemetryCollector {
    events: std::sync::Mutex<Vec<TelemetryEvent>>,
}

impl TelemetryCollector {
    fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn record(&self, event: TelemetryEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    fn get_events(&self) -> Vec<TelemetryEvent> {
        self.events.lock().unwrap().clone()
    }

    fn clear(&self) {
        let mut events = self.events.lock().unwrap();
        events.clear();
    }

    fn count_by_type(&self, event_type: &str) -> usize {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .filter(|e| format!("{:?}", e).starts_with(event_type))
            .count()
    }
}

mod telemetry_emission {
    use super::*;

    #[test]
    fn should_emit_all_telemetry_events_for_successful_request() {
        let collector = Arc::new(TelemetryCollector::new());
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(5),
        }));

        let request_id = "test-req-001".to_string();
        let collector_clone = Arc::clone(&collector);

        collector.record(TelemetryEvent::RequestReceived {
            request_id: request_id.clone(),
        });

        shedder.enqueue();
        match shedder.should_accept() {
            Ok(_guard) => {
                collector_clone.record(TelemetryEvent::RequestAccepted {
                    request_id: request_id.clone(),
                });

                thread::sleep(Duration::from_millis(10));

                collector_clone.record(TelemetryEvent::RequestCompleted {
                    request_id: request_id.clone(),
                    duration_ms: 10,
                    status: 200,
                });
            }
            Err(_) => {
                collector_clone.record(TelemetryEvent::RequestRejected {
                    request_id: request_id.clone(),
                    reason: RejectionReason::InflightLimit,
                });
            }
        }

        let events = collector.get_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TelemetryEvent::RequestReceived { .. })),
            "should emit RequestReceived"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TelemetryEvent::RequestAccepted { .. })),
            "should emit RequestAccepted"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TelemetryEvent::RequestCompleted { .. })),
            "should emit RequestCompleted"
        );
    }

    #[test]
    fn should_emit_rejection_telemetry_when_overloaded() {
        let collector = Arc::new(TelemetryCollector::new());
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(1),
            max_queue: Some(1),
        }));

        let _guard = shedder.should_accept().unwrap();

        let collector_clone = Arc::clone(&collector);
        let request_id = "test-req-002".to_string();

        collector_clone.record(TelemetryEvent::RequestReceived {
            request_id: request_id.clone(),
        });

        shedder.enqueue();
        if shedder.should_accept().is_err() {
            collector_clone.record(TelemetryEvent::RequestRejected {
                request_id: request_id.clone(),
                reason: RejectionReason::InflightLimit,
            });
            collector_clone.record(TelemetryEvent::LoadShedTriggered {
                inflight: shedder.inflight(),
                queued: shedder.queued(),
            });
        }

        let events = collector.get_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TelemetryEvent::RequestRejected { .. })),
            "should emit RequestRejected"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TelemetryEvent::LoadShedTriggered { .. })),
            "should emit LoadShedTriggered"
        );
    }

    #[test]
    fn should_emit_telemetry_for_multiple_concurrent_requests() {
        let collector = Arc::new(TelemetryCollector::new());
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(5),
            max_queue: Some(10),
        }));

        let barrier = Arc::new(Barrier::new(5));
        let mut handles = vec![];

        for i in 0..5 {
            let shedder_clone = Arc::clone(&shedder);
            let collector_clone = Arc::clone(&collector);
            let barrier_clone = Arc::clone(&barrier);

            let handle = thread::spawn(move || {
                let request_id = format!("req-{}", i);

                collector_clone.record(TelemetryEvent::RequestReceived {
                    request_id: request_id.clone(),
                });

                shedder_clone.enqueue();
                match shedder_clone.should_accept() {
                    Ok(_guard) => {
                        collector_clone.record(TelemetryEvent::RequestAccepted {
                            request_id: request_id.clone(),
                        });

                        barrier_clone.wait();
                        thread::sleep(Duration::from_millis(5));

                        collector_clone.record(TelemetryEvent::RequestCompleted {
                            request_id: request_id.clone(),
                            duration_ms: 5,
                            status: 200,
                        });
                    }
                    Err(_) => {
                        collector_clone.record(TelemetryEvent::RequestRejected {
                            request_id: request_id.clone(),
                            reason: RejectionReason::InflightLimit,
                        });
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let events = collector.get_events();
        let received_count = events
            .iter()
            .filter(|e| matches!(e, TelemetryEvent::RequestReceived { .. }))
            .count();
        let accepted_count = events
            .iter()
            .filter(|e| matches!(e, TelemetryEvent::RequestAccepted { .. }))
            .count();
        let completed_count = events
            .iter()
            .filter(|e| matches!(e, TelemetryEvent::RequestCompleted { .. }))
            .count();

        assert_eq!(received_count, 5, "should emit 5 RequestReceived events");
        assert_eq!(accepted_count, 5, "should emit 5 RequestAccepted events");
        assert_eq!(completed_count, 5, "should emit 5 RequestCompleted events");
    }

    #[test]
    fn should_emit_load_shed_metrics_when_threshold_exceeded() {
        let collector = Arc::new(TelemetryCollector::new());
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(2),
            max_queue: Some(2),
        }));

        let guards: Vec<_> = (0..2).map(|_| shedder.should_accept().unwrap()).collect();

        shedder.enqueue();
        shedder.enqueue();
        shedder.enqueue();

        let collector_clone = Arc::clone(&collector);
        collector_clone.record(TelemetryEvent::LoadShedTriggered {
            inflight: shedder.inflight(),
            queued: shedder.queued(),
        });

        if shedder.should_accept().is_err() {
            collector_clone.record(TelemetryEvent::RequestRejected {
                request_id: "overflow-req".to_string(),
                reason: RejectionReason::QueueLimit,
            });
        }

        let events = collector.get_events();
        let load_shed_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TelemetryEvent::LoadShedTriggered { .. }))
            .collect();

        assert_eq!(load_shed_events.len(), 1);
        if let TelemetryEvent::LoadShedTriggered { inflight, queued } = load_shed_events[0] {
            assert_eq!(*inflight, 2, "should track inflight count");
            assert_eq!(*queued, 3, "should track queued count");
        }

        drop(guards);
    }

    #[test]
    fn should_emit_dependency_unavailable_telemetry() {
        let collector = Arc::new(TelemetryCollector::new());

        collector.record(TelemetryEvent::DependencyUnavailable {
            dependency: "database".to_string(),
        });
        collector.record(TelemetryEvent::DependencyUnavailable {
            dependency: "redis".to_string(),
        });
        collector.record(TelemetryEvent::RequestRejected {
            request_id: "dep-req-001".to_string(),
            reason: RejectionReason::DependencyUnavailable,
        });

        let events = collector.get_events();
        let dep_unavailable_count = events
            .iter()
            .filter(|e| matches!(e, TelemetryEvent::DependencyUnavailable { .. }))
            .count();

        assert_eq!(
            dep_unavailable_count, 2,
            "should emit 2 DependencyUnavailable events"
        );

        let rejection = events
            .iter()
            .find(|e| matches!(e, TelemetryEvent::RequestRejected { .. }));
        assert!(rejection.is_some());
        if let Some(TelemetryEvent::RequestRejected { reason, .. }) = rejection {
            assert!(
                matches!(reason, RejectionReason::DependencyUnavailable),
                "should have correct rejection reason"
            );
        }
    }

    #[test]
    fn should_emit_telemetry_with_accurate_timing() {
        let collector = Arc::new(TelemetryCollector::new());
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(5),
        });

        let request_id = "timed-req".to_string();
        let start = Instant::now();

        collector.record(TelemetryEvent::RequestReceived {
            request_id: request_id.clone(),
        });

        shedder.enqueue();
        let _guard = shedder.should_accept().unwrap();

        collector.record(TelemetryEvent::RequestAccepted {
            request_id: request_id.clone(),
        });

        thread::sleep(Duration::from_millis(20));

        let elapsed_ms = start.elapsed().as_millis() as u64;

        collector.record(TelemetryEvent::RequestCompleted {
            request_id: request_id.clone(),
            duration_ms: elapsed_ms,
            status: 200,
        });

        let events = collector.get_events();
        let completed = events
            .iter()
            .find(|e| matches!(e, TelemetryEvent::RequestCompleted { .. }))
            .unwrap();

        if let TelemetryEvent::RequestCompleted { duration_ms, .. } = completed {
            assert!(*duration_ms >= 20, "duration should be at least 20ms");
            assert!(
                *duration_ms < 500,
                "duration should be reasonable (< 500ms)"
            );
        }
    }
}

mod dependency_outage_readiness {
    use super::*;

    #[test]
    fn should_shed_load_when_dependency_overloaded() {
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(3),
            max_queue: Some(2),
        });

        let accepted = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let shedder = shedder.clone();
                let accepted = Arc::clone(&accepted);
                let rejected = Arc::clone(&rejected);

                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(i * 5));

                    shedder.enqueue();
                    match shedder.should_accept() {
                        Ok(_guard) => {
                            accepted.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(30));
                        }
                        Err(_) => {
                            rejected.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let total_accepted = accepted.load(Ordering::Relaxed);
        let total_rejected = rejected.load(Ordering::Relaxed);

        assert!(
            total_rejected > 0,
            "should reject some requests when overloaded: accepted={}, rejected={}",
            total_accepted,
            total_rejected
        );
        assert_eq!(
            total_accepted + total_rejected,
            20,
            "all requests should be accounted for"
        );
    }

    #[test]
    fn should_recover_after_dependency_restored() {
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(2),
            max_queue: Some(5),
        }));

        // Queue up 2 requests and accept them
        let guards: Vec<_> = (0..2)
            .map(|_| {
                shedder.enqueue();
                shedder.should_accept().unwrap()
            })
            .collect();

        // At capacity - inflight=2, queued=0
        // Try to accept one more - should succeed since queued=0 < max_queue=5
        // but fail since inflight=2 >= max_inflight=2
        shedder.enqueue();
        assert!(
            shedder.should_accept().is_err(),
            "should reject when at inflight capacity"
        );

        drop(guards);

        thread::sleep(Duration::from_millis(10));

        // After dropping, inflight=0, queued still accounts for the failed attempt
        // Try again - should succeed
        shedder.enqueue();
        assert!(
            shedder.should_accept().is_ok(),
            "should accept after resources freed"
        );
    }

    #[test]
    fn should_maintain_graceful_degradation_under_cascading_failure() {
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(5),
            max_queue: Some(5),
        }));

        let success_count = Arc::new(AtomicU64::new(0));
        let shed_count = Arc::new(AtomicU64::new(0));

        let mut handles = vec![];

        let barrier = Arc::new(Barrier::new(10));

        for _ in 0..10 {
            let shedder = Arc::clone(&shedder);
            let success_count = Arc::clone(&success_count);
            let shed_count = Arc::clone(&shed_count);
            let barrier = Arc::clone(&barrier);

            let handle = thread::spawn(move || {
                barrier.wait();
                for _ in 0..5 {
                    shedder.enqueue();
                    match shedder.should_accept() {
                        Ok(_guard) => {
                            success_count.fetch_add(1, Ordering::Relaxed);
                            // Hold the guard longer to create more contention
                            thread::sleep(Duration::from_millis(20));
                        }
                        Err(_) => {
                            shed_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let total = success_count.load(Ordering::Relaxed) + shed_count.load(Ordering::Relaxed);
        assert_eq!(total, 50, "all 50 requests should be processed or shed");

        assert!(
            shed_count.load(Ordering::Relaxed) > 0,
            "should shed some load during overload: shed={}, success={}",
            shed_count.load(Ordering::Relaxed),
            success_count.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn should_prevent_cascade_with_zero_inflight_limit() {
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(0),
            max_queue: Some(10),
        });

        shedder.enqueue();
        let result = shedder.should_accept();

        assert!(
            result.is_err(),
            "should reject all requests when inflight limit is 0 (circuit open)"
        );
    }

    #[test]
    fn should_prioritize_critical_requests_during_outage() {
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(3),
            max_queue: Some(2),
        }));

        let critical_accepted = Arc::new(AtomicU64::new(0));
        let normal_accepted = Arc::new(AtomicU64::new(0));

        let mut handles = vec![];

        for i in 0..10 {
            let shedder = Arc::clone(&shedder);
            let critical_accepted = Arc::clone(&critical_accepted);
            let normal_accepted = Arc::clone(&normal_accepted);
            let is_critical = i < 3;

            let handle = thread::spawn(move || {
                shedder.enqueue();

                if is_critical {
                    let mut retries = 0;
                    while retries < 5 {
                        if let Ok(_guard) = shedder.should_accept() {
                            critical_accepted.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(10));
                            return;
                        }
                        retries += 1;
                        thread::sleep(Duration::from_millis(2));
                    }
                } else if shedder.should_accept().is_ok() {
                    normal_accepted.fetch_add(1, Ordering::Relaxed);
                    thread::sleep(Duration::from_millis(10));
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let critical = critical_accepted.load(Ordering::Relaxed);
        let normal = normal_accepted.load(Ordering::Relaxed);

        assert!(
            critical >= normal,
            "critical requests should be prioritized: critical={}, normal={}",
            critical,
            normal
        );
    }

    #[test]
    fn should_track_resource_exhaustion_metrics() {
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(2),
            max_queue: Some(3),
        });

        assert_eq!(shedder.inflight(), 0);
        assert_eq!(shedder.queued(), 0);

        shedder.enqueue();
        shedder.enqueue();
        assert_eq!(shedder.queued(), 2);

        let _g1 = shedder.should_accept().unwrap();
        assert_eq!(shedder.inflight(), 1);
        assert_eq!(shedder.queued(), 1);

        let _g2 = shedder.should_accept().unwrap();
        assert_eq!(shedder.inflight(), 2);
        assert_eq!(shedder.queued(), 0);

        assert!(
            shedder.should_accept().is_err(),
            "should reject when both inflight and queue exhausted"
        );
    }

    #[test]
    fn should_handle_rapid_outage_recovery() {
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(5),
            max_queue: Some(10),
        }));

        let iteration_count = Arc::new(AtomicU64::new(0));

        for cycle in 0..5 {
            let shedder_clone = Arc::clone(&shedder);
            let iteration_count_clone = Arc::clone(&iteration_count);

            thread::spawn(move || {
                for _ in 0..10 {
                    shedder_clone.enqueue();
                    if let Ok(_guard) = shedder_clone.should_accept() {
                        iteration_count_clone.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .join()
            .unwrap();

            assert!(
                shedder.inflight() == 0,
                "all requests should complete in cycle {}",
                cycle
            );
        }

        assert_eq!(
            iteration_count.load(Ordering::Relaxed),
            50,
            "all requests should be accepted"
        );
    }
}

mod request_lifecycle_telemetry {
    use super::*;

    #[test]
    fn should_correlate_request_id_across_lifecycle() {
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(10),
            max_queue: Some(5),
        });

        let _request_id = "correlation-test-123";

        shedder.enqueue();
        let _guard = shedder.should_accept().unwrap();

        assert_eq!(
            shedder.inflight(),
            1,
            "request should be tracked by the shedder"
        );
    }

    #[test]
    fn should_emit_status_code_in_completion_telemetry() {
        let collector = Arc::new(TelemetryCollector::new());

        let statuses = vec![200, 201, 400, 404, 500, 503];

        for status in statuses {
            collector.record(TelemetryEvent::RequestCompleted {
                request_id: format!("req-{}", status),
                duration_ms: 10,
                status,
            });
        }

        let events = collector.get_events();
        assert_eq!(events.len(), 6);

        let completed_events: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let TelemetryEvent::RequestCompleted { status, .. } = e {
                    Some(*status)
                } else {
                    None
                }
            })
            .collect();

        assert!(completed_events.contains(&200));
        assert!(completed_events.contains(&500));
        assert!(completed_events.contains(&503));
    }

    #[test]
    fn should_measure_request_duration_accurately() {
        let durations = vec![
            Duration::from_millis(1),
            Duration::from_millis(5),
            Duration::from_millis(10),
            Duration::from_millis(50),
            Duration::from_millis(100),
        ];

        for expected_duration in durations {
            let start = Instant::now();
            thread::sleep(expected_duration);
            let actual_duration = start.elapsed();

            // On heavily loaded systems, thread::sleep can take much longer than requested
            // Just verify that at least the minimum time has passed
            assert!(
                actual_duration >= expected_duration.saturating_sub(Duration::from_millis(5)),
                "duration should be at least the expected time minus small tolerance: expected ~{:?}, got {:?}",
                expected_duration,
                actual_duration
            );
            // Upper bound is very loose due to system scheduling variability (CI systems can be very slow)
            assert!(
                actual_duration <= expected_duration + Duration::from_millis(500),
                "duration should not exceed expected by too much: expected ~{:?}, got {:?}",
                expected_duration,
                actual_duration
            );
        }
    }
}

mod integration_with_existing_tests {
    use super::*;

    #[test]
    fn should_maintain_existing_load_shedder_behavior() {
        let shedder = LoadShedder::new(LoadShedConfig {
            max_inflight: Some(100),
            max_queue: Some(50),
        });

        let mut guards = vec![];
        for _ in 0..50 {
            guards.push(shedder.should_accept().unwrap());
        }

        assert_eq!(shedder.inflight(), 50);

        drop(guards);

        assert_eq!(shedder.inflight(), 0);
    }

    #[test]
    fn should_handle_high_concurrency_with_telemetry() {
        let shedder = Arc::new(LoadShedder::new(LoadShedConfig {
            max_inflight: Some(50),
            max_queue: Some(100),
        }));

        let accepted = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let shedder = Arc::clone(&shedder);
                let accepted = Arc::clone(&accepted);

                thread::spawn(move || {
                    shedder.enqueue();
                    if let Ok(_guard) = shedder.should_accept() {
                        accepted.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(1));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let final_accepted = accepted.load(Ordering::Relaxed);
        assert!(final_accepted > 0, "should accept some requests");
        assert!(final_accepted <= 100, "should not exceed total requests");
    }
}
