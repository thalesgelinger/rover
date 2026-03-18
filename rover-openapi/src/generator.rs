use rover_parser::{GuardSchema, GuardType, SemanticModel};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub fn generate_spec(model: &SemanticModel, title: &str, version: &str) -> Value {
    let mut paths = Map::new();

    if let Some(server) = &model.server {
        for route in &server.routes {
            let path_entry = paths
                .entry(route.path.clone())
                .or_insert_with(|| Value::Object(Map::new()));

            if let Value::Object(path_obj) = path_entry {
                let mut operation = Map::new();

                operation.insert(
                    "summary".into(),
                    Value::String(format!("{} {}", route.method, route.path)),
                );

                let mut parameters = Vec::new();
                add_path_parameters(&mut parameters, route);
                add_query_parameters(&mut parameters, route);
                add_header_parameters(&mut parameters, route);

                if !parameters.is_empty() {
                    operation.insert("parameters".into(), Value::Array(parameters));
                }

                // Add requestBody for POST/PUT/PATCH routes
                let has_body =
                    route.method == "POST" || route.method == "PUT" || route.method == "PATCH";
                if has_body && route.request.body_used {
                    if let Some(body_schema) = &route.request.body_schema {
                        operation.insert(
                            "requestBody".into(),
                            json!({
                                "required": true,
                                "content": {
                                    "application/json": {
                                        "schema": body_schema.schema.clone()
                                    }
                                }
                            }),
                        );
                    } else {
                        // Add placeholder schema for routes without :expect
                        operation.insert(
                            "requestBody".into(),
                            json!({
                                "required": true,
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object"
                                        }
                                    }
                                }
                            }),
                        );
                    }
                }

                operation.insert("responses".into(), Value::Object(build_responses(route)));

                path_obj.insert(route.method.to_lowercase(), Value::Object(operation));
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

fn add_path_parameters(parameters: &mut Vec<Value>, route: &rover_parser::Route) {
    let mut seen = HashSet::new();
    let names: Vec<String> = if route.request.path_params.is_empty() {
        extract_path_params(&route.path)
    } else {
        route
            .request
            .path_params
            .iter()
            .map(|param| param.name.clone())
            .collect()
    };

    for name in names {
        if seen.insert(name.clone()) {
            push_parameter(
                parameters,
                name,
                "path",
                true,
                json!({
                    "type": "string"
                }),
            );
        }
    }
}

fn add_query_parameters(parameters: &mut Vec<Value>, route: &rover_parser::Route) {
    for param in &route.request.query_params {
        let schema = guard_schema_to_openapi_schema(&param.schema);
        push_parameter(
            parameters,
            param.name.clone(),
            "query",
            param.schema.required,
            schema,
        );
    }
}

fn add_header_parameters(parameters: &mut Vec<Value>, route: &rover_parser::Route) {
    for header in &route.request.headers {
        let schema = guard_schema_to_openapi_schema(&header.schema);
        push_parameter(
            parameters,
            header.name.clone(),
            "header",
            header.schema.required,
            schema,
        );
    }
}

fn push_parameter(
    parameters: &mut Vec<Value>,
    name: String,
    location: &str,
    required: bool,
    schema: Value,
) {
    parameters.push(json!({
        "name": name,
        "in": location,
        "required": required,
        "schema": schema
    }));
}

fn guard_schema_to_openapi_schema(schema: &GuardSchema) -> Value {
    use GuardType::*;

    let mut base = match &schema.guard_type {
        String => json!({ "type": "string" }),
        Integer => json!({ "type": "integer" }),
        Number => json!({ "type": "number" }),
        Boolean => json!({ "type": "boolean" }),
        Array(inner) => json!({
            "type": "array",
            "items": guard_schema_to_openapi_schema(inner)
        }),
        Object(properties) => {
            let mut props = Map::new();
            let mut required = Vec::new();
            for (name, prop_schema) in properties {
                if prop_schema.required {
                    required.push(Value::String(name.clone()));
                }
                props.insert(name.clone(), guard_schema_to_openapi_schema(prop_schema));
            }

            let mut obj = Map::new();
            obj.insert("type".into(), Value::String("object".into()));
            obj.insert("properties".into(), Value::Object(props));
            if !required.is_empty() {
                obj.insert("required".into(), Value::Array(required));
            }
            Value::Object(obj)
        }
    };

    if let Value::Object(map) = &mut base {
        if let Some(default) = &schema.default {
            map.insert("default".into(), default.clone());
        }
        if let Some(enum_values) = &schema.enum_values {
            map.insert(
                "enum".into(),
                Value::Array(
                    enum_values
                        .iter()
                        .map(|v| Value::String(v.clone()))
                        .collect(),
                ),
            );
        }
    }

    base
}

fn build_responses(route: &rover_parser::Route) -> Map<String, Value> {
    let mut responses = Map::new();

    if route.responses.is_empty() {
        responses.insert(
            "200".into(),
            json!({
                "description": "Successful response"
            }),
        );
        return responses;
    }

    for response in &route.responses {
        let mut content_map = Map::new();
        let schema = value_to_openapi_schema(&response.schema);
        content_map.insert(
            response.content_type.clone(),
            json!({
                "schema": schema,
                "example": response.schema.clone()
            }),
        );

        responses.insert(
            response.status.to_string(),
            json!({
                "description": format!("Response with status {}", response.status),
                "content": content_map
            }),
        );
    }

    responses
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

fn value_to_openapi_schema(value: &Value) -> Value {
    match value {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Array(arr) => {
            if arr.is_empty() {
                json!({
                    "type": "array",
                    "items": {}
                })
            } else {
                json!({
                    "type": "array",
                    "items": value_to_openapi_schema(&arr[0])
                })
            }
        }
        Value::Object(obj) => {
            let mut properties = Map::new();
            let mut required = Vec::new();

            for (key, val) in obj {
                properties.insert(key.clone(), value_to_openapi_schema(val));
                required.push(Value::String(key.clone()));
            }

            let mut schema = Map::new();
            schema.insert("type".into(), Value::String("object".into()));
            schema.insert("properties".into(), Value::Object(properties));
            if !required.is_empty() {
                schema.insert("required".into(), Value::Array(required));
            }
            Value::Object(schema)
        }
    }
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

    #[test]
    fn spec_includes_parameters_and_body() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.users.p_id.get(ctx)
    local page = ctx:query().page
    local agent = ctx:headers()["user-agent"]
    return api.json { ok = true, page = page, agent = agent }
end

function api.users.post(ctx)
    return api.json(ctx:body():expect {
        name = g:string():required(),
        role = g:string():enum({"admin", "user"})
    })
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let get_params = spec["paths"]["/users/{id}"]["get"]["parameters"]
            .as_array()
            .unwrap();
        assert!(get_params
            .iter()
            .any(|p| p["name"] == "id" && p["in"] == "path"));
        assert!(get_params
            .iter()
            .any(|p| p["name"] == "page" && p["in"] == "query"));
        assert!(get_params
            .iter()
            .any(|p| p["name"] == "user-agent" && p["in"] == "header"));

        let request_body = &spec["paths"]["/users"]["post"]["requestBody"];
        assert!(request_body["required"].as_bool().unwrap());
        assert_eq!(
            request_body["content"]["application/json"]["schema"]["type"],
            "object"
        );
        let role_enum =
            &request_body["content"]["application/json"]["schema"]["properties"]["role"]["enum"];
        assert_eq!(role_enum.as_array().unwrap().len(), 2);
    }

    #[test]
    fn spec_includes_response_schemas() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.users.p_id.get(ctx)
    return api.json { id = 1, name = "test", active = true }
end

function api.users.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required(),
        email = g:string():required(),
    }
    return api.json:status(201, { id = 1, name = user.name, email = user.email })
end

function api.users.p_id.delete(ctx)
    return api.json:status(204, {})
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let get_response = &spec["paths"]["/users/{id}"]["get"]["responses"]["200"];
        assert_eq!(
            get_response["content"]["application/json"]["schema"]["type"],
            "object"
        );
        assert!(
            get_response["content"]["application/json"]["schema"]["properties"]["id"]["type"]
                == "integer"
        );
        assert!(
            get_response["content"]["application/json"]["schema"]["properties"]["name"]["type"]
                == "string"
        );
        assert!(
            get_response["content"]["application/json"]["schema"]["properties"]["active"]["type"]
                == "boolean"
        );

        let post_response = &spec["paths"]["/users"]["post"]["responses"]["201"];
        assert_eq!(
            post_response["content"]["application/json"]["schema"]["type"],
            "object"
        );

        let delete_response = &spec["paths"]["/users/{id}"]["delete"]["responses"]["204"];
        assert_eq!(
            delete_response["content"]["application/json"]["schema"]["type"],
            "object"
        );
    }

    #[test]
    fn spec_includes_multiple_response_codes() {
        let code = r#"
local api = rover.server {}

function api.hello.get(ctx)
    local token = ctx:headers().Authorization
    
    if not token then
        return api.json:status(401, { message = "Unauthorized" })
    end
    
    return api.json:status(200, { message = "Hello World" })
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let responses = &spec["paths"]["/hello"]["get"]["responses"];
        assert!(responses["200"].is_object(), "Should have 200 response");
        assert!(responses["401"].is_object(), "Should have 401 response");

        let ok_response = &responses["200"];
        assert_eq!(
            ok_response["content"]["application/json"]["schema"]["type"],
            "object"
        );

        let error_response = &responses["401"];
        assert_eq!(
            error_response["content"]["application/json"]["schema"]["type"],
            "object"
        );
    }

    #[test]
    fn spec_includes_nested_object_body_schema() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.users.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required(),
        profile = g:object {
            bio = g:string(),
            age = g:integer(),
        },
    }
    return api.json(user)
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let body_schema = &spec["paths"]["/users"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"];
        assert_eq!(body_schema["type"], "object");
        assert_eq!(body_schema["properties"]["profile"]["type"], "object");
        assert_eq!(
            body_schema["properties"]["profile"]["properties"]["bio"]["type"],
            "string"
        );
        assert_eq!(
            body_schema["properties"]["profile"]["properties"]["age"]["type"],
            "integer"
        );
    }

    #[test]
    fn spec_includes_array_body_schema() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.tags.post(ctx)
    local data = ctx:body():expect {
        tags = g:array(g:string()),
        scores = g:array(g:integer()),
    }
    return api.json(data)
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let body_schema =
            &spec["paths"]["/tags"]["post"]["requestBody"]["content"]["application/json"]["schema"];
        assert_eq!(body_schema["properties"]["tags"]["type"], "array");
        assert_eq!(body_schema["properties"]["tags"]["items"]["type"], "string");
        assert_eq!(body_schema["properties"]["scores"]["type"], "array");
        assert_eq!(
            body_schema["properties"]["scores"]["items"]["type"],
            "integer"
        );
    }

    #[test]
    fn spec_includes_default_values() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.settings.post(ctx)
    local data = ctx:body():expect {
        theme = g:string():default("light"),
        count = g:integer():default(10),
    }
    return api.json(data)
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let body_schema = &spec["paths"]["/settings"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"];
        assert_eq!(body_schema["properties"]["theme"]["default"], "light");
        assert_eq!(body_schema["properties"]["count"]["default"], 10);
    }

    #[test]
    fn spec_includes_required_fields() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.users.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required(),
        email = g:string():required(),
        age = g:integer(),
    }
    return api.json(user)
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let body_schema = &spec["paths"]["/users"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"];
        let required = body_schema["required"].as_array().unwrap();
        assert!(required.contains(&"name".into()), "name should be required");
        assert!(
            required.contains(&"email".into()),
            "email should be required"
        );
        assert!(
            !required.contains(&"age".into()),
            "age should not be required"
        );
    }

    #[test]
    fn spec_includes_enum_values() {
        let code = r#"
local api = rover.server {}
local g = rover.guard

function api.status.post(ctx)
    local data = ctx:body():expect {
        status = g:string():enum({"active", "inactive", "pending"}):required(),
    }
    return api.json(data)
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let body_schema = &spec["paths"]["/status"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"];
        let enum_values = body_schema["properties"]["status"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(enum_values.len(), 3);
        assert!(enum_values.contains(&"active".into()));
        assert!(enum_values.contains(&"inactive".into()));
        assert!(enum_values.contains(&"pending".into()));
    }

    #[test]
    fn spec_includes_response_examples() {
        let code = r#"
local api = rover.server {}

function api.hello.get(ctx)
    return api.json { message = "Hello World", count = 42 }
end

return api
"#;

        let model = rover_parser::analyze(code);
        let spec = generate_spec(&model, "Test", "1.0.0");

        let response = &spec["paths"]["/hello"]["get"]["responses"]["200"];
        let example = &response["content"]["application/json"]["example"];
        assert_eq!(example["message"], "Hello World");
        assert_eq!(example["count"], 42);
    }
}
