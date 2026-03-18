//! Integration tests for long-running stream cancellation and reconnect paths
//!
//! Tests verify:
//! - Stream cancellation during active streaming
//! - SSE reconnection with retry hints
//! - Producer cleanup on cancellation
//! - Connection state transitions during cancellation
//! - Timeout handling for long-running streams

use bytes::Bytes;
use rover_server::{SseWriter, write_chunk_header, write_final_chunk};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

mod stream_state_machine {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StreamState {
        Idle,
        HeadersSent,
        Streaming,
        FinalChunkQueued,
        Closed,
    }

    struct SimulatedStream {
        state: StreamState,
        chunks: VecDeque<Bytes>,
        final_sent: bool,
        bytes_written: u64,
        cancelled: bool,
    }

    impl SimulatedStream {
        fn new() -> Self {
            Self {
                state: StreamState::Idle,
                chunks: VecDeque::new(),
                final_sent: false,
                bytes_written: 0,
                cancelled: false,
            }
        }

        fn start_stream(&mut self) {
            assert_eq!(self.state, StreamState::Idle);
            self.state = StreamState::HeadersSent;
        }

        fn queue_chunk(&mut self, chunk: Bytes) {
            if self.cancelled || self.final_sent {
                return;
            }
            let encoded = Self::encode_chunk(&chunk);
            self.chunks.push_back(encoded);
            self.bytes_written += chunk.len() as u64;
        }

        fn encode_chunk(data: &[u8]) -> Bytes {
            let size = data.len();
            let hex_size = format!("{:x}", size);
            let mut encoded = Vec::with_capacity(hex_size.len() + 2 + size + 2);
            encoded.extend_from_slice(hex_size.as_bytes());
            encoded.extend_from_slice(b"\r\n");
            encoded.extend_from_slice(data);
            encoded.extend_from_slice(b"\r\n");
            Bytes::from(encoded)
        }

        fn queue_final(&mut self) {
            if !self.final_sent {
                self.chunks.push_back(Bytes::from_static(b"0\r\n\r\n"));
                self.final_sent = true;
                self.state = StreamState::FinalChunkQueued;
            }
        }

        fn cancel(&mut self) {
            self.cancelled = true;
            self.chunks.clear();
            self.state = StreamState::Closed;
        }

        fn tick(&mut self) -> Option<Bytes> {
            if self.cancelled {
                return None;
            }
            match self.state {
                StreamState::HeadersSent => {
                    self.state = StreamState::Streaming;
                    None
                }
                StreamState::Streaming | StreamState::FinalChunkQueued => {
                    let chunk = self.chunks.pop_front();
                    if self.final_sent && self.chunks.is_empty() && chunk.is_none() {
                        self.state = StreamState::Closed;
                    }
                    chunk
                }
                _ => None,
            }
        }

        fn is_complete(&self) -> bool {
            self.state == StreamState::Closed
        }

        fn is_cancelled(&self) -> bool {
            self.cancelled
        }
    }

    #[test]
    fn should_transition_through_stream_states() {
        let mut stream = SimulatedStream::new();
        assert_eq!(stream.state, StreamState::Idle);

        stream.start_stream();
        assert_eq!(stream.state, StreamState::HeadersSent);

        stream.tick();
        assert_eq!(stream.state, StreamState::Streaming);

        stream.queue_chunk(Bytes::from_static(b"hello"));
        stream.queue_chunk(Bytes::from_static(b"world"));
        assert_eq!(stream.chunks.len(), 2);

        stream.queue_final();
        assert_eq!(stream.state, StreamState::FinalChunkQueued);
        assert!(stream.final_sent);
    }

    #[test]
    fn should_cancel_stream_and_clear_pending_chunks() {
        let mut stream = SimulatedStream::new();
        stream.start_stream();
        stream.tick();

        stream.queue_chunk(Bytes::from_static(b"chunk1"));
        stream.queue_chunk(Bytes::from_static(b"chunk2"));
        stream.queue_chunk(Bytes::from_static(b"chunk3"));
        assert_eq!(stream.chunks.len(), 3);

        stream.cancel();
        assert!(stream.is_cancelled());
        assert!(stream.chunks.is_empty());
        assert_eq!(stream.state, StreamState::Closed);
    }

    #[test]
    fn should_ignore_chunks_after_final_sent() {
        let mut stream = SimulatedStream::new();
        stream.start_stream();
        stream.tick();

        stream.queue_chunk(Bytes::from_static(b"first"));
        stream.queue_final();
        assert_eq!(stream.chunks.len(), 2);

        stream.queue_chunk(Bytes::from_static(b"after-final"));
        assert_eq!(stream.chunks.len(), 2);
    }

    #[test]
    fn should_ignore_chunks_after_cancel() {
        let mut stream = SimulatedStream::new();
        stream.start_stream();
        stream.tick();

        stream.queue_chunk(Bytes::from_static(b"before"));
        assert_eq!(stream.chunks.len(), 1);

        stream.cancel();

        stream.queue_chunk(Bytes::from_static(b"after"));
        assert_eq!(stream.chunks.len(), 0);
    }

    #[test]
    fn should_complete_stream_normally() {
        let mut stream = SimulatedStream::new();
        stream.start_stream();
        stream.tick();

        stream.queue_chunk(Bytes::from_static(b"data"));
        stream.queue_final();

        let chunk1 = stream.tick();
        assert!(chunk1.is_some());

        let final_chunk = stream.tick();
        assert!(final_chunk.is_some());
        assert_eq!(&final_chunk.unwrap()[..], b"0\r\n\r\n");

        assert!(stream.tick().is_none());
        assert!(stream.is_complete());
    }
}

mod sse_reconnect {
    use super::*;

    struct SseConnection {
        retry_ms: u32,
        last_event_id: Option<String>,
        events_received: u32,
        reconnect_count: u32,
        connected: bool,
    }

    impl SseConnection {
        fn new(retry_ms: u32) -> Self {
            Self {
                retry_ms,
                last_event_id: None,
                events_received: 0,
                reconnect_count: 0,
                connected: false,
            }
        }

        fn connect(&mut self) {
            self.connected = true;
            if self.reconnect_count > 0 {
                // On reconnect, send Last-Event-ID header
            }
        }

        fn disconnect(&mut self) {
            self.connected = false;
            self.reconnect_count += 1;
        }

        fn receive_event(&mut self, id: Option<String>, _data: &str) {
            if let Some(event_id) = id {
                self.last_event_id = Some(event_id);
            }
            self.events_received += 1;
        }

        fn should_reconnect(&self) -> bool {
            !self.connected && self.retry_ms > 0
        }

        fn get_reconnect_delay(&self) -> Duration {
            Duration::from_millis(self.retry_ms as u64)
        }
    }

    #[test]
    fn should_track_last_event_id_for_reconnect() {
        let mut conn = SseConnection::new(3000);
        conn.connect();

        conn.receive_event(Some("evt-1".to_string()), "first");
        assert_eq!(conn.last_event_id, Some("evt-1".to_string()));

        conn.receive_event(Some("evt-2".to_string()), "second");
        assert_eq!(conn.last_event_id, Some("evt-2".to_string()));

        conn.disconnect();
        assert!(conn.should_reconnect());
        assert_eq!(conn.get_reconnect_delay(), Duration::from_millis(3000));
    }

    #[test]
    fn should_increment_reconnect_count_on_disconnect() {
        let mut conn = SseConnection::new(1000);
        assert_eq!(conn.reconnect_count, 0);

        conn.connect();
        conn.disconnect();
        assert_eq!(conn.reconnect_count, 1);

        conn.connect();
        conn.disconnect();
        assert_eq!(conn.reconnect_count, 2);
    }

    #[test]
    fn should_not_reconnect_with_zero_retry() {
        let mut conn = SseConnection::new(0);
        conn.connect();
        conn.disconnect();

        assert!(!conn.should_reconnect());
        assert_eq!(conn.get_reconnect_delay(), Duration::from_millis(0));
    }

    #[test]
    fn should_format_retry_hint_correctly() {
        let mut buf = Vec::new();
        SseWriter::format_retry(&mut buf, 5000);
        assert_eq!(&buf, b"retry:5000\n\n");
    }

    #[test]
    fn should_format_event_with_id_for_reconnect() {
        let mut buf = Vec::new();
        SseWriter::format_event(&mut buf, Some("message"), "hello", Some("evt-42"));

        let buf_str = String::from_utf8_lossy(&buf);
        assert!(buf_str.starts_with("id:evt-42\n"));
        assert!(buf_str.contains("event:message\n"));
        assert!(buf_str.contains("data:hello\n"));
    }
}

mod producer_lifecycle {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct MockProducer {
        call_count: Rc<RefCell<u32>>,
        return_values: Vec<Option<Bytes>>,
        current_idx: usize,
        cancelled: bool,
    }

    impl MockProducer {
        fn new(call_count: Rc<RefCell<u32>>) -> Self {
            Self {
                call_count,
                return_values: Vec::new(),
                current_idx: 0,
                cancelled: false,
            }
        }

        fn with_returns(call_count: Rc<RefCell<u32>>, values: Vec<Option<Bytes>>) -> Self {
            Self {
                call_count,
                return_values: values,
                current_idx: 0,
                cancelled: false,
            }
        }

        fn call(&mut self) -> Option<Bytes> {
            if self.cancelled {
                return None;
            }
            *self.call_count.borrow_mut() += 1;
            if self.current_idx < self.return_values.len() {
                let result = self.return_values[self.current_idx].clone();
                self.current_idx += 1;
                result
            } else {
                None
            }
        }

        fn cancel(&mut self) {
            self.cancelled = true;
        }
    }

    #[test]
    fn should_call_producer_until_nil() {
        let call_count = Rc::new(RefCell::new(0));
        let mut producer = MockProducer::with_returns(
            call_count.clone(),
            vec![
                Some(Bytes::from_static(b"chunk1")),
                Some(Bytes::from_static(b"chunk2")),
                None,
            ],
        );

        let chunk1 = producer.call();
        assert!(chunk1.is_some());
        assert_eq!(&chunk1.unwrap()[..], b"chunk1");

        let chunk2 = producer.call();
        assert!(chunk2.is_some());

        let end = producer.call();
        assert!(end.is_none());
        assert_eq!(*call_count.borrow(), 3);
    }

    #[test]
    fn should_stop_producing_on_cancel() {
        let call_count = Rc::new(RefCell::new(0));
        let mut producer = MockProducer::with_returns(
            call_count.clone(),
            vec![
                Some(Bytes::from_static(b"a")),
                Some(Bytes::from_static(b"b")),
                Some(Bytes::from_static(b"c")),
                None,
            ],
        );

        let _ = producer.call();
        let _ = producer.call();
        assert_eq!(*call_count.borrow(), 2);

        producer.cancel();

        let result = producer.call();
        assert!(result.is_none());
        assert_eq!(*call_count.borrow(), 2);
    }

    #[test]
    fn should_handle_empty_producer() {
        let call_count = Rc::new(RefCell::new(0));
        let mut producer = MockProducer::with_returns(call_count.clone(), vec![None]);

        let result = producer.call();
        assert!(result.is_none());
        assert_eq!(*call_count.borrow(), 1);
    }
}

mod chunked_encoding {
    use super::*;

    #[test]
    fn should_encode_chunk_with_correct_format() {
        let header = write_chunk_header(5);
        assert_eq!(&header, b"5\r\n");

        let header = write_chunk_header(255);
        assert_eq!(&header, b"ff\r\n");

        let header = write_chunk_header(4096);
        assert_eq!(&header, b"1000\r\n");
    }

    #[test]
    fn should_write_final_chunk_correctly() {
        let final_chunk = write_final_chunk();
        assert_eq!(&final_chunk, b"0\r\n\r\n");
    }

    #[test]
    fn should_handle_large_chunk_sizes() {
        let header = write_chunk_header(16777215);
        assert_eq!(&header, b"ffffff\r\n");
    }
}

mod connection_cleanup {
    use super::*;

    struct ConnectionTracker {
        active_streams: Arc<AtomicU64>,
        total_bytes_sent: Arc<AtomicU64>,
        cancelled_count: Arc<AtomicU64>,
    }

    impl ConnectionTracker {
        fn new() -> Self {
            Self {
                active_streams: Arc::new(AtomicU64::new(0)),
                total_bytes_sent: Arc::new(AtomicU64::new(0)),
                cancelled_count: Arc::new(AtomicU64::new(0)),
            }
        }

        fn start_stream(&self) -> StreamGuard {
            self.active_streams.fetch_add(1, Ordering::Relaxed);
            StreamGuard {
                active_streams: Arc::clone(&self.active_streams),
                total_bytes_sent: Arc::clone(&self.total_bytes_sent),
                cancelled_count: Arc::clone(&self.cancelled_count),
                cancelled: false,
                bytes_sent: 0,
            }
        }

        fn active_count(&self) -> u64 {
            self.active_streams.load(Ordering::Relaxed)
        }

        fn total_bytes(&self) -> u64 {
            self.total_bytes_sent.load(Ordering::Relaxed)
        }

        fn cancelled_count(&self) -> u64 {
            self.cancelled_count.load(Ordering::Relaxed)
        }
    }

    struct StreamGuard {
        active_streams: Arc<AtomicU64>,
        total_bytes_sent: Arc<AtomicU64>,
        cancelled_count: Arc<AtomicU64>,
        cancelled: bool,
        bytes_sent: u64,
    }

    impl StreamGuard {
        fn send_chunk(&mut self, size: u64) {
            self.bytes_sent += size;
            self.total_bytes_sent.fetch_add(size, Ordering::Relaxed);
        }

        fn cancel(&mut self) {
            if !self.cancelled {
                self.cancelled = true;
                self.cancelled_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    impl Drop for StreamGuard {
        fn drop(&mut self) {
            self.active_streams.fetch_sub(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn should_track_active_streams() {
        let tracker = ConnectionTracker::new();
        assert_eq!(tracker.active_count(), 0);

        {
            let _guard1 = tracker.start_stream();
            assert_eq!(tracker.active_count(), 1);

            let _guard2 = tracker.start_stream();
            assert_eq!(tracker.active_count(), 2);
        }

        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn should_track_bytes_sent() {
        let tracker = ConnectionTracker::new();

        let mut guard = tracker.start_stream();
        guard.send_chunk(100);
        guard.send_chunk(200);
        guard.send_chunk(300);

        assert_eq!(tracker.total_bytes(), 600);
    }

    #[test]
    fn should_track_cancellations() {
        let tracker = ConnectionTracker::new();

        let mut guard1 = tracker.start_stream();
        let mut guard2 = tracker.start_stream();
        let guard3 = tracker.start_stream();

        guard1.cancel();
        guard2.cancel();

        assert_eq!(tracker.cancelled_count(), 2);
        assert_eq!(tracker.active_count(), 3);

        drop(guard1);
        drop(guard2);
        drop(guard3);

        assert_eq!(tracker.active_count(), 0);
        assert_eq!(tracker.cancelled_count(), 2);
    }

    #[test]
    fn should_not_double_count_cancellations() {
        let tracker = ConnectionTracker::new();

        let mut guard = tracker.start_stream();
        guard.cancel();
        guard.cancel();
        guard.cancel();

        assert_eq!(tracker.cancelled_count(), 1);
    }
}

mod timeout_handling {
    use super::*;

    struct TimeoutConfig {
        idle_timeout_ms: u64,
        write_timeout_ms: u64,
        keepalive_interval_ms: u64,
    }

    impl Default for TimeoutConfig {
        fn default() -> Self {
            Self {
                idle_timeout_ms: 30000,
                write_timeout_ms: 5000,
                keepalive_interval_ms: 15000,
            }
        }
    }

    struct TimeoutState {
        last_activity: Instant,
        last_write: Instant,
        config: TimeoutConfig,
    }

    impl TimeoutState {
        fn new(config: TimeoutConfig) -> Self {
            let now = Instant::now();
            Self {
                last_activity: now,
                last_write: now,
                config,
            }
        }

        fn record_activity(&mut self) {
            self.last_activity = Instant::now();
        }

        fn record_write(&mut self) {
            self.last_write = Instant::now();
            self.record_activity();
        }

        fn is_idle_timeout(&self) -> bool {
            self.last_activity.elapsed() > Duration::from_millis(self.config.idle_timeout_ms)
        }

        fn is_write_timeout(&self) -> bool {
            self.last_write.elapsed() > Duration::from_millis(self.config.write_timeout_ms)
        }

        fn needs_keepalive(&self) -> bool {
            self.last_write.elapsed() > Duration::from_millis(self.config.keepalive_interval_ms)
        }

        fn simulate_elapsed(&mut self, elapsed: Duration) {
            self.last_activity -= elapsed;
            self.last_write -= elapsed;
        }
    }

    #[test]
    fn should_detect_idle_timeout() {
        let mut state = TimeoutState::new(TimeoutConfig {
            idle_timeout_ms: 100,
            write_timeout_ms: 1000,
            keepalive_interval_ms: 50,
        });

        assert!(!state.is_idle_timeout());

        state.simulate_elapsed(Duration::from_millis(150));
        assert!(state.is_idle_timeout());
    }

    #[test]
    fn should_detect_write_timeout() {
        let mut state = TimeoutState::new(TimeoutConfig {
            idle_timeout_ms: 1000,
            write_timeout_ms: 100,
            keepalive_interval_ms: 50,
        });

        assert!(!state.is_write_timeout());

        state.simulate_elapsed(Duration::from_millis(150));
        assert!(state.is_write_timeout());
    }

    #[test]
    fn should_detect_keepalive_needed() {
        let mut state = TimeoutState::new(TimeoutConfig {
            idle_timeout_ms: 1000,
            write_timeout_ms: 1000,
            keepalive_interval_ms: 50,
        });

        assert!(!state.needs_keepalive());

        state.simulate_elapsed(Duration::from_millis(60));
        assert!(state.needs_keepalive());
    }

    #[test]
    fn should_reset_on_activity() {
        let mut state = TimeoutState::new(TimeoutConfig {
            idle_timeout_ms: 100,
            write_timeout_ms: 100,
            keepalive_interval_ms: 50,
        });

        state.simulate_elapsed(Duration::from_millis(80));
        assert!(!state.is_idle_timeout());

        state.simulate_elapsed(Duration::from_millis(30));
        assert!(state.is_idle_timeout());

        state.record_activity();
        assert!(!state.is_idle_timeout());
    }
}

mod concurrent_cancellation {
    use super::*;

    struct StreamManager {
        streams: Vec<(u64, Arc<AtomicBool>)>,
        next_id: AtomicU64,
    }

    impl StreamManager {
        fn new() -> Self {
            Self {
                streams: Vec::new(),
                next_id: AtomicU64::new(1),
            }
        }

        fn create_stream(&mut self) -> (u64, Arc<AtomicBool>) {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let cancelled = Arc::new(AtomicBool::new(false));
            self.streams.push((id, Arc::clone(&cancelled)));
            (id, cancelled)
        }

        fn cancel_stream(&mut self, id: u64) -> bool {
            for (stream_id, cancelled) in &self.streams {
                if *stream_id == id {
                    cancelled.store(true, Ordering::Release);
                    return true;
                }
            }
            false
        }

        fn cancel_all(&mut self) {
            for (_, cancelled) in &self.streams {
                cancelled.store(true, Ordering::Release);
            }
        }

        fn active_count(&self) -> usize {
            self.streams.len()
        }
    }

    #[test]
    fn should_cancel_individual_stream() {
        let mut manager = StreamManager::new();

        let (id1, cancelled1) = manager.create_stream();
        let (_id2, cancelled2) = manager.create_stream();

        assert!(!cancelled1.load(Ordering::Acquire));
        assert!(!cancelled2.load(Ordering::Acquire));

        assert!(manager.cancel_stream(id1));
        assert!(cancelled1.load(Ordering::Acquire));
        assert!(!cancelled2.load(Ordering::Acquire));
    }

    #[test]
    fn should_cancel_all_streams() {
        let mut manager = StreamManager::new();

        let (_, c1) = manager.create_stream();
        let (_, c2) = manager.create_stream();
        let (_, c3) = manager.create_stream();

        manager.cancel_all();

        assert!(c1.load(Ordering::Acquire));
        assert!(c2.load(Ordering::Acquire));
        assert!(c3.load(Ordering::Acquire));
    }

    #[test]
    fn should_return_false_for_nonexistent_stream() {
        let mut manager = StreamManager::new();
        manager.create_stream();

        assert!(!manager.cancel_stream(999));
    }
}

mod graceful_shutdown {
    use super::*;

    enum ShutdownState {
        Running,
        Draining,
        Shutdown,
    }

    struct GracefulShutdown {
        state: ShutdownState,
        active_streams: u64,
        drain_deadline: Option<Instant>,
        drain_timeout_ms: u64,
    }

    impl GracefulShutdown {
        fn new(drain_timeout_ms: u64) -> Self {
            Self {
                state: ShutdownState::Running,
                active_streams: 0,
                drain_deadline: None,
                drain_timeout_ms,
            }
        }

        fn start_stream(&mut self) -> bool {
            match self.state {
                ShutdownState::Running => {
                    self.active_streams += 1;
                    true
                }
                _ => false,
            }
        }

        fn end_stream(&mut self) {
            if self.active_streams > 0 {
                self.active_streams -= 1;
            }
        }

        fn initiate_shutdown(&mut self) {
            if matches!(self.state, ShutdownState::Running) {
                self.state = ShutdownState::Draining;
                self.drain_deadline =
                    Some(Instant::now() + Duration::from_millis(self.drain_timeout_ms));
            }
        }

        fn tick(&mut self) -> bool {
            match self.state {
                ShutdownState::Draining => {
                    if self.active_streams == 0 {
                        self.state = ShutdownState::Shutdown;
                        return true;
                    }
                    if let Some(deadline) = self.drain_deadline
                        && Instant::now() >= deadline
                    {
                        self.state = ShutdownState::Shutdown;
                        return true;
                    }
                    false
                }
                ShutdownState::Shutdown => true,
                ShutdownState::Running => false,
            }
        }

        fn is_shutdown(&self) -> bool {
            matches!(self.state, ShutdownState::Shutdown)
        }

        fn is_draining(&self) -> bool {
            matches!(self.state, ShutdownState::Draining)
        }
    }

    #[test]
    fn should_reject_new_streams_while_draining() {
        let mut shutdown = GracefulShutdown::new(5000);

        assert!(shutdown.start_stream());
        shutdown.end_stream();

        shutdown.initiate_shutdown();
        assert!(shutdown.is_draining());

        assert!(
            !shutdown.start_stream(),
            "should reject new stream while draining"
        );
    }

    #[test]
    fn should_wait_for_active_streams_to_finish() {
        let mut shutdown = GracefulShutdown::new(5000);

        shutdown.start_stream();
        shutdown.start_stream();
        shutdown.initiate_shutdown();

        assert!(!shutdown.tick(), "should not shutdown with active streams");

        shutdown.end_stream();
        assert!(
            !shutdown.tick(),
            "should not shutdown with one active stream"
        );

        shutdown.end_stream();
        assert!(shutdown.tick(), "should shutdown when all streams finish");
        assert!(shutdown.is_shutdown());
    }

    #[test]
    fn should_force_shutdown_on_timeout() {
        let mut shutdown = GracefulShutdown::new(100);

        shutdown.start_stream();
        shutdown.start_stream();
        shutdown.initiate_shutdown();

        thread::sleep(Duration::from_millis(150));

        assert!(shutdown.tick(), "should force shutdown after timeout");
        assert!(shutdown.is_shutdown());
        assert_eq!(shutdown.active_streams, 2);
    }

    #[test]
    fn should_shutdown_immediately_with_no_streams() {
        let mut shutdown = GracefulShutdown::new(5000);
        shutdown.initiate_shutdown();

        assert!(shutdown.tick());
        assert!(shutdown.is_shutdown());
    }
}

mod backpressure_handling {
    use super::*;

    struct BackpressureBuffer {
        chunks: VecDeque<Bytes>,
        max_chunks: usize,
        total_bytes: u64,
        max_bytes: u64,
        blocked: bool,
    }

    impl BackpressureBuffer {
        fn new(max_chunks: usize, max_bytes: u64) -> Self {
            Self {
                chunks: VecDeque::new(),
                max_chunks,
                total_bytes: 0,
                max_bytes,
                blocked: false,
            }
        }

        fn try_push(&mut self, chunk: Bytes) -> Result<(), Bytes> {
            if self.blocked {
                return Err(chunk);
            }

            if self.chunks.len() >= self.max_chunks {
                self.blocked = true;
                return Err(chunk);
            }

            let new_total = self.total_bytes + chunk.len() as u64;
            if new_total > self.max_bytes {
                self.blocked = true;
                return Err(chunk);
            }

            self.total_bytes = new_total;
            self.chunks.push_back(chunk);
            Ok(())
        }

        fn pop(&mut self) -> Option<Bytes> {
            let chunk = self.chunks.pop_front();
            if let Some(ref c) = chunk {
                self.total_bytes -= c.len() as u64;
            }
            if self.blocked && self.chunks.len() < self.max_chunks / 2 {
                self.blocked = false;
            }
            chunk
        }

        fn is_blocked(&self) -> bool {
            self.blocked
        }

        fn len(&self) -> usize {
            self.chunks.len()
        }
    }

    #[test]
    fn should_block_on_max_chunks() {
        let mut buf = BackpressureBuffer::new(3, 10000);

        assert!(buf.try_push(Bytes::from_static(b"a")).is_ok());
        assert!(buf.try_push(Bytes::from_static(b"b")).is_ok());
        assert!(buf.try_push(Bytes::from_static(b"c")).is_ok());

        assert!(!buf.is_blocked(), "not blocked yet, at limit");
        assert!(buf.try_push(Bytes::from_static(b"d")).is_err());
        assert!(buf.is_blocked(), "blocked after exceeding limit");
    }

    #[test]
    fn should_block_on_max_bytes() {
        let mut buf = BackpressureBuffer::new(100, 10);

        assert!(buf.try_push(Bytes::from_static(b"12345")).is_ok());
        assert!(buf.try_push(Bytes::from_static(b"12345")).is_ok());

        assert!(!buf.is_blocked(), "not blocked yet, at limit");
        assert!(buf.try_push(Bytes::from_static(b"x")).is_err());
        assert!(buf.is_blocked(), "blocked after exceeding limit");
    }

    #[test]
    fn should_unblock_after_drain() {
        let mut buf = BackpressureBuffer::new(3, 10000);

        buf.try_push(Bytes::from_static(b"a")).unwrap();
        buf.try_push(Bytes::from_static(b"b")).unwrap();
        buf.try_push(Bytes::from_static(b"c")).unwrap();
        assert!(buf.try_push(Bytes::from_static(b"d")).is_err());
        assert!(buf.is_blocked());

        buf.pop();
        buf.pop();
        buf.pop();

        assert!(
            !buf.is_blocked(),
            "should unblock after draining below threshold"
        );
    }

    #[test]
    fn should_reject_while_blocked() {
        let mut buf = BackpressureBuffer::new(1, 10000);

        buf.try_push(Bytes::from_static(b"a")).unwrap();
        assert!(buf.try_push(Bytes::from_static(b"b")).is_err());
        assert!(buf.is_blocked());

        assert!(buf.try_push(Bytes::from_static(b"c")).is_err());
        assert!(buf.try_push(Bytes::from_static(b"d")).is_err());
    }
}
