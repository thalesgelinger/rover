use rover_server::{LifecyclePhase, ReadinessState, readiness_probe_result};

#[test]
fn should_report_healthy_state_when_running_and_dependencies_ok() {
    let result = readiness_probe_result(LifecyclePhase::Running, &[]);

    assert_eq!(result.state, ReadinessState::Healthy);
    assert_eq!(result.status_code, 200);
    assert_eq!(
        result.body,
        bytes::Bytes::from_static(b"{\"status\":\"ready\"}")
    );
}

#[test]
fn should_report_degraded_state_when_not_accepting_connections() {
    let result = readiness_probe_result(LifecyclePhase::Draining, &[]);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
    assert_eq!(
        result.body,
        bytes::Bytes::from_static(b"{\"status\":\"not_ready\"}")
    );
}

#[test]
fn should_report_dependency_failure_state_with_structured_reasons() {
    let failed_dependencies = vec!["database".to_string(), "redis".to_string()];
    let result = readiness_probe_result(LifecyclePhase::Running, &failed_dependencies);

    assert_eq!(result.state, ReadinessState::DependencyFailure);
    assert_eq!(result.status_code, 503);

    let body: serde_json::Value =
        serde_json::from_slice(result.body.as_ref()).expect("valid readiness JSON body");
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["reasons"][0]["code"], "dependency_unavailable");
    assert_eq!(body["reasons"][0]["dependency"], "database");
    assert_eq!(body["reasons"][1]["code"], "dependency_unavailable");
    assert_eq!(body["reasons"][1]["dependency"], "redis");
}

#[test]
fn should_report_degraded_state_when_starting() {
    let result = readiness_probe_result(LifecyclePhase::Starting, &[]);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
}

#[test]
fn should_report_degraded_state_when_shutting_down() {
    let result = readiness_probe_result(LifecyclePhase::ShuttingDown, &[]);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
}

#[test]
fn should_report_degraded_state_when_shutdown() {
    let result = readiness_probe_result(LifecyclePhase::Shutdown, &[]);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
}

#[test]
fn should_report_degraded_state_when_reloading() {
    let result = readiness_probe_result(LifecyclePhase::Reloading, &[]);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
}

#[test]
fn should_report_degraded_over_dependency_failure_when_draining() {
    let failed_dependencies = vec!["database".to_string()];
    let result = readiness_probe_result(LifecyclePhase::Draining, &failed_dependencies);

    assert_eq!(result.state, ReadinessState::Degraded);
    assert_eq!(result.status_code, 503);
}

#[test]
fn should_handle_single_dependency_failure() {
    let failed_dependencies = vec!["cache".to_string()];
    let result = readiness_probe_result(LifecyclePhase::Running, &failed_dependencies);

    assert_eq!(result.state, ReadinessState::DependencyFailure);
    assert_eq!(result.status_code, 503);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["reasons"].as_array().unwrap().len(), 1);
    assert_eq!(body["reasons"][0]["dependency"], "cache");
}

#[test]
fn should_handle_multiple_dependency_failures() {
    let failed_dependencies = vec![
        "database".to_string(),
        "redis".to_string(),
        "cache".to_string(),
        "message_queue".to_string(),
    ];
    let result = readiness_probe_result(LifecyclePhase::Running, &failed_dependencies);

    assert_eq!(result.state, ReadinessState::DependencyFailure);
    assert_eq!(result.status_code, 503);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    assert_eq!(body["status"], "not_ready");

    let reasons = body["reasons"].as_array().unwrap();
    assert_eq!(reasons.len(), 4);

    let deps: Vec<String> = reasons
        .iter()
        .map(|r| r["dependency"].as_str().unwrap().to_string())
        .collect();
    assert!(deps.contains(&"database".to_string()));
    assert!(deps.contains(&"redis".to_string()));
    assert!(deps.contains(&"cache".to_string()));
    assert!(deps.contains(&"message_queue".to_string()));
}

#[test]
fn should_produce_valid_json_for_healthy_state() {
    let result = readiness_probe_result(LifecyclePhase::Running, &[]);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    assert_eq!(body["status"], "ready");
    assert!(body.as_object().unwrap().contains_key("status"));
}

#[test]
fn should_produce_valid_json_for_degraded_state() {
    let result = readiness_probe_result(LifecyclePhase::Draining, &[]);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    assert_eq!(body["status"], "not_ready");
}

#[test]
fn should_handle_empty_dependency_name() {
    let failed_dependencies = vec!["".to_string()];
    let result = readiness_probe_result(LifecyclePhase::Running, &failed_dependencies);

    assert_eq!(result.state, ReadinessState::DependencyFailure);
    assert_eq!(result.status_code, 503);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    assert_eq!(body["reasons"][0]["dependency"], "");
}

#[test]
fn should_preserve_dependency_order_in_response() {
    let failed_dependencies = vec![
        "z_last".to_string(),
        "a_first".to_string(),
        "m_middle".to_string(),
    ];
    let result = readiness_probe_result(LifecyclePhase::Running, &failed_dependencies);

    let body: serde_json::Value = serde_json::from_slice(result.body.as_ref()).expect("valid JSON");
    let reasons = body["reasons"].as_array().unwrap();

    assert_eq!(reasons[0]["dependency"], "z_last");
    assert_eq!(reasons[1]["dependency"], "a_first");
    assert_eq!(reasons[2]["dependency"], "m_middle");
}
