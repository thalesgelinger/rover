use std::time::{Duration, Instant};

use tracing::info;

const LOG_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct LoopMetrics {
    last_emit: Instant,
    poll_calls: u64,
    total_poll_ns: u128,
    max_poll_us: u64,
    events_processed: u64,
    timer_checks: u64,
    timer_expired: u64,
    resume_calls: u64,
    resumed_coroutines: u64,
    pending_high_water: usize,
    conn_high_water: usize,
}

impl LoopMetrics {
    pub fn new() -> Self {
        Self {
            last_emit: Instant::now(),
            poll_calls: 0,
            total_poll_ns: 0,
            max_poll_us: 0,
            events_processed: 0,
            timer_checks: 0,
            timer_expired: 0,
            resume_calls: 0,
            resumed_coroutines: 0,
            pending_high_water: 0,
            conn_high_water: 0,
        }
    }

    pub fn record_poll(&mut self, duration: Duration) {
        self.poll_calls += 1;
        self.total_poll_ns += duration.as_nanos();
        let micros = duration.as_micros() as u64;
        if micros > self.max_poll_us {
            self.max_poll_us = micros;
        }
    }

    pub fn record_events(&mut self, count: usize) {
        self.events_processed += count as u64;
    }

    pub fn record_timer(&mut self, expired: usize) {
        self.timer_checks += 1;
        self.timer_expired += expired as u64;
    }

    pub fn record_resumes(&mut self, resumed: usize) {
        if resumed == 0 {
            return;
        }
        self.resume_calls += 1;
        self.resumed_coroutines += resumed as u64;
    }

    pub fn observe_pending(&mut self, pending: usize) {
        if pending > self.pending_high_water {
            self.pending_high_water = pending;
        }
    }

    pub fn observe_connections(&mut self, connections: usize) {
        if connections > self.conn_high_water {
            self.conn_high_water = connections;
        }
    }

    pub fn maybe_emit(&mut self) {
        if self.last_emit.elapsed() < LOG_INTERVAL {
            return;
        }

        let avg_poll_us = if self.poll_calls > 0 {
            (self.total_poll_ns / self.poll_calls as u128) as u64 / 1000
        } else {
            0
        };

        info!(
            target = "rover::event_loop",
            poll_avg_us = avg_poll_us,
            poll_max_us = self.max_poll_us,
            events = self.events_processed,
            timer_checks = self.timer_checks,
            timer_expired = self.timer_expired,
            resume_calls = self.resume_calls,
            resumed = self.resumed_coroutines,
            pending_high_water = self.pending_high_water,
            conn_high_water = self.conn_high_water,
            "event-loop-metrics"
        );

        self.poll_calls = 0;
        self.total_poll_ns = 0;
        self.max_poll_us = 0;
        self.events_processed = 0;
        self.timer_checks = 0;
        self.timer_expired = 0;
        self.resume_calls = 0;
        self.resumed_coroutines = 0;
        self.pending_high_water = 0;
        self.conn_high_water = 0;
        self.last_emit = Instant::now();
    }
}
