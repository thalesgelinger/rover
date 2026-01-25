use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

/// Timer entry with wake time and coroutine ID
#[derive(Debug, Clone)]
pub struct Timer {
    pub wake_time: Instant,
    pub coroutine_id: usize,
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.wake_time == other.wake_time
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (earlier times have higher priority)
        other.wake_time.cmp(&self.wake_time)
    }
}

/// Timer queue using BinaryHeap for efficient scheduling
#[derive(Debug)]
pub struct TimerQueue {
    heap: BinaryHeap<Timer>,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    /// Schedule a timer for the given coroutine
    pub fn schedule(&mut self, coroutine_id: usize, wake_time: Instant) {
        self.heap.push(Timer {
            wake_time,
            coroutine_id,
        });
    }

    /// Get the next wake time without removing the timer
    pub fn peek_wake_time(&self) -> Option<Instant> {
        self.heap.peek().map(|timer| timer.wake_time)
    }

    /// Pop all timers that are ready (wake_time <= now)
    pub fn pop_ready(&mut self, now: Instant) -> Vec<usize> {
        let mut ready = Vec::new();

        while let Some(timer) = self.heap.peek() {
            if timer.wake_time <= now {
                let timer = self.heap.pop().unwrap();
                ready.push(timer.coroutine_id);
            } else {
                break;
            }
        }

        ready
    }

    /// Get the number of pending timers
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

impl Default for TimerQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_timer_ordering() {
        let now = Instant::now();
        let timer1 = Timer {
            wake_time: now + Duration::from_millis(100),
            coroutine_id: 1,
        };
        let timer2 = Timer {
            wake_time: now + Duration::from_millis(50),
            coroutine_id: 2,
        };

        // Timer2 should be "greater" (higher priority) because it has earlier wake time
        assert!(timer2 > timer1);
    }

    #[test]
    fn test_timer_queue_order() {
        let now = Instant::now();
        let mut queue = TimerQueue::new();

        // Add timers in arbitrary order
        queue.schedule(1, now + Duration::from_millis(300));
        queue.schedule(2, now + Duration::from_millis(100));
        queue.schedule(3, now + Duration::from_millis(200));

        assert_eq!(queue.len(), 3);

        // Pop ready timers at different times
        let ready = queue.pop_ready(now + Duration::from_millis(150));
        assert_eq!(ready, vec![2]); // Only timer 2 is ready

        let ready = queue.pop_ready(now + Duration::from_millis(250));
        assert_eq!(ready, vec![3]); // Timer 3 is now ready

        let ready = queue.pop_ready(now + Duration::from_millis(350));
        assert_eq!(ready, vec![1]); // Timer 1 is finally ready

        assert!(queue.is_empty());
    }

    #[test]
    fn test_peek_wake_time() {
        let now = Instant::now();
        let mut queue = TimerQueue::new();

        assert!(queue.peek_wake_time().is_none());

        let wake1 = now + Duration::from_millis(100);
        let wake2 = now + Duration::from_millis(50);

        queue.schedule(1, wake1);
        queue.schedule(2, wake2);

        // Should return the earliest wake time
        assert_eq!(queue.peek_wake_time(), Some(wake2));
    }

    #[test]
    fn test_pop_ready_multiple() {
        let now = Instant::now();
        let mut queue = TimerQueue::new();

        // Add multiple timers with the same wake time
        let wake = now + Duration::from_millis(100);
        queue.schedule(1, wake);
        queue.schedule(2, wake);
        queue.schedule(3, wake);

        // All should be popped at once
        let ready = queue.pop_ready(now + Duration::from_millis(100));
        assert_eq!(ready.len(), 3);
        assert!(ready.contains(&1));
        assert!(ready.contains(&2));
        assert!(ready.contains(&3));
    }
}
