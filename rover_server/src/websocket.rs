use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Bytes, body::Incoming};
use hyper_tungstenite::{HyperWebsocket, tungstenite::Message};
use serde_json::json;
use tracing::{debug, error, info};

/// Component event message from client
#[derive(serde::Deserialize)]
struct ComponentEvent {
    instance_id: String,
    event_name: String,
    state: serde_json::Value,
}

/// Component event response to client
#[derive(serde::Serialize)]
struct ComponentResponse {
    state: serde_json::Value,
    html: String,
}

/// Handle WebSocket upgrade request
pub async fn handle_websocket_upgrade(
    mut req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, StatusCode> {
    // Check if this is a WebSocket upgrade request
    if !hyper_tungstenite::is_upgrade_request(&req) {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("WebSocket upgrade request received");

    // Upgrade the connection
    let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None)
        .map_err(|e| {
            error!("Failed to upgrade WebSocket: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Spawn a task to handle the WebSocket connection
    tokio::spawn(async move {
        if let Err(e) = handle_websocket(websocket).await {
            error!("WebSocket error: {}", e);
        }
    });

    Ok(response)
}

/// Handle WebSocket connection lifecycle
async fn handle_websocket(websocket: HyperWebsocket) -> Result<()> {
    let mut ws_stream = websocket.await?;

    info!("WebSocket connection established");

    // Message processing loop
    while let Some(msg_result) = ws_stream.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                debug!("Received WebSocket message: {}", text);

                // Parse the component event
                match serde_json::from_str::<ComponentEvent>(&text) {
                    Ok(event) => {
                        // Process the event (this will be integrated with component system)
                        match process_component_event(event).await {
                            Ok(response) => {
                                let response_json = serde_json::to_string(&response)?;
                                if let Err(e) = ws_stream.send(Message::Text(response_json)).await {
                                    error!("Failed to send WebSocket response: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to process component event: {}", e);
                                let error_response = json!({
                                    "error": format!("{}", e)
                                });
                                let _ = ws_stream
                                    .send(Message::Text(serde_json::to_string(&error_response)?))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse component event: {}", e);
                        let error_response = json!({
                            "error": format!("Invalid message format: {}", e)
                        });
                        let _ = ws_stream
                            .send(Message::Text(serde_json::to_string(&error_response)?))
                            .await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket connection closed by client");
                break;
            }
            Ok(Message::Ping(data)) => {
                debug!("Received ping, sending pong");
                if let Err(e) = ws_stream.send(Message::Pong(data)).await {
                    error!("Failed to send pong: {}", e);
                    break;
                }
            }
            Ok(_) => {
                // Ignore other message types (Binary, Pong, Frame)
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    info!("WebSocket connection closed");
    Ok(())
}

/// Process a component event (placeholder - will be integrated with component system)
async fn process_component_event(event: ComponentEvent) -> Result<ComponentResponse> {
    // TODO: This will call into the Lua component system
    // For now, return a placeholder response

    debug!(
        "Processing event '{}' for component '{}'",
        event.event_name, event.instance_id
    );

    // Placeholder: increment state if it's a number
    let new_state = if let Some(num) = event.state.as_i64() {
        json!(num + 1)
    } else {
        event.state
    };

    let html = format!("<h1>Counter {}</h1>", new_state);

    Ok(ComponentResponse {
        state: new_state,
        html,
    })
}
