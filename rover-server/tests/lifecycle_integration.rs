//! Integration tests for server lifecycle controls
//!
//! Tests startup, shutdown, and reload lifecycle hooks and phase transitions.

use rover_server::{LifecycleConfig, LifecycleEvent, LifecycleManager, LifecyclePhase};

#[test]
fn should_transition_through_all_phases_during_shutdown() {
    let mut manager = LifecycleManager::new();

    // Initial state
    assert_eq!(manager.current_phase(), LifecyclePhase::Starting);

    // Transition to Running
    manager.transition_to(LifecyclePhase::Running);
    assert_eq!(manager.current_phase(), LifecyclePhase::Running);
    assert!(manager.current_phase().can_accept_connections());
    assert!(manager.current_phase().can_process_requests());
    assert!(!manager.current_phase().is_terminal());

    // Request shutdown
    manager.request_shutdown();
    assert!(manager.is_shutdown_requested());

    // Transition to Draining
    manager.transition_to(LifecyclePhase::Draining);
    assert_eq!(manager.current_phase(), LifecyclePhase::Draining);
    assert!(!manager.current_phase().can_accept_connections());
    assert!(manager.current_phase().can_process_requests()); // Still processing existing

    // Transition to ShuttingDown
    manager.transition_to(LifecyclePhase::ShuttingDown);
    assert_eq!(manager.current_phase(), LifecyclePhase::ShuttingDown);
    assert!(!manager.current_phase().can_process_requests());

    // Transition to Shutdown
    manager.transition_to(LifecyclePhase::Shutdown);
    assert_eq!(manager.current_phase(), LifecyclePhase::Shutdown);
    assert!(!manager.current_phase().can_accept_connections());
    assert!(!manager.current_phase().can_process_requests());
    assert!(manager.current_phase().is_terminal());
}

#[test]
fn should_support_reload_lifecycle() {
    let mut manager = LifecycleManager::new();

    // Start in Running
    manager.transition_to(LifecyclePhase::Running);
    assert_eq!(manager.current_phase(), LifecyclePhase::Running);

    // Request reload
    manager.request_reload();
    assert!(manager.is_reload_requested());

    // Transition through reload phases
    manager.transition_to(LifecyclePhase::Reloading);
    assert_eq!(manager.current_phase(), LifecyclePhase::Reloading);
    assert!(!manager.current_phase().can_accept_connections());

    // Complete reload
    manager.clear_reload_request();
    assert!(!manager.is_reload_requested());

    manager.transition_to(LifecyclePhase::Running);
    assert_eq!(manager.current_phase(), LifecyclePhase::Running);
    assert!(manager.current_phase().can_accept_connections());
}

#[test]
fn should_track_time_in_phase() {
    let mut manager = LifecycleManager::new();

    // Initially no time tracked
    assert!(manager.time_in_current_phase().is_none());

    // Transition starts tracking
    manager.transition_to(LifecyclePhase::Running);
    let time1 = manager.time_in_current_phase().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    let time2 = manager.time_in_current_phase().unwrap();
    assert!(time2 > time1);
}

#[test]
fn should_detect_drain_timeout() {
    let config = LifecycleConfig {
        enabled: true,
        hook_timeout_secs: 30,
        graceful_shutdown: true,
        drain_timeout_secs: 1, // 1 second timeout for test
        reload_on_signal: false,
    };

    let mut manager = LifecycleManager::with_config(config);

    // Not draining, no timeout
    assert!(!manager.is_drain_timeout_exceeded());

    // Transition to Draining
    manager.transition_to(LifecyclePhase::Draining);
    assert!(!manager.is_drain_timeout_exceeded());

    // Wait for timeout
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(manager.is_drain_timeout_exceeded());
}

#[test]
fn should_not_detect_drain_timeout_when_not_draining() {
    let config = LifecycleConfig {
        enabled: true,
        hook_timeout_secs: 30,
        graceful_shutdown: true,
        drain_timeout_secs: 1,
        reload_on_signal: false,
    };

    let mut manager = LifecycleManager::with_config(config);

    // In running phase, no timeout detection
    manager.transition_to(LifecyclePhase::Running);
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(!manager.is_drain_timeout_exceeded());
}

#[test]
fn should_handle_multiple_shutdown_requests_idempotently() {
    let mut manager = LifecycleManager::new();

    assert!(!manager.is_shutdown_requested());

    manager.request_shutdown();
    assert!(manager.is_shutdown_requested());

    // Second request should not change state
    manager.request_shutdown();
    assert!(manager.is_shutdown_requested());
}

#[test]
fn should_handle_multiple_reload_requests_idempotently() {
    let mut manager = LifecycleManager::new();

    assert!(!manager.is_reload_requested());

    manager.request_reload();
    assert!(manager.is_reload_requested());

    // Second request should not change state
    manager.request_reload();
    assert!(manager.is_reload_requested());

    // Clear and verify
    manager.clear_reload_request();
    assert!(!manager.is_reload_requested());
}

#[test]
fn should_disable_hooks_when_configured() {
    let config = LifecycleConfig {
        enabled: false, // Disabled
        hook_timeout_secs: 30,
        graceful_shutdown: true,
        drain_timeout_secs: 30,
        reload_on_signal: false,
    };

    let manager = LifecycleManager::with_config(config);

    // Execute hooks should return Ok without doing anything
    let lua = mlua::Lua::new();
    let result = manager.execute_hooks(&lua, LifecycleEvent::Startup);
    assert!(result.is_ok());
}

#[test]
fn should_map_events_to_correct_phases() {
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
fn should_phase_determine_connection_capabilities() {
    // Can accept connections
    assert!(LifecyclePhase::Running.can_accept_connections());
    assert!(!LifecyclePhase::Starting.can_accept_connections());
    assert!(!LifecyclePhase::Draining.can_accept_connections());
    assert!(!LifecyclePhase::ShuttingDown.can_accept_connections());
    assert!(!LifecyclePhase::Shutdown.can_accept_connections());
    assert!(!LifecyclePhase::Reloading.can_accept_connections());

    // Can process requests
    assert!(LifecyclePhase::Running.can_process_requests());
    assert!(LifecyclePhase::Draining.can_process_requests());
    assert!(!LifecyclePhase::Starting.can_process_requests());
    assert!(!LifecyclePhase::ShuttingDown.can_process_requests());
    assert!(!LifecyclePhase::Shutdown.can_process_requests());
    assert!(!LifecyclePhase::Reloading.can_process_requests());

    // Terminal phases
    assert!(LifecyclePhase::Shutdown.is_terminal());
    assert!(!LifecyclePhase::Running.is_terminal());
    assert!(!LifecyclePhase::Draining.is_terminal());
    assert!(!LifecyclePhase::ShuttingDown.is_terminal());
    assert!(!LifecyclePhase::Starting.is_terminal());
    assert!(!LifecyclePhase::Reloading.is_terminal());
}
