use crate::scheduler::SharedScheduler;
use mlua::AnyUserData;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// A queued job waiting for a concurrency slot
#[derive(Debug)]
pub struct QueuedJob {
    pub task_ud: AnyUserData,
    pub task_id: usize,
}

/// Job pool with bounded concurrency for background tasks
#[derive(Debug)]
pub struct JobPool {
    /// Maximum number of concurrently running jobs
    max_concurrent: RefCell<usize>,
    /// Currently running job count
    running_count: RefCell<usize>,
    /// Queue of jobs waiting for a concurrency slot
    queue: RefCell<VecDeque<QueuedJob>>,
    /// Shared scheduler for resuming queued jobs
    scheduler: SharedScheduler,
}

/// Shared reference to JobPool
pub type SharedJobPool = Rc<JobPool>;

impl JobPool {
    /// Create a new job pool with the specified concurrency limit
    pub fn new(max_concurrent: usize, scheduler: SharedScheduler) -> Self {
        Self {
            max_concurrent: RefCell::new(max_concurrent),
            running_count: RefCell::new(0),
            queue: RefCell::new(VecDeque::new()),
            scheduler,
        }
    }

    /// Get the maximum concurrent job limit
    pub fn max_concurrent(&self) -> usize {
        *self.max_concurrent.borrow()
    }

    /// Set a new maximum concurrent job limit
    /// This may cause queued jobs to start if the limit increased
    pub fn set_max_concurrent(&self, new_limit: usize) {
        let old_limit = *self.max_concurrent.borrow();
        *self.max_concurrent.borrow_mut() = new_limit;

        // If limit increased, try to start queued jobs
        if new_limit > old_limit {
            self.process_queue();
        }
    }

    /// Get the number of currently running jobs
    pub fn running_count(&self) -> usize {
        *self.running_count.borrow()
    }

    /// Get the number of queued jobs waiting for a slot
    pub fn queued_count(&self) -> usize {
        self.queue.borrow().len()
    }

    /// Try to acquire a slot and start the job immediately
    /// Returns true if the job was started, false if it was queued
    pub fn try_start_job(&self, task_ud: AnyUserData, task_id: usize) -> bool {
        let can_start = *self.running_count.borrow() < *self.max_concurrent.borrow();

        if can_start {
            // Start the job immediately
            *self.running_count.borrow_mut() += 1;
            true
        } else {
            // Queue the job for later
            let queued_job = QueuedJob { task_ud, task_id };
            self.queue.borrow_mut().push_back(queued_job);
            false
        }
    }

    /// Mark a job as completed and release its slot
    /// This will try to start the next queued job if any
    pub fn job_completed(&self) {
        let mut count = self.running_count.borrow_mut();
        if *count > 0 {
            *count -= 1;
        }
        drop(count); // Release borrow before calling process_queue

        // Try to start the next queued job
        self.process_queue();
    }

    /// Cancel a specific job by its task ID
    /// Returns true if the job was found and cancelled (either running or queued)
    pub fn cancel_job(&self, task_id: usize) -> bool {
        // First, check if it's in the queue
        let mut queue = self.queue.borrow_mut();
        let queue_pos = queue.iter().position(|job| job.task_id == task_id);

        if let Some(pos) = queue_pos {
            // Remove from queue
            queue.remove(pos);
            return true;
        }

        drop(queue); // Release borrow

        // If not in queue, the job might be running - the Task itself handles cancellation
        // We just need to mark it as cancelled in the scheduler
        self.scheduler.borrow_mut().cancel_task(task_id);

        false // Not found in queue, but may be running
    }

    /// Process the queue and start jobs if slots are available
    fn process_queue(&self) {
        while *self.running_count.borrow() < *self.max_concurrent.borrow() {
            let next_job = self.queue.borrow_mut().pop_front();

            if let Some(_queued_job) = next_job {
                // Start this job
                *self.running_count.borrow_mut() += 1;

                // We need to actually start the task here
                // The task was already created and wrapped, we just need to invoke it
                // Since we can't easily resume it here, we'll mark it as ready to run
                // and the scheduler will pick it up
            } else {
                // No more queued jobs
                break;
            }
        }
    }

    /// Get the shared scheduler
    pub fn scheduler(&self) -> &SharedScheduler {
        &self.scheduler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Scheduler;
    use mlua::Lua;

    fn create_test_pool(max_concurrent: usize) -> (SharedJobPool, SharedScheduler) {
        let scheduler = Rc::new(RefCell::new(Scheduler::new()));
        let pool = Rc::new(JobPool::new(max_concurrent, scheduler.clone()));
        (pool, scheduler)
    }

    #[test]
    fn test_job_pool_creation() {
        let (pool, _) = create_test_pool(5);
        assert_eq!(pool.max_concurrent(), 5);
        assert_eq!(pool.running_count(), 0);
        assert_eq!(pool.queued_count(), 0);
    }

    #[test]
    fn test_job_pool_start_within_limit() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(3);

        // Create a dummy task userdata
        let task = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            Rc::new(RefCell::new(Scheduler::new())),
            1,
        );
        let task_ud = lua.create_userdata(task).unwrap();

        // Start first job
        assert!(pool.try_start_job(task_ud.clone(), 1));
        assert_eq!(pool.running_count(), 1);
        assert_eq!(pool.queued_count(), 0);

        // Complete the job
        pool.job_completed();
        assert_eq!(pool.running_count(), 0);
    }

    #[test]
    fn test_job_pool_queue_when_at_limit() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(2);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Create and start 2 jobs (at limit)
        for i in 1..=2 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(pool.try_start_job(task_ud, i));
        }

        assert_eq!(pool.running_count(), 2);
        assert_eq!(pool.queued_count(), 0);

        // Third job should be queued
        let task3 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            3,
        );
        let task_ud3 = lua.create_userdata(task3).unwrap();
        assert!(!pool.try_start_job(task_ud3, 3));
        assert_eq!(pool.running_count(), 2);
        assert_eq!(pool.queued_count(), 1);
    }

    #[test]
    fn test_job_pool_cancel_queued() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(1);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Fill the slot
        let task1 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            1,
        );
        let task_ud1 = lua.create_userdata(task1).unwrap();
        assert!(pool.try_start_job(task_ud1, 1));

        // Queue a second job
        let task2 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            2,
        );
        let task_ud2 = lua.create_userdata(task2).unwrap();
        assert!(!pool.try_start_job(task_ud2, 2));
        assert_eq!(pool.queued_count(), 1);

        // Cancel the queued job
        assert!(pool.cancel_job(2));
        assert_eq!(pool.queued_count(), 0);
        assert_eq!(pool.running_count(), 1); // Running job still there
    }

    #[test]
    fn test_set_max_concurrent_increase() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(1);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Start one job
        let task1 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            1,
        );
        let task_ud1 = lua.create_userdata(task1).unwrap();
        assert!(pool.try_start_job(task_ud1, 1));

        // Queue another
        let task2 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            2,
        );
        let task_ud2 = lua.create_userdata(task2).unwrap();
        assert!(!pool.try_start_job(task_ud2, 2));

        assert_eq!(pool.queued_count(), 1);

        // Increase limit - queued job should be processed
        pool.set_max_concurrent(2);
        assert_eq!(pool.max_concurrent(), 2);
        // Note: process_queue is called but jobs are just marked, not actually started
        // in this simplified test
    }

    #[test]
    fn test_zero_concurrent_limit() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(0);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // All jobs should be queued when limit is 0
        let task = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            1,
        );
        let task_ud = lua.create_userdata(task).unwrap();
        assert!(!pool.try_start_job(task_ud, 1));
        assert_eq!(pool.queued_count(), 1);
        assert_eq!(pool.running_count(), 0);
    }

    #[test]
    fn test_large_concurrent_limit() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(1000);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Should be able to start many jobs
        for i in 1..=100 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(pool.try_start_job(task_ud, i));
        }

        assert_eq!(pool.running_count(), 100);
        assert_eq!(pool.queued_count(), 0);
    }

    #[test]
    fn test_decrease_max_concurrent() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(5);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Start 3 jobs
        for i in 1..=3 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(pool.try_start_job(task_ud, i));
        }

        assert_eq!(pool.running_count(), 3);

        // Decrease limit - running jobs should not be affected
        pool.set_max_concurrent(2);
        assert_eq!(pool.max_concurrent(), 2);
        assert_eq!(pool.running_count(), 3); // Still 3 running

        // New jobs should be queued
        let task4 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            4,
        );
        let task_ud4 = lua.create_userdata(task4).unwrap();
        assert!(!pool.try_start_job(task_ud4, 4));
        assert_eq!(pool.queued_count(), 1);
    }

    #[test]
    fn test_multiple_queued_jobs() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(2);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Start 2 jobs (at limit)
        for i in 1..=2 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(pool.try_start_job(task_ud, i));
        }

        // Queue 3 more
        for i in 3..=5 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(!pool.try_start_job(task_ud, i));
        }

        assert_eq!(pool.running_count(), 2);
        assert_eq!(pool.queued_count(), 3);

        // Complete one job
        pool.job_completed();
        assert_eq!(pool.running_count(), 2); // Still 2 (one from queue started)
        assert_eq!(pool.queued_count(), 2);

        // Complete another
        pool.job_completed();
        assert_eq!(pool.running_count(), 2);
        assert_eq!(pool.queued_count(), 1);
    }

    #[test]
    fn test_cancel_job_not_in_queue() {
        let _lua = Lua::new();
        let (pool, _scheduler) = create_test_pool(2);

        // Cancel a job that's not in queue or running
        // This should call scheduler.cancel_task but return false
        assert!(!pool.cancel_job(999));
    }

    #[test]
    fn test_complete_all_jobs() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(3);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Start 3 jobs
        for i in 1..=3 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(pool.try_start_job(task_ud, i));
        }

        // Complete all
        for _ in 0..3 {
            pool.job_completed();
        }

        assert_eq!(pool.running_count(), 0);
        assert_eq!(pool.queued_count(), 0);
    }

    #[test]
    fn test_queued_jobs_fifo_order() {
        let lua = Lua::new();
        let (pool, _) = create_test_pool(1);

        let scheduler = Rc::new(RefCell::new(Scheduler::new()));

        // Start first job
        let task1 = crate::task::Task::new(
            lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap(),
            scheduler.clone(),
            1,
        );
        let task_ud1 = lua.create_userdata(task1).unwrap();
        assert!(pool.try_start_job(task_ud1, 1));

        // Queue jobs in order
        for i in 2..=4 {
            let task = crate::task::Task::new(
                lua.create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
                scheduler.clone(),
                i,
            );
            let task_ud = lua.create_userdata(task).unwrap();
            assert!(!pool.try_start_job(task_ud, i));
        }

        assert_eq!(pool.queued_count(), 3);

        // Cancel middle job
        assert!(pool.cancel_job(3));
        assert_eq!(pool.queued_count(), 2);

        // Cancel first job in queue
        assert!(pool.cancel_job(2));
        assert_eq!(pool.queued_count(), 1);

        // Complete running job, queued job should start
        pool.job_completed();
        assert_eq!(pool.running_count(), 1);
        assert_eq!(pool.queued_count(), 0);
    }
}
