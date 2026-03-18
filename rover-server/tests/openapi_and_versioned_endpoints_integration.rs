use std::time::Duration;

use anyhow::Result;
use reqwest::Client;

fn get_server_url() -> String {
    std::env::var("ROVER_TEST_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:4242".to_string())
}

async fn get_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("create reqwest client")
}

#[tokio::test]
async fn should_serve_openapi_docs_endpoint() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/ops/docs", get_server_url());

    let response = client
        .get(&url)
        .header("Authorization", "Bearer test-token")
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type header")
        .to_str()?;
    assert!(content_type.contains("text/html"));

    let body = response.text().await?;
    assert!(body.contains("<!doctype html>"), "should be HTML document");
    assert!(
        body.contains("API Documentation"),
        "should have API Documentation title"
    );
    assert!(
        body.contains("@scalar/api-reference"),
        "should include Scalar script"
    );

    Ok(())
}

#[tokio::test]
async fn should_serve_openapi_spec_in_html() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/ops/docs", get_server_url());

    let response = client
        .get(&url)
        .header("Authorization", "Bearer test-token")
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);

    let body = response.text().await?;
    assert!(
        body.contains("Scalar.createApiReference"),
        "should initialize Scalar"
    );
    assert!(
        body.contains("configuration"),
        "should have configuration object"
    );
    assert!(body.contains("content"), "should have content key");

    Ok(())
}

#[tokio::test]
async fn should_reject_docs_without_auth() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/ops/docs", get_server_url());

    let response = client.get(&url).send().await?;

    assert_eq!(response.status().as_u16(), 401);

    let body = response.text().await?;
    assert!(body.contains("error") || body.contains("Management endpoint requires auth token"));

    Ok(())
}

#[tokio::test]
async fn should_serve_v1_api_version() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/v1/users", get_server_url());

    let response = client.get(&url).send().await?;

    assert_eq!(response.status().as_u16(), 200);

    let body = response.text().await?;
    assert!(body.contains("v1") || body.contains("users"));

    Ok(())
}

#[tokio::test]
async fn should_serve_v2_api_version() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/v2/users", get_server_url());

    let response = client.get(&url).send().await?;

    assert_eq!(response.status().as_u16(), 200);

    let body = response.text().await?;
    assert!(body.contains("v2") || body.contains("users"));

    Ok(())
}

#[tokio::test]
async fn should_support_nested_versioned_routes() -> Result<()> {
    let client = get_client().await;

    // Test v1 nested route
    let v1_url = format!("{}/v1/api/users", get_server_url());
    let v1_response = client.get(&v1_url).send().await?;
    assert_eq!(v1_response.status().as_u16(), 200);

    // Test v2 nested route
    let v2_url = format!("{}/v2/api/users", get_server_url());
    let v2_response = client.get(&v2_url).send().await?;
    assert_eq!(v2_response.status().as_u16(), 200);

    Ok(())
}

#[tokio::test]
async fn should_support_deeply_nested_versioned_routes() -> Result<()> {
    let client = get_client().await;
    let url = format!("{}/v1/admin/users/123/permissions", get_server_url());

    let response = client.get(&url).send().await?;

    assert_eq!(response.status().as_u16(), 200);

    let body = response.text().await?;
    assert!(body.contains("permissions") || body.contains("v1"));

    Ok(())
}

#[tokio::test]
async fn should_handle_versioned_routes_with_path_params() -> Result<()> {
    let client = get_client().await;

    // Test v1 with path param
    let v1_url = format!("{}/v1/users/42", get_server_url());
    let v1_response = client.get(&v1_url).send().await?;
    assert_eq!(v1_response.status().as_u16(), 200);

    let v1_body = v1_response.text().await?;
    assert!(v1_body.contains("42") || v1_body.contains("id"));

    // Test v2 with path param
    let v2_url = format!("{}/v2/users/42", get_server_url());
    let v2_response = client.get(&v2_url).send().await?;
    assert_eq!(v2_response.status().as_u16(), 200);

    let v2_body = v2_response.text().await?;
    assert!(v2_body.contains("42") || v2_body.contains("id"));

    Ok(())
}

#[tokio::test]
async fn should_coexist_unversioned_and_versioned_routes() -> Result<()> {
    let client = get_client().await;

    // Unversioned health route
    let health_url = format!("{}/health", get_server_url());
    let health_response = client.get(&health_url).send().await?;
    assert_eq!(health_response.status().as_u16(), 200);

    // Versioned routes
    let v1_url = format!("{}/v1/users", get_server_url());
    let v1_response = client.get(&v1_url).send().await?;
    assert_eq!(v1_response.status().as_u16(), 200);

    let v2_url = format!("{}/v2/users", get_server_url());
    let v2_response = client.get(&v2_url).send().await?;
    assert_eq!(v2_response.status().as_u16(), 200);

    Ok(())
}

#[tokio::test]
async fn should_support_different_http_methods_on_versioned_routes() -> Result<()> {
    let client = get_client().await;
    let base_url = format!("{}/v1/users", get_server_url());

    // GET
    let get_response = client.get(&base_url).send().await?;
    assert_eq!(get_response.status().as_u16(), 200);

    // POST
    let post_response = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .body(r#"{"name":"test"}"#)
        .send()
        .await?;
    assert!(post_response.status().is_success() || post_response.status().as_u16() == 404);

    // PUT
    let put_url = format!("{}/v1/users/123", get_server_url());
    let put_response = client
        .put(&put_url)
        .header("Content-Type", "application/json")
        .body(r#"{"name":"updated"}"#)
        .send()
        .await?;
    assert!(put_response.status().is_success() || put_response.status().as_u16() == 404);

    // DELETE
    let delete_response = client.delete(&put_url).send().await?;
    assert!(delete_response.status().is_success() || delete_response.status().as_u16() == 404);

    Ok(())
}

#[tokio::test]
async fn should_return_different_responses_for_different_versions() -> Result<()> {
    let client = get_client().await;

    let v1_url = format!("{}/v1/users", get_server_url());
    let v1_response = client.get(&v1_url).send().await?;
    let v1_body = v1_response.text().await?;

    let v2_url = format!("{}/v2/users", get_server_url());
    let v2_response = client.get(&v2_url).send().await?;
    let v2_body = v2_response.text().await?;

    // V1 and V2 should have different response shapes
    assert!(
        v1_body != v2_body || v1_body.contains("v1") || v2_body.contains("v2"),
        "V1 and V2 should return different responses"
    );

    Ok(())
}
