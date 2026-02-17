mod timer;

use mlua::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

pub use timer::TimerQueue;

/// Shared scheduler wrapper for use in Lua app_data
pub type SharedScheduler = Rc<RefCell<Scheduler>>;

/// Pending coroutine waiting to be resumed
#[derive(Debug)]
pub struct PendingCoroutine {
    pub thread: LuaThread,
}

/// Scheduler manages coroutine execution with timer-based scheduling
#[derive(Debug)]
pub struct Scheduler {
    timers: TimerQueue,
    pending: HashMap<usize, PendingCoroutine>,
    next_id: usize,
    /// Track cancelled threads (by thread pointer identity)
    cancelled: HashSet<usize>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            timers: TimerQueue::new(),
            pending: HashMap::new(),
            next_id: 1,
            cancelled: HashSet::new(),
        }
    }

    /// Get the next task ID (without incrementing)
    pub fn next_task_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Cancel a task - marks the ID as cancelled
    pub fn cancel_task(&mut self, task_id: usize) {
        self.cancelled.insert(task_id);
        // Also remove from pending if it's there
        self.pending.remove(&task_id);
    }

    /// Check if a task is cancelled
    pub fn is_cancelled(&self, task_id: usize) -> bool {
        self.cancelled.contains(&task_id)
    }

    /// Schedule a coroutine to resume after a delay
    pub fn schedule_delay(&mut self, thread: LuaThread, delay_ms: u64) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        self.schedule_delay_with_id(id, thread, delay_ms)
    }

    /// Schedule a coroutine to resume after a delay using a fixed ID
    pub fn schedule_delay_with_id(&mut self, id: usize, thread: LuaThread, delay_ms: u64) -> usize {
        // Replace any existing pending entry for this ID
        self.pending.remove(&id);

        let wake_time = Instant::now() + std::time::Duration::from_millis(delay_ms);
        self.timers.schedule(id, wake_time);
        self.pending.insert(id, PendingCoroutine { thread });

        // eprintln!("Scheduled coroutine {} with {}ms delay (wake after {:?})", id, delay_ms, wake_time);

        id
    }

    /// Tick the scheduler and return IDs of coroutines ready to resume
    pub fn tick(&mut self, now: Instant) -> Vec<usize> {
        self.timers.pop_ready(now)
    }

    /// Get the next wake time for sleeping
    pub fn next_wake_time(&self) -> Option<Instant> {
        self.timers.peek_wake_time()
    }

    /// Take a pending coroutine by ID (removes it from pending map)
    pub fn take_pending(&mut self, id: usize) -> LuaResult<PendingCoroutine> {
        self.pending
            .remove(&id)
            .ok_or_else(|| LuaError::RuntimeError(format!("Coroutine {} not found", id)))
    }

    /// Check if there are any pending coroutines
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get count of pending coroutines
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_scheduler_schedule_and_tick() {
        let lua = Lua::new();
        let mut scheduler = Scheduler::new();

        let thread1 = lua
            .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        let thread2 = lua
            .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();

        // Schedule with different delays
        let id1 = scheduler.schedule_delay(thread1, 100);
        let id2 = scheduler.schedule_delay(thread2, 50);

        assert_eq!(scheduler.pending_count(), 2);

        let now = Instant::now();

        // Tick before any timers are ready
        let ready = scheduler.tick(now);
        assert_eq!(ready.len(), 0);

        // Tick when second timer is ready
        let ready = scheduler.tick(now + Duration::from_millis(60));
        assert_eq!(ready, vec![id2]);

        // Tick when first timer is ready
        let ready = scheduler.tick(now + Duration::from_millis(110));
        assert_eq!(ready, vec![id1]);
    }

    #[test]
    fn test_scheduler_take_pending() {
        let lua = Lua::new();
        let mut scheduler = Scheduler::new();

        let thread = lua
            .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        let id = scheduler.schedule_delay(thread, 100);

        assert_eq!(scheduler.pending_count(), 1);

        let pending = scheduler.take_pending(id).unwrap();
        assert!(pending.thread.status() == LuaThreadStatus::Resumable);
        assert_eq!(scheduler.pending_count(), 0);

        // Taking again should fail
        assert!(scheduler.take_pending(id).is_err());
    }

    #[test]
    fn test_scheduler_next_wake_time() {
        let lua = Lua::new();
        let mut scheduler = Scheduler::new();

        assert!(scheduler.next_wake_time().is_none());

        let thread = lua
            .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        let now = Instant::now();
        scheduler.schedule_delay(thread, 100);

        let wake_time = scheduler.next_wake_time().unwrap();
        let expected = now + Duration::from_millis(100);

        // Allow small timing variance
        assert!(wake_time >= expected && wake_time <= expected + Duration::from_millis(10));
    }

    #[test]
    fn test_scheduler_has_pending() {
        let lua = Lua::new();
        let mut scheduler = Scheduler::new();

        assert!(!scheduler.has_pending());

        let thread = lua
            .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        scheduler.schedule_delay(thread, 100);

        assert!(scheduler.has_pending());
    }
}
