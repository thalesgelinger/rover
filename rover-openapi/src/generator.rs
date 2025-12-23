use rover_parser::SemanticModel;
use serde_json::{json, Value};

pub fn generate_spec(model: &SemanticModel, title: &str, version: &str) -> Value {
    let mut paths = serde_json::Map::new();

    if let Some(server) = &model.server {
        for route in &server.routes {
            let path_key = &route.path;
            let method = route.method.to_lowercase();

            // Get or create path entry
            let path_entry = paths
                .entry(path_key.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));

            if let Value::Object(path_obj) = path_entry {
                // Build operation
                let mut operation = serde_json::Map::new();

                // Add summary
                operation.insert(
                    "summary".to_string(),
                    Value::String(format!("{} {}", route.method, route.path)),
                );

                // Add parameters for path variables (extract from path)
                let params = extract_path_params(&route.path);
                if !params.is_empty() {
                    let param_array: Vec<Value> = params
                        .iter()
                        .map(|name| {
                            json!({
                                "name": name,
                                "in": "path",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            })
                        })
                        .collect();
                    operation.insert("parameters".to_string(), Value::Array(param_array));
                }

                // Add responses
                let mut responses = serde_json::Map::new();
                if !route.responses.is_empty() {
                    for response in &route.responses {
                        let status_str = response.status.to_string();
                        let mut content_map = serde_json::Map::new();
                        content_map.insert(
                            response.content_type.clone(),
                            json!({
                                "schema": response.schema
                            }),
                        );
                        responses.insert(
                            status_str,
                            json!({
                                "description": format!("Response with status {}", response.status),
                                "content": content_map
                            }),
                        );
                    }
                } else {
                    // Default empty response if none defined
                    responses.insert(
                        "200".to_string(),
                        json!({
                            "description": "Successful response"
                        }),
                    );
                }
                operation.insert("responses".to_string(), Value::Object(responses));

                // Insert operation under method
                path_obj.insert(method, Value::Object(operation));
            }
        }
    }

    json!({
        "openapi": "3.0.0",
        "info": {
            "title": title,
            "version": version
        },
        "paths": paths
    })
}

fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut in_param = false;
    let mut current_param = String::new();

    for ch in path.chars() {
        match ch {
            '{' => {
                in_param = true;
                current_param.clear();
            }
            '}' => {
                if in_param && !current_param.is_empty() {
                    params.push(current_param.clone());
                }
                in_param = false;
            }
            _ if in_param => {
                current_param.push(ch);
            }
            _ => {}
        }
    }

    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_params() {
        assert_eq!(extract_path_params("/hello"), vec![] as Vec<String>);
        assert_eq!(extract_path_params("/hello/{id}"), vec!["id"]);
        assert_eq!(
            extract_path_params("/users/{id}/posts/{postId}"),
            vec!["id", "postId"]
        );
    }
}
