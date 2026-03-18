use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use mlua::{Function, Lua, RegistryKey, Value};
use tracing::{debug, info, warn};

/// Lifecycle phases for the server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecyclePhase {
    /// Server is starting up
    Starting,
    /// Server is running normally
    Running,
    /// Server is draining connections (graceful shutdown)
    Draining,
    /// Server is shutting down
    ShuttingDown,
    /// Server has shut down
    Shutdown,
    /// Server is reloading configuration
    Reloading,
}

impl LifecyclePhase {
    /// Returns true if the server can accept new connections in this phase
    pub fn can_accept_connections(&self) -> bool {
        matches!(self, LifecyclePhase::Running)
    }

    /// Returns true if the server should process requests in this phase
    pub fn can_process_requests(&self) -> bool {
        matches!(self, LifecyclePhase::Running | LifecyclePhase::Draining)
    }

    /// Returns true if the server is in a terminal phase
    pub fn is_terminal(&self) -> bool {
        matches!(self, LifecyclePhase::Shutdown)
    }
}

/// Event types for lifecycle hooks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// Server is starting
    Startup,
    /// Server has started and is ready
    Ready,
    /// Server is beginning graceful shutdown
    ShutdownRequested,
    /// Server is draining connections
    Draining,
    /// Server has completed shutdown
    ShutdownComplete,
    /// Configuration reload requested
    ReloadRequested,
    /// Configuration reload completed
    ReloadComplete,
}

impl LifecycleEvent {
    /// Returns the phase associated with this event
    pub fn phase(&self) -> LifecyclePhase {
        match self {
            LifecycleEvent::Startup => LifecyclePhase::Starting,
            LifecycleEvent::Ready => LifecyclePhase::Running,
            LifecycleEvent::ShutdownRequested => LifecyclePhase::Draining,
            LifecycleEvent::Draining => LifecyclePhase::Draining,
            LifecycleEvent::ShutdownComplete => LifecyclePhase::Shutdown,
            LifecycleEvent::ReloadRequested => LifecyclePhase::Reloading,
            LifecycleEvent::ReloadComplete => LifecyclePhase::Running,
        }
    }
}

/// Configuration for lifecycle behavior
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// Enable lifecycle hooks
    pub enabled: bool,
    /// Maximum time to wait for hooks to complete (seconds)
    pub hook_timeout_secs: u64,
    /// Enable graceful shutdown on SIGTERM/SIGINT
    pub graceful_shutdown: bool,
    /// Time to wait for connections to drain (seconds)
    pub drain_timeout_secs: u64,
    /// Enable configuration reload on SIGHUP
    pub reload_on_signal: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: 30,
            reload_on_signal: false,
        }
    }
}

/// A lifecycle hook callback
#[derive(Clone)]
pub struct LifecycleHook {
    pub name: String,
    pub handler: Arc<RegistryKey>,
}

/// Manages server lifecycle events and hooks
pub struct LifecycleManager {
    current_phase: LifecyclePhase,
    config: LifecycleConfig,
    hooks: Vec<(LifecycleEvent, LifecycleHook)>,
    phase_start_time: Option<Instant>,
    shutdown_requested: bool,
    reload_requested: bool,
}

impl LifecycleManager {
    /// Create a new lifecycle manager with default config
    pub fn new() -> Self {
        Self::with_config(LifecycleConfig::default())
    }

    /// Create a new lifecycle manager with custom config
    pub fn with_config(config: LifecycleConfig) -> Self {
        Self {
            current_phase: LifecyclePhase::Starting,
            config,
            hooks: Vec::new(),
            phase_start_time: None,
            shutdown_requested: false,
            reload_requested: false,
        }
    }

    /// Register a hook for a specific lifecycle event
    pub fn register_hook(
        &mut self,
        event: LifecycleEvent,
        name: String,
        handler: Arc<RegistryKey>,
    ) {
        let name_clone = name.clone();
        self.hooks.push((event, LifecycleHook { name, handler }));
        debug!(
            "Registered lifecycle hook '{}' for event {:?}",
            name_clone, event
        );
    }

    /// Get the current lifecycle phase
    pub fn current_phase(&self) -> LifecyclePhase {
        self.current_phase
    }

    /// Check if shutdown has been requested
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Check if reload has been requested
    pub fn is_reload_requested(&self) -> bool {
        self.reload_requested
    }

    /// Request graceful shutdown
    pub fn request_shutdown(&mut self) {
        if !self.shutdown_requested {
            info!("Shutdown requested");
            self.shutdown_requested = true;
        }
    }

    /// Request configuration reload
    pub fn request_reload(&mut self) {
        if !self.reload_requested {
            info!("Reload requested");
            self.reload_requested = true;
        }
    }

    /// Clear reload request flag
    pub fn clear_reload_request(&mut self) {
        self.reload_requested = false;
    }

    /// Transition to a new phase
    pub fn transition_to(&mut self, phase: LifecyclePhase) {
        if self.current_phase != phase {
            info!(
                "Lifecycle transition: {:?} -> {:?}",
                self.current_phase, phase
            );
            self.current_phase = phase;
            self.phase_start_time = Some(Instant::now());
        }
    }

    /// Get the duration spent in the current phase
    pub fn time_in_current_phase(&self) -> Option<Duration> {
        self.phase_start_time.map(|start| start.elapsed())
    }

    /// Execute all hooks registered for a specific event
    pub fn execute_hooks(&self, lua: &Lua, event: LifecycleEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let event_hooks: Vec<_> = self
            .hooks
            .iter()
            .filter(|(e, _)| *e == event)
            .map(|(_, hook)| hook.clone())
            .collect();

        if event_hooks.is_empty() {
            return Ok(());
        }

        info!(
            "Executing {} hooks for event {:?}",
            event_hooks.len(),
            event
        );
        let timeout = Duration::from_secs(self.config.hook_timeout_secs);
        let start = Instant::now();

        for hook in event_hooks {
            if start.elapsed() > timeout {
                warn!("Hook execution timeout reached after {:?}", timeout);
                break;
            }

            debug!("Executing lifecycle hook '{}'", hook.name);

            let handler: Function = lua.registry_value(&hook.handler)?;
            let result = handler.call::<Value>(());

            if let Err(e) = result {
                warn!("Lifecycle hook '{}' failed: {}", hook.name, e);
            } else {
                debug!("Lifecycle hook '{}' completed successfully", hook.name);
            }
        }

        Ok(())
    }

    /// Check if drain timeout has been exceeded
    pub fn is_drain_timeout_exceeded(&self) -> bool {
        if self.current_phase != LifecyclePhase::Draining {
            return false;
        }

        if let Some(duration) = self.time_in_current_phase() {
            duration > Duration::from_secs(self.config.drain_timeout_secs)
        } else {
            false
        }
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_create_lifecycle_manager_with_default_config() {
        let manager = LifecycleManager::new();
        assert_eq!(manager.current_phase(), LifecyclePhase::Starting);
        assert!(manager.config.enabled);
        assert_eq!(manager.config.hook_timeout_secs, 30);
        assert!(manager.config.graceful_shutdown);
        assert_eq!(manager.config.drain_timeout_secs, 30);
    }

    #[test]
    fn should_create_lifecycle_manager_with_custom_config() {
        let config = LifecycleConfig {
            enabled: false,
            hook_timeout_secs: 60,
            graceful_shutdown: false,
            drain_timeout_secs: 45,
            reload_on_signal: true,
        };
        let manager = LifecycleManager::with_config(config);
        assert!(!manager.config.enabled);
        assert_eq!(manager.config.hook_timeout_secs, 60);
        assert!(!manager.config.graceful_shutdown);
        assert_eq!(manager.config.drain_timeout_secs, 45);
        assert!(manager.config.reload_on_signal);
    }

    #[test]
    fn should_transition_between_phases() {
        let mut manager = LifecycleManager::new();
        assert_eq!(manager.current_phase(), LifecyclePhase::Starting);

        manager.transition_to(LifecyclePhase::Running);
        assert_eq!(manager.current_phase(), LifecyclePhase::Running);

        manager.transition_to(LifecyclePhase::Draining);
        assert_eq!(manager.current_phase(), LifecyclePhase::Draining);

        manager.transition_to(LifecyclePhase::Shutdown);
        assert_eq!(manager.current_phase(), LifecyclePhase::Shutdown);
    }

    #[test]
    fn should_track_shutdown_request() {
        let mut manager = LifecycleManager::new();
        assert!(!manager.is_shutdown_requested());

        manager.request_shutdown();
        assert!(manager.is_shutdown_requested());

        // Multiple requests should not change state
        manager.request_shutdown();
        assert!(manager.is_shutdown_requested());
    }

    #[test]
    fn should_track_reload_request() {
        let mut manager = LifecycleManager::new();
        assert!(!manager.is_reload_requested());

        manager.request_reload();
        assert!(manager.is_reload_requested());

        manager.clear_reload_request();
        assert!(!manager.is_reload_requested());
    }

    #[test]
    fn should_determine_connection_acceptance() {
        assert!(LifecyclePhase::Running.can_accept_connections());
        assert!(!LifecyclePhase::Starting.can_accept_connections());
        assert!(!LifecyclePhase::Draining.can_accept_connections());
        assert!(!LifecyclePhase::Shutdown.can_accept_connections());
        assert!(!LifecyclePhase::ShuttingDown.can_accept_connections());
        assert!(!LifecyclePhase::Reloading.can_accept_connections());
    }

    #[test]
    fn should_determine_request_processing() {
        assert!(LifecyclePhase::Running.can_process_requests());
        assert!(LifecyclePhase::Draining.can_process_requests());
        assert!(!LifecyclePhase::Starting.can_process_requests());
        assert!(!LifecyclePhase::ShuttingDown.can_process_requests());
        assert!(!LifecyclePhase::Shutdown.can_process_requests());
        assert!(!LifecyclePhase::Reloading.can_process_requests());
    }

    #[test]
    fn should_identify_terminal_phases() {
        assert!(LifecyclePhase::Shutdown.is_terminal());
        assert!(!LifecyclePhase::Running.is_terminal());
        assert!(!LifecyclePhase::Draining.is_terminal());
        assert!(!LifecyclePhase::Starting.is_terminal());
    }

    #[test]
    fn should_map_events_to_phases() {
        assert_eq!(LifecycleEvent::Startup.phase(), LifecyclePhase::Starting);
        assert_eq!(LifecycleEvent::Ready.phase(), LifecyclePhase::Running);
        assert_eq!(
            LifecycleEvent::ShutdownRequested.phase(),
            LifecyclePhase::Draining
        );
        assert_eq!(LifecycleEvent::Draining.phase(), LifecyclePhase::Draining);
        assert_eq!(
            LifecycleEvent::ShutdownComplete.phase(),
            LifecyclePhase::Shutdown
        );
        assert_eq!(
            LifecycleEvent::ReloadRequested.phase(),
            LifecyclePhase::Reloading
        );
        assert_eq!(
            LifecycleEvent::ReloadComplete.phase(),
            LifecyclePhase::Running
        );
    }

    #[test]
    fn should_track_time_in_phase() {
        let mut manager = LifecycleManager::new();
        manager.transition_to(LifecyclePhase::Running);

        std::thread::sleep(Duration::from_millis(10));

        let time = manager.time_in_current_phase().unwrap();
        assert!(time >= Duration::from_millis(10));
    }

    #[test]
    fn should_not_detect_drain_timeout_when_not_draining() {
        let mut manager = LifecycleManager::new();
        manager.transition_to(LifecyclePhase::Running);
        assert!(!manager.is_drain_timeout_exceeded());
    }

    #[test]
    fn should_detect_drain_timeout_exceeded() {
        let config = LifecycleConfig {
            enabled: true,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: 1,
            reload_on_signal: false,
        };
        let mut manager = LifecycleManager::with_config(config);

        // Not draining yet
        manager.transition_to(LifecyclePhase::Running);
        assert!(!manager.is_drain_timeout_exceeded());

        // Now draining, but not timed out yet
        manager.transition_to(LifecyclePhase::Draining);
        assert!(!manager.is_drain_timeout_exceeded());

        // Wait for timeout
        std::thread::sleep(Duration::from_secs(2));
        assert!(manager.is_drain_timeout_exceeded());
    }

    #[test]
    fn should_handle_zero_drain_timeout() {
        let config = LifecycleConfig {
            enabled: true,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: 0,
            reload_on_signal: false,
        };
        let mut manager = LifecycleManager::with_config(config);

        manager.transition_to(LifecyclePhase::Draining);
        // With zero timeout, should immediately be exceeded
        std::thread::sleep(Duration::from_millis(10));
        assert!(manager.is_drain_timeout_exceeded());
    }

    #[test]
    fn should_handle_phase_reload_transitions() {
        let mut manager = LifecycleManager::new();

        // Start at Starting
        assert_eq!(manager.current_phase(), LifecyclePhase::Starting);

        // To Running
        manager.transition_to(LifecyclePhase::Running);
        assert_eq!(manager.current_phase(), LifecyclePhase::Running);

        // To Reloading
        manager.transition_to(LifecyclePhase::Reloading);
        assert_eq!(manager.current_phase(), LifecyclePhase::Reloading);
        assert!(!manager.current_phase().can_accept_connections());
        assert!(!manager.current_phase().can_process_requests());

        // Back to Running
        manager.transition_to(LifecyclePhase::Running);
        assert_eq!(manager.current_phase(), LifecyclePhase::Running);
        assert!(manager.current_phase().can_accept_connections());
    }

    #[test]
    fn should_handle_all_phase_transitions() {
        let mut manager = LifecycleManager::new();
        let phases = vec![
            LifecyclePhase::Starting,
            LifecyclePhase::Running,
            LifecyclePhase::Draining,
            LifecyclePhase::ShuttingDown,
            LifecyclePhase::Shutdown,
        ];

        for phase in phases {
            manager.transition_to(phase);
            assert_eq!(manager.current_phase(), phase);
        }
    }

    #[test]
    fn should_handle_repeated_same_phase_transition() {
        let mut manager = LifecycleManager::new();
        manager.transition_to(LifecyclePhase::Running);
        manager.transition_to(LifecyclePhase::Running);
        manager.transition_to(LifecyclePhase::Running);
        assert_eq!(manager.current_phase(), LifecyclePhase::Running);
    }

    #[test]
    fn should_register_and_execute_hooks() {
        let lua = Lua::new();
        let mut manager = LifecycleManager::new();

        // Create a simple hook function
        let hook_fn = lua.create_function(|_, ()| Ok(())).unwrap();
        let key = lua.create_registry_value(hook_fn).unwrap();
        manager.register_hook(
            LifecycleEvent::Startup,
            "test_hook".to_string(),
            Arc::new(key),
        );

        // Execute hooks - should not error
        let result = manager.execute_hooks(&lua, LifecycleEvent::Startup);
        assert!(result.is_ok());
    }

    #[test]
    fn should_not_execute_hooks_when_disabled() {
        let lua = Lua::new();
        let config = LifecycleConfig {
            enabled: false,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: 30,
            reload_on_signal: false,
        };
        let mut manager = LifecycleManager::with_config(config);

        let hook_fn = lua.create_function(|_, ()| Ok(())).unwrap();
        let key = lua.create_registry_value(hook_fn).unwrap();
        manager.register_hook(
            LifecycleEvent::Startup,
            "test_hook".to_string(),
            Arc::new(key),
        );

        // Execute hooks - should return Ok immediately without running hooks
        let result = manager.execute_hooks(&lua, LifecycleEvent::Startup);
        assert!(result.is_ok());
    }

    #[test]
    fn should_handle_hook_execution_with_no_hooks() {
        let lua = Lua::new();
        let manager = LifecycleManager::new();

        // Execute hooks for an event with no registered hooks
        let result = manager.execute_hooks(&lua, LifecycleEvent::Ready);
        assert!(result.is_ok());
    }
}
