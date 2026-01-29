use crate::SharedSignalRuntime;
use crate::lua::DelayMarker;
use mlua::prelude::*;

/// Result of running a coroutine
#[derive(Debug)]
pub enum CoroutineResult {
    /// Coroutine completed successfully
    Completed,
    /// Coroutine yielded with a delay request
    YieldedDelay { delay_ms: u64 },
    /// Coroutine yielded with another value (not currently handled)
    YieldedOther,
}

impl CoroutineResult {
    /// Check if the coroutine can be resumed
    pub fn is_resumable(&self) -> bool {
        matches!(
            self,
            CoroutineResult::YieldedDelay { .. } | CoroutineResult::YieldedOther
        )
    }
}

/// Run a coroutine with signal batching
///
/// This wraps the coroutine execution in begin_batch/end_batch to ensure
/// all signal updates are atomic and effects run after all updates complete.
///
/// Follows the rover-server pattern for coroutine execution.
/// Takes a reference to the thread so it can be re-used after yielding.
pub fn run_coroutine(
    lua: &Lua,
    runtime: &SharedSignalRuntime,
    thread: &LuaThread,
    args: LuaValue,
) -> LuaResult<CoroutineResult> {
    // Begin signal batch before resuming
    runtime.begin_batch();

    // Resume the coroutine
    let result = thread.resume::<LuaMultiValue>(args);

    // End signal batch and run pending effects
    runtime
        .end_batch(lua)
        .map_err(|e| LuaError::RuntimeError(format!("Effect error: {:?}", e)))?;

    // Analyze the result
    match result {
        Ok(_values) => {
            // Check thread status
            match thread.status() {
                LuaThreadStatus::Resumable => {
                    // Thread yielded - return YieldedOther for basic implementation
                    Ok(CoroutineResult::YieldedOther)
                }
                LuaThreadStatus::Finished => {
                    // Thread completed
                    Ok(CoroutineResult::Completed)
                }
                LuaThreadStatus::Error => Err(LuaError::RuntimeError(
                    "Coroutine is in error state".to_string(),
                )),
                _ => {
                    // Running state - shouldn't happen here
                    Ok(CoroutineResult::YieldedOther)
                }
            }
        }
        Err(e) => {
            // Check if the error is actually a yield with DelayMarker
            // This is a bit of a hack - we need to detect the yield value
            // The proper way is to check the returned values from resume()
            Err(e)
        }
    }
}

/// Run a coroutine that may yield a DelayMarker
///
/// This version properly detects DelayMarker in the yield values.
/// Takes a reference to the thread so it can be re-used after yielding.
pub fn run_coroutine_with_delay(
    lua: &Lua,
    runtime: &SharedSignalRuntime,
    thread: &LuaThread,
    args: LuaValue,
) -> LuaResult<CoroutineResult> {
    // Begin signal batch before resuming
    runtime.begin_batch();

    // Resume the coroutine and capture yield values
    let result = thread.resume::<LuaMultiValue>(args);

    // End signal batch and run pending effects
    runtime
        .end_batch(lua)
        .map_err(|e| LuaError::RuntimeError(format!("Effect error: {:?}", e)))?;

    // Analyze the result
    match result {
        Ok(values) => {
            match thread.status() {
                LuaThreadStatus::Resumable => {
                    // Thread yielded - check the first yield value
                    if let Some(value) = values.into_iter().next() {
                        // Try to extract DelayMarker
                        if let Some(marker) = value.as_userdata() {
                            if let Ok(delay_marker) = marker.borrow::<DelayMarker>() {
                                return Ok(CoroutineResult::YieldedDelay {
                                    delay_ms: delay_marker.delay_ms,
                                });
                            }
                        }
                    }
                    Ok(CoroutineResult::YieldedOther)
                }
                LuaThreadStatus::Finished => {
                    // Thread completed
                    Ok(CoroutineResult::Completed)
                }
                LuaThreadStatus::Error => Err(LuaError::RuntimeError(
                    "Coroutine is in error state".to_string(),
                )),
                _ => {
                    // Running state - shouldn't happen here
                    Ok(CoroutineResult::YieldedOther)
                }
            }
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::SignalRuntime;
    use std::rc::Rc;

    #[test]
    fn test_coroutine_completed() {
        let lua = Lua::new();
        let runtime = Rc::new(SignalRuntime::new());

        // Store runtime in Lua app_data
        lua.set_app_data(runtime.clone());

        // Create a coroutine that completes immediately
        let func = lua
            .load("return function() return 42 end")
            .eval::<LuaFunction>()
            .unwrap();
        let thread = lua.create_thread(func).unwrap();

        let result = run_coroutine_with_delay(&lua, &runtime, &thread, LuaValue::Nil).unwrap();
        assert!(matches!(result, CoroutineResult::Completed));
    }

    #[test]
    fn test_coroutine_yield_delay() {
        let lua = Lua::new();
        let runtime = Rc::new(SignalRuntime::new());

        lua.set_app_data(runtime.clone());

        // Register the delay function
        let delay_fn = lua
            .create_function(|lua, delay_ms: u64| {
                let marker = DelayMarker { delay_ms };
                lua.create_userdata(marker)
            })
            .unwrap();
        lua.globals().set("test_delay", delay_fn).unwrap();

        // Create a coroutine that yields a delay
        let func = lua
            .load(
                r#"
            return function()
                coroutine.yield(test_delay(1000))
                return 42
            end
        "#,
            )
            .eval::<LuaFunction>()
            .unwrap();
        let thread = lua.create_thread(func).unwrap();

        let result = run_coroutine_with_delay(&lua, &runtime, &thread, LuaValue::Nil).unwrap();
        match result {
            CoroutineResult::YieldedDelay { delay_ms } => {
                assert_eq!(delay_ms, 1000);
            }
            _ => panic!("Expected YieldedDelay"),
        }
    }

    #[test]
    fn test_coroutine_signal_batching() {
        let lua = Lua::new();
        let runtime = Rc::new(SignalRuntime::new());

        lua.set_app_data(runtime.clone());

        // Create a signal
        let signal_id = runtime.create_signal(crate::signal::SignalValue::Float(0.0));

        // Create an effect that tracks signal reads
        let effect_called = Rc::new(std::cell::RefCell::new(false));
        let effect_called_clone = effect_called.clone();

        let effect_fn = lua
            .create_function(move |lua, _: ()| {
                let runtime: SharedSignalRuntime =
                    lua.app_data_ref::<SharedSignalRuntime>().unwrap().clone();
                let _value = runtime.get_signal(&lua, signal_id)?;
                *effect_called_clone.borrow_mut() = true;
                Ok(())
            })
            .unwrap();

        let effect_key = lua.create_registry_value(effect_fn).unwrap();
        let _effect_id = runtime.create_effect(&lua, effect_key).unwrap();

        // Create a coroutine that updates the signal
        let func = lua
            .create_function(move |lua, _: ()| {
                let runtime: SharedSignalRuntime =
                    lua.app_data_ref::<SharedSignalRuntime>().unwrap().clone();
                runtime.set_signal(&lua, signal_id, crate::signal::SignalValue::Float(42.0));
                Ok(())
            })
            .unwrap();
        let thread = lua.create_thread(func).unwrap();

        // Reset effect tracking
        *effect_called.borrow_mut() = false;

        // Run coroutine - effect should be called during end_batch
        let _result = run_coroutine(&lua, &runtime, &thread, LuaValue::Nil).unwrap();

        // Effect should have been called
        assert!(*effect_called.borrow());
    }
}
