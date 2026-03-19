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
