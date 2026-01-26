use crate::scheduler::SharedScheduler;
use mlua::{AnyUserData, Lua, MetaMethod, UserData, UserDataMethods};
use std::cell::RefCell;

/// Task status tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    Yielded,
    Cancelled,
    Completed,
}

/// A task wraps a Lua thread (coroutine) for async execution
pub struct Task {
    pub(crate) thread: mlua::Thread,
    pub(crate) scheduler: SharedScheduler,
    pub(crate) status: RefCell<TaskStatus>,
    pub(crate) id: usize,
    /// Timer ID if currently scheduled
    pub(crate) timer_id: RefCell<Option<usize>>,
}

impl Task {
    pub fn new(thread: mlua::Thread, scheduler: SharedScheduler, id: usize) -> Self {
        Self {
            thread,
            scheduler,
            status: RefCell::new(TaskStatus::Ready),
            id,
            timer_id: RefCell::new(None),
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_status(&self, status: TaskStatus) {
        *self.status.borrow_mut() = status;
    }

    pub fn get_status(&self) -> TaskStatus {
        *self.status.borrow()
    }

    /// Cancel this task
    pub fn cancel(&self) {
        self.set_status(TaskStatus::Cancelled);

        // Cancel timer if scheduled
        if let Some(timer_id) = *self.timer_id.borrow() {
            self.scheduler.borrow_mut().cancel_task(timer_id);
            *self.timer_id.borrow_mut() = None;
        }
    }
}

impl UserData for Task {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Add a __call metamethod so tasks are callable: task()
        methods.add_meta_function(MetaMethod::Call, |lua, (_ud, args): (AnyUserData, mlua::Value)| {
            let ud = _ud;
            let task = ud.borrow::<Task>()?;

            // Check if cancelled
            if task.get_status() == TaskStatus::Cancelled {
                return Err(mlua::Error::RuntimeError("Task is cancelled".to_string()));
            }

            // Check if already completed
            if task.get_status() == TaskStatus::Completed {
                return Err(mlua::Error::RuntimeError("Task is completed".to_string()));
            }

            // Get coroutine info to see if we can resume
            let thread = task.thread.clone();
            let status = thread.status();

            match status {
                mlua::ThreadStatus::Resumable => {
                    // Resume the coroutine
                    let scheduler = task.scheduler.clone();
                    let _task_id = task.id;

                    // Resume with batching
                    let runtime = crate::lua::helpers::get_runtime(lua)?;
                    runtime.begin_batch();

                    let result: mlua::Result<mlua::MultiValue> = thread.resume(args);

                    // End batch
                    if let Err(e) = runtime.end_batch(lua) {
                        task.set_status(TaskStatus::Completed);
                        return Err(mlua::Error::RuntimeError(format!("Effect error: {:?}", e)));
                    }

                    match result {
                        Ok(values) => {
                            // Check if yielded or completed
                            let new_status = match thread.status() {
                                mlua::ThreadStatus::Resumable => TaskStatus::Yielded,
                                _ => TaskStatus::Completed,
                            };

                            if new_status == TaskStatus::Yielded {
                                // Check if yielded a DelayMarker (from coroutine.yield in wrapper)
                                let values_vec: Vec<mlua::Value> = values.into();
                                if let Some(mlua::Value::UserData(ud)) = values_vec.first() {
                                    if ud.is::<crate::lua::DelayMarker>() {
                                        if let Ok(marker) = ud.borrow::<crate::lua::DelayMarker>() {
                                            task.set_status(TaskStatus::Yielded);
                                            let timer_id = scheduler.borrow_mut().schedule_delay(thread, marker.delay_ms);
                                            *task.timer_id.borrow_mut() = Some(timer_id);
                                            return Ok(mlua::MultiValue::new());
                                        }
                                    }
                                }
                                // Unknown yield - keep as yielded but don't reschedule
                                task.set_status(TaskStatus::Yielded);
                            } else {
                                // Thread completed normally
                                task.set_status(TaskStatus::Completed);
                                *task.timer_id.borrow_mut() = None;
                            }

                            Ok(mlua::MultiValue::new())
                        }
                        Err(e) => {
                            task.set_status(TaskStatus::Completed);
                            *task.timer_id.borrow_mut() = None;
                            Err(e)
                        }
                    }
                }
                _ => {
                    // Thread is dead or can't resume
                    task.set_status(TaskStatus::Completed);
                    *task.timer_id.borrow_mut() = None;
                    Ok(mlua::MultiValue::new())
                }
            }
        });

        // Add cancel() method
        methods.add_method("cancel", |_lua, this, ()| {
            this.cancel();
            Ok(())
        });

        // Add status() method
        methods.add_method("status", |_lua, this, ()| {
            let status = this.get_status();
            let status_str = match status {
                TaskStatus::Ready => "ready",
                TaskStatus::Running => "running",
                TaskStatus::Yielded => "yielded",
                TaskStatus::Cancelled => "cancelled",
                TaskStatus::Completed => "completed",
            };
            Ok(status_str.to_string())
        });
    }
}

/// Cancel a task from Lua
pub fn cancel_task(_lua: &Lua, task_ud: AnyUserData) -> mlua::Result<()> {
    if let Ok(task) = task_ud.borrow::<Task>() {
        task.cancel();
        Ok(())
    } else {
        Err(mlua::Error::RuntimeError("Expected a Task".to_string()))
    }
}

/// Create a new task from a Lua function
///
/// This wraps the user's function in a pure Lua wrapper that overrides
/// rover.delay() to yield directly. This allows users to write natural code
/// like `rover.delay(1000)` without needing explicit `coroutine.yield()`.
pub fn create_task(lua: &Lua, func: mlua::Function) -> mlua::Result<AnyUserData> {
    // Create a wrapped function that overrides rover.delay locally
    // The wrapper loops to handle yields and resumptions
    let wrapped_code = r#"
        return function(user_fn)
            -- Override rover.delay to yield directly
            local old_delay = rover.delay

            local task_delay = function(ms)
                -- Call the original _delay_ms to get the marker
                local marker = rover._delay_ms(ms)
                -- Yield immediately from Lua (not Rust, so no C-call boundary issue)
                return coroutine.yield(marker)
            end

            -- Return a function that uses the overridden delay
            return function(...)
                local args = {...}
                local first_call = true

                while true do
                    -- Temporarily override rover.delay in the global scope
                    rover.delay = task_delay

                    -- Call user function (only pass args on first call)
                    local results
                    if first_call then
                        results = {pcall(user_fn, table.unpack(args))}
                        first_call = false
                    else
                        results = {pcall(user_fn)}
                    end

                    -- Restore original delay
                    rover.delay = old_delay

                    -- Check for errors
                    if not results[1] then
                        error(results[2], 0)
                    end

                    -- Remove pcall status
                    table.remove(results, 1)

                    -- Check if first result is a DelayMarker
                    if #results > 0 and type(results[1]) == "userdata" then
                        local success, delay_ms = pcall(function()
                            return results[1].delay_ms
                        end)

                        if success and type(delay_ms) == "number" then
                            -- This is a DelayMarker, yield it
                            -- coroutine.yield() works here because we're in pure Lua!
                            return coroutine.yield(table.unpack(results))
                        end
                    end

                    -- Not a DelayMarker, function completed normally
                    return table.unpack(results)
                end
            end
        end
    "#;

    let wrapper_factory: mlua::Function = lua.load(wrapped_code).eval().map_err(|e| {
        mlua::Error::RuntimeError(format!("Failed to create wrapper factory: {}", e))
    })?;

    // Create the wrapper and apply it to the user function
    let wrapped_func: mlua::Function = wrapper_factory.call(func)?;

    let scheduler = crate::lua::helpers::get_scheduler(lua)?;
    let thread = lua.create_thread(wrapped_func)?;
    let task_id = scheduler.borrow_mut().next_task_id();
    let task = Task::new(thread, scheduler.clone(), task_id);
    lua.create_userdata(task)
}

/// Run `rover.task.all(...)` - parallel task execution
pub fn task_all(lua: &Lua, args: mlua::MultiValue) -> mlua::Result<mlua::MultiValue> {
    // Collect all Task arguments
    let mut tasks: Vec<AnyUserData> = Vec::new();
    for value in args {
        match value {
            mlua::Value::UserData(ud) => {
                if ud.is::<Task>() {
                    tasks.push(ud);
                } else {
                    return Err(mlua::Error::RuntimeError(
                        "All arguments to rover.task.all must be Tasks".to_string(),
                    ));
                }
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "All arguments to rover.task.all must be Tasks".to_string(),
                ))
            }
        }
    }

    if tasks.is_empty() {
        return Ok(mlua::MultiValue::new());
    }

    // Start all tasks
    for task_ud in &tasks {
        let task = task_ud.borrow::<Task>()?;
        if task.get_status() == TaskStatus::Ready {
            // Call the task to start it
            drop(task); // Drop borrow before calling

            // Create a call string that invokes the task
            let task_value = mlua::Value::UserData(task_ud.clone());
            let result: mlua::Result<mlua::MultiValue> = lua.load("return ...()").call(task_value);

            if let Err(e) = result {
                eprintln!("Error starting task: {:?}", e);
            }
        }
    }

    // Return empty multivalue for now
    // Full Promise.all-like behavior would require waiting for completion
    Ok(mlua::MultiValue::new())
}

/// Module initialization
pub fn register_task_module(lua: &Lua, rover_table: &mlua::Table) -> mlua::Result<()> {
    // First, create the task table with methods
    let task_table = lua.create_table()?;

    // rover.task.cancel(task)
    task_table.set("cancel", lua.create_function(cancel_task)?)?;

    // rover.task.all(...)
    task_table.set("all", lua.create_function(task_all)?)?;

    // Now set the task table to rover.task
    // But we also need rover.task(fn) to create a task
    // So we use a metatable trick: make the table callable
    // Note: __call receives the table as first arg, then the function arg
    task_table.set_metatable(Some({
        let mt = lua.create_table()?;
        mt.set(
            "__call",
            lua.create_function(|lua, (_table, func): (mlua::Value, mlua::Function)| {
                create_task(lua, func)
            })?,
        )?;
        mt
    }))?;

    rover_table.set("task", task_table)?;

    Ok(())
}
