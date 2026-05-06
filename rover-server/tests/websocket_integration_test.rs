use std::time::Duration;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

fn get_server_url() -> String {
    std::env::var("ROVER_TEST_SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:4242".to_string())
}

async fn send_and_receive(url: &str, messages: Vec<Value>) -> Result<Vec<Value>> {
    let (mut ws_stream, _) = connect_async(url).await?;

    let mut responses = Vec::new();

    for msg in messages {
        let text = serde_json::to_string(&msg)?;
        ws_stream.send(Message::Text(text)).await?;

        if let Some(Ok(Message::Text(response))) = ws_stream.next().await
            && let Ok(json) = serde_json::from_str::<Value>(&response)
        {
            responses.push(json);
        }
    }

    ws_stream.close(None).await?;
    Ok(responses)
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_echo_server() -> Result<()> {
    let url = format!("{}/echo", get_server_url());

    let messages = vec![
        json!({"type": "echo", "text": "hello"}),
        json!({"type": "echo", "text": "world"}),
    ];

    let responses = send_and_receive(&url, messages).await?;

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["type"], "echo");
    assert_eq!(responses[0]["text"], "hello");
    assert_eq!(responses[1]["type"], "echo");
    assert_eq!(responses[1]["text"], "world");

    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_join_hook() -> Result<()> {
    let url = format!("{}/echo", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    if let Some(Ok(Message::Text(welcome))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&welcome)?;
        assert_eq!(json["type"], "welcome");
        assert!(json["message"].is_string());
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_state_management() -> Result<()> {
    let url = format!("{}/chat?user_id=test_user", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "alice"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "user_joined");
        assert_eq!(json["user_id"], "alice");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_broadcast() -> Result<()> {
    let url = format!("{}/chat?user_id=broadcaster", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "hello everyone"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
        assert_eq!(json["text"], "hello everyone");
        assert!(json["timestamp"].is_number());
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_validation() -> Result<()> {
    let url = format!("{}/chat?user_id=validator", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": ""}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "error");
        assert!(json["message"].is_string());
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_path_parameters() -> Result<()> {
    let url = format!("{}/chat/general?user_id=room_user", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "room message"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
        assert_eq!(json["text"], "room message");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_topic_subscription() -> Result<()> {
    let url = format!("{}/chat/room1?user_id=subscriber", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "room1 message"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
        assert_eq!(json["room_id"], "room1");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_fallback_handler() -> Result<()> {
    let url = format!("{}/echo", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"custom": "data"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "echo");
        assert_eq!(json["custom"], "data");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_leave_hook() -> Result<()> {
    let url = format!("{}/chat?user_id=leaver", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "leaver"}),
        )?))
        .await?;

    ws_stream.next().await;

    ws_stream.close(None).await?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_multiple_clients() -> Result<()> {
    let url1 = format!("{}/chat?user_id=client1", get_server_url());
    let url2 = format!("{}/chat?user_id=client2", get_server_url());

    let (mut ws1, _) = connect_async(&url1).await?;
    let (mut ws2, _) = connect_async(&url2).await?;

    ws1.send(Message::Text(serde_json::to_string(
        &json!({"type": "chat", "text": "from client1"}),
    )?))
    .await?;

    if let Some(Ok(Message::Text(response))) = ws1.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
    }

    ws1.close(None).await?;
    ws2.close(None).await?;

    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_connect_flow_ctx_methods() -> Result<()> {
    let url = format!(
        "{}/chat/general?user_id=ctx_test&name=TestUser",
        get_server_url()
    );

    let (mut ws_stream, _) = connect_async(&url).await?;

    if let Some(Ok(Message::Text(welcome))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&welcome)?;
        assert_eq!(json["type"], "user_joined");
        assert_eq!(json["room_id"], "general");
        assert_eq!(json["user_id"], "ctx_test");
        assert_eq!(json["name"], "TestUser");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_message_flow_state_propagation() -> Result<()> {
    let url = format!("{}/chat?user_id=state_test", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream.next().await;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "alice"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "user_joined");
        assert_eq!(json["user_id"], "alice");
    }

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "hello from alice"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
        assert_eq!(json["user_id"], "alice");
        assert_eq!(json["text"], "hello from alice");
    }

    ws_stream.close(None).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_close_flow_state_cleanup() -> Result<()> {
    let url1 = format!("{}/chat?user_id=closer1", get_server_url());
    let url2 = format!("{}/chat?user_id=closer2", get_server_url());

    let (mut ws1, _) = connect_async(&url1).await?;
    let (mut ws2, _) = connect_async(&url2).await?;

    ws1.next().await;
    ws2.next().await;

    ws1.send(Message::Text(serde_json::to_string(
        &json!({"type": "identify", "user_id": "closer1"}),
    )?))
    .await?;
    ws1.next().await;

    ws2.send(Message::Text(serde_json::to_string(
        &json!({"type": "identify", "user_id": "closer2"}),
    )?))
    .await?;
    ws2.next().await;

    ws1.close(None).await?;

    tokio::time::sleep(Duration::from_millis(150)).await;

    ws2.close(None).await?;

    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_complete_lifecycle() -> Result<()> {
    let url = format!("{}/chat?user_id=lifecycle_test", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    if let Some(Ok(Message::Text(welcome))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&welcome)?;
        assert_eq!(json["type"], "welcome");
    }

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "lifecycle_user"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "user_joined");
        assert_eq!(json["user_id"], "lifecycle_user");
    }

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "test message"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["type"], "chat");
        assert_eq!(json["user_id"], "lifecycle_user");
        assert_eq!(json["text"], "test message");
    }

    ws_stream.close(None).await?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}

#[tokio::test]
#[ignore = "requires ROVER_TEST_SERVER_URL"]
async fn test_websocket_state_update_chain() -> Result<()> {
    let url = format!("{}/chat?user_id=chain_test", get_server_url());

    let (mut ws_stream, _) = connect_async(&url).await?;

    ws_stream.next().await;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "user_v1"}),
        )?))
        .await?;
    ws_stream.next().await;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "msg1"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["user_id"], "user_v1");
    }

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "identify", "user_id": "user_v2"}),
        )?))
        .await?;
    ws_stream.next().await;

    ws_stream
        .send(Message::Text(serde_json::to_string(
            &json!({"type": "chat", "text": "msg2"}),
        )?))
        .await?;

    if let Some(Ok(Message::Text(response))) = ws_stream.next().await {
        let json: Value = serde_json::from_str(&response)?;
        assert_eq!(json["user_id"], "user_v2");
    }

    ws_stream.close(None).await?;
    Ok(())
}
