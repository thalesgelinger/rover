use crate::job_pool::{JobPool, SharedJobPool};
use crate::scheduler::SharedScheduler;
use crate::task::{Task, TaskStatus, create_task as create_task_internal, start_task};
use mlua::{
    AnyUserData, Function, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataMethods,
    Value,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Global job pool manager for background tasks
/// This manages the shared job pool across all background job operations
pub struct JobManager {
    /// Default concurrency limit (can be overridden per app)
    default_max_concurrent: RefCell<usize>,
    /// Active job pools by app context (if needed for multi-app scenarios)
    pools: RefCell<HashMap<String, SharedJobPool>>,
}

impl JobManager {
    /// Create a new job manager with the specified default concurrency limit
    pub fn new(default_max_concurrent: usize) -> Self {
        Self {
            default_max_concurrent: RefCell::new(default_max_concurrent),
            pools: RefCell::new(HashMap::new()),
        }
    }

    /// Get the default maximum concurrent jobs
    pub fn default_max_concurrent(&self) -> usize {
        *self.default_max_concurrent.borrow()
    }

    /// Set the default maximum concurrent jobs
    pub fn set_default_max_concurrent(&self, limit: usize) {
        *self.default_max_concurrent.borrow_mut() = limit;
    }

    /// Get or create a job pool for the given app context
    pub fn get_or_create_pool(&self, name: &str, scheduler: SharedScheduler) -> SharedJobPool {
        let mut pools = self.pools.borrow_mut();
        if let Some(pool) = pools.get(name) {
            return pool.clone();
        }

        let pool = Rc::new(JobPool::new(self.default_max_concurrent(), scheduler));
        pools.insert(name.to_string(), pool.clone());
        pool
    }

    /// Remove a job pool (for cleanup)
    pub fn remove_pool(&self, name: &str) -> bool {
        self.pools.borrow_mut().remove(name).is_some()
    }
}

impl UserData for JobManager {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Set the maximum concurrent jobs
        methods.add_method("set_max_concurrent", |_lua, this, limit: usize| {
            this.set_default_max_concurrent(limit);
            Ok(())
        });

        // Get the current maximum concurrent jobs
        methods.add_method("get_max_concurrent", |_lua, this, ()| {
            Ok(this.default_max_concurrent())
        });
    }
}

/// BackgroundJob wraps a task and integrates with the job pool for concurrency control
pub struct BackgroundJob {
    /// The underlying task
    task_ud: AnyUserData,
    /// Job pool for concurrency control
    pool: SharedJobPool,
    /// Whether this job is currently queued (waiting for a slot)
    is_queued: RefCell<bool>,
}

impl BackgroundJob {
    pub fn new(task_ud: AnyUserData, pool: SharedJobPool) -> Self {
        Self {
            task_ud,
            pool,
            is_queued: RefCell::new(false),
        }
    }

    /// Get the task ID
    pub fn id(&self) -> Option<usize> {
        self.get_task_id()
    }

    /// Get the task ID (internal)
    fn get_task_id(&self) -> Option<usize> {
        if let Ok(task) = self.task_ud.borrow::<Task>() {
            Some(task.id())
        } else {
            None
        }
    }

    /// Mark as queued
    pub fn set_queued(&self, queued: bool) {
        *self.is_queued.borrow_mut() = queued;
    }

    /// Check if this job is queued
    pub fn is_queued(&self) -> bool {
        *self.is_queued.borrow()
    }
}

impl UserData for BackgroundJob {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Cancel the job (works for both queued and running jobs)
        methods.add_method("cancel", |_lua, this, ()| {
            if let Some(task_id) = this.get_task_id() {
                // Cancel in the pool (handles queued jobs)
                this.pool.cancel_job(task_id);

                // Also cancel the underlying task (handles running jobs)
                if let Ok(task) = this.task_ud.borrow::<Task>() {
                    task.cancel();
                }
            }
            Ok(())
        });

        // Kill alias for cancel
        methods.add_method("kill", |_lua, this, ()| {
            if let Some(task_id) = this.get_task_id() {
                this.pool.cancel_job(task_id);
                if let Ok(task) = this.task_ud.borrow::<Task>() {
                    task.cancel();
                }
            }
            Ok(())
        });

        // Get job ID (returns task ID)
        methods.add_method("id", |_lua, this, ()| Ok(this.get_task_id()));

        // pid() alias for id() - backward compatibility with old task API
        methods.add_method("pid", |_lua, this, ()| Ok(this.get_task_id()));

        // Check if job is queued
        methods.add_method("is_queued", |_lua, this, ()| Ok(this.is_queued()));

        // Forward __call to the underlying task
        methods.add_meta_function(
            MetaMethod::Call,
            |_lua, (ud, args): (AnyUserData, Value)| -> Result<MultiValue> {
                let job = ud.borrow::<BackgroundJob>()?;

                // If queued, we can't call yet
                if job.is_queued() {
                    return Err(mlua::Error::RuntimeError(
                        "Job is queued waiting for a concurrency slot".to_string(),
                    ));
                }

                // Forward to the underlying task's __call
                let task_ud = job.task_ud.clone();
                drop(job); // Release borrow

                let mt = task_ud.metatable()?;
                let call_fn: Function = mt.get(MetaMethod::Call.name())?;
                call_fn.call((task_ud, args))
            },
        );
    }
}

/// Create a background job and add it to the job pool
fn create_background_job(lua: &Lua, func: Function, pool: SharedJobPool) -> Result<AnyUserData> {
    // Create the underlying task
    let task_ud = create_task_internal(lua, func)?;
    let task_id = if let Ok(task) = task_ud.borrow::<Task>() {
        task.id()
    } else {
        return Err(mlua::Error::RuntimeError(
            "Failed to get task ID".to_string(),
        ));
    };

    // Try to start the job in the pool
    let can_start = pool.try_start_job(task_ud.clone(), task_id);

    // Create the background job wrapper
    let job = BackgroundJob::new(task_ud.clone(), pool.clone());

    if can_start {
        // Start the task immediately
        job.set_queued(false);
        start_task(lua, &task_ud)?;
    } else {
        // Mark as queued - will be started when a slot is available
        job.set_queued(true);

        // Store the job in the scheduler's app_data for later processing
        // We'll need to check the queue when jobs complete
    }

    // Store reference to job pool completion tracking
    // We need to wrap the task completion to release the pool slot

    lua.create_userdata(job)
}

/// Start a queued job when a slot becomes available
/// This should be called by the scheduler when processing completed jobs
pub fn start_queued_job(lua: &Lua, job_ud: &AnyUserData) -> Result<()> {
    if let Ok(job) = job_ud.borrow::<BackgroundJob>() {
        if job.is_queued() {
            job.set_queued(false);
            // Start the underlying task
            start_task(lua, &job.task_ud)?;
        }
    }
    Ok(())
}

/// Register the jobs module with Lua (adds rover.jobs, rover.job, rover.spawn)
pub fn register_jobs_module(
    lua: &Lua,
    rover_table: &Table,
    scheduler: SharedScheduler,
) -> Result<()> {
    // Create job manager with default limit of 10 concurrent jobs
    let job_manager = Rc::new(JobManager::new(10));

    // Store scheduler in Lua app_data for access
    lua.set_app_data(scheduler.clone());

    // Get or create the default pool
    let pool = job_manager.get_or_create_pool("default", scheduler);

    // Create the jobs table
    let jobs_table = lua.create_table()?;

    // Store job manager reference
    let job_manager_ud = lua.create_userdata(JobManager::new(10))?;
    jobs_table.set("_manager", job_manager_ud)?;

    // rover.jobs.set_max_concurrent(n) - set the concurrency limit
    let pool_for_set_max = pool.clone();
    jobs_table.set(
        "set_max_concurrent",
        lua.create_function(move |_lua, limit: usize| {
            pool_for_set_max.set_max_concurrent(limit);
            Ok(())
        })?,
    )?;

    // rover.jobs.get_max_concurrent() - get the current limit
    let pool_for_get_max = pool.clone();
    jobs_table.set(
        "get_max_concurrent",
        lua.create_function(move |_lua, ()| Ok(pool_for_get_max.max_concurrent()))?,
    )?;

    // rover.jobs.stats() - get current job pool statistics
    let pool_for_stats = pool.clone();
    jobs_table.set(
        "stats",
        lua.create_function(move |lua, ()| {
            let stats = lua.create_table()?;
            stats.set("max_concurrent", pool_for_stats.max_concurrent())?;
            stats.set("running", pool_for_stats.running_count())?;
            stats.set("queued", pool_for_stats.queued_count())?;
            stats.set(
                "available_slots",
                pool_for_stats
                    .max_concurrent()
                    .saturating_sub(pool_for_stats.running_count()),
            )?;
            Ok(stats)
        })?,
    )?;

    // rover.jobs.cancel(job) - cancel a specific job
    jobs_table.set(
        "cancel",
        lua.create_function(|_lua, job_ud: AnyUserData| {
            if let Ok(job) = job_ud.borrow::<BackgroundJob>() {
                if let Some(task_id) = job.get_task_id() {
                    job.pool.cancel_job(task_id);
                    if let Ok(task) = job.task_ud.borrow::<Task>() {
                        task.cancel();
                    }
                }
            }
            Ok(())
        })?,
    )?;

    // rover.jobs.cancel_all() - cancel all jobs in the pool
    let pool_for_cancel_all = pool.clone();
    jobs_table.set(
        "cancel_all",
        lua.create_function(move |_lua, ()| {
            // Cancel all running tasks in the pool
            // This is a best-effort operation
            pool_for_cancel_all.scheduler().borrow_mut().cancel_task(0); // Cancel all
            Ok(())
        })?,
    )?;

    // Set the jobs table
    rover_table.set("jobs", jobs_table)?;

    // rover.job(fn) - create a background job (doesn't start immediately)
    // This allows for configuration before starting
    let pool_for_job = pool.clone();
    let job_fn = lua.create_function(move |lua, func: Function| {
        create_background_job(lua, func, pool_for_job.clone())
    })?;
    rover_table.set("job", job_fn)?;

    // rover.spawn(fn) - create and immediately start a background job
    // This is the most common use case
    let pool_for_spawn = pool.clone();
    let spawn_fn = lua.create_function(move |lua, func: Function| {
        create_background_job(lua, func, pool_for_spawn.clone())
    })?;
    rover_table.set("spawn", spawn_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Scheduler;
    use crate::signal::SignalRuntime;
    use crate::ui::registry::UiRegistry;
    use std::cell::RefCell;

    fn setup_lua() -> (
        Lua,
        SharedScheduler,
        Rc<SignalRuntime>,
        Rc<RefCell<UiRegistry>>,
    ) {
        let lua = Lua::new();
        let scheduler: SharedScheduler = Rc::new(RefCell::new(Scheduler::new()));
        let runtime = Rc::new(SignalRuntime::new());
        let registry = Rc::new(RefCell::new(UiRegistry::new()));

        lua.set_app_data(scheduler.clone());
        lua.set_app_data(runtime.clone());
        lua.set_app_data(registry.clone());

        // Set up minimal rover table for task creation
        let rover_table = lua.create_table().expect("create rover table");

        // Register the _delay_ms function that tasks need
        let delay_fn = lua
            .create_function(|lua, delay_ms: u64| {
                lua.create_userdata(crate::lua::DelayMarker { delay_ms })
            })
            .expect("create delay function");
        rover_table
            .set("_delay_ms", delay_fn)
            .expect("set _delay_ms");

        // Set delay to also return a marker (tasks need this)
        let delay_wrapper = lua
            .create_function(|lua, delay_ms: u64| {
                lua.create_userdata(crate::lua::DelayMarker { delay_ms })
            })
            .expect("create delay wrapper");
        rover_table.set("delay", delay_wrapper).expect("set delay");

        lua.globals()
            .set("rover", rover_table)
            .expect("set rover global");

        (lua, scheduler, runtime, registry)
    }

    #[test]
    fn test_job_manager() {
        let manager = JobManager::new(5);
        assert_eq!(manager.default_max_concurrent(), 5);

        manager.set_default_max_concurrent(10);
        assert_eq!(manager.default_max_concurrent(), 10);
    }

    #[test]
    fn test_create_background_job() {
        let (lua, scheduler, _runtime, _registry) = setup_lua();
        let pool = Rc::new(JobPool::new(2, scheduler));

        // Create a simple job function
        let func = lua
            .create_function(|_, ()| Ok(()))
            .expect("create function");

        let job_ud = create_background_job(&lua, func, pool.clone()).expect("create job");

        // Check job properties
        let job = job_ud.borrow::<BackgroundJob>().expect("borrow job");
        assert!(!job.is_queued()); // Should start immediately (pool has capacity)
        assert!(job.id().is_some());
    }

    #[test]
    fn test_background_job_queued_when_at_limit() {
        let (lua, scheduler, _runtime, _registry) = setup_lua();
        let pool = Rc::new(JobPool::new(1, scheduler));

        // Create and start first job
        let func1 = lua
            .create_function(|_, ()| Ok(()))
            .expect("create function");
        let _job1 = create_background_job(&lua, func1, pool.clone()).expect("create job 1");

        // Create second job - should be queued
        let func2 = lua
            .create_function(|_, ()| Ok(()))
            .expect("create function");
        let job2_ud = create_background_job(&lua, func2, pool.clone()).expect("create job 2");

        let job2 = job2_ud.borrow::<BackgroundJob>().expect("borrow job 2");
        assert!(job2.is_queued()); // Should be queued (at concurrency limit)
        assert_eq!(pool.queued_count(), 1);
    }

    #[test]
    fn test_jobs_module_registration() {
        let (lua, scheduler, _runtime, _registry) = setup_lua();

        let rover_table = lua.create_table().expect("create rover table");

        // Register the jobs module
        register_jobs_module(&lua, &rover_table, scheduler).expect("register jobs module");

        lua.globals()
            .set("rover", rover_table)
            .expect("set rover global");

        // Test that rover.jobs exists
        let jobs_exists: bool = lua
            .load("return rover.jobs ~= nil")
            .eval()
            .expect("check jobs exists");
        assert!(jobs_exists);

        // Test that rover.spawn exists
        let spawn_exists: bool = lua
            .load("return rover.spawn ~= nil")
            .eval()
            .expect("check spawn exists");
        assert!(spawn_exists);

        // Test that rover.job exists
        let job_exists: bool = lua
            .load("return rover.job ~= nil")
            .eval()
            .expect("check job exists");
        assert!(job_exists);
    }

    #[test]
    fn test_jobs_stats() {
        let (lua, scheduler, _runtime, _registry) = setup_lua();
        let pool = Rc::new(JobPool::new(5, scheduler));

        // Get stats directly from the pool
        assert_eq!(pool.max_concurrent(), 5);
        assert_eq!(pool.running_count(), 0);
        assert_eq!(pool.queued_count(), 0);
    }
}
