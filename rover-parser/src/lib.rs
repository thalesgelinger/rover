mod analyzer;
use tree_sitter::Parser;

use crate::analyzer::Analyzer;
pub use analyzer::{
    BodySchema, GuardSchema, GuardType, HeaderParam, ParsingError, PathParam, QueryParam, Request,
    Response, Route, RoverServer, SemanticModel, ValidationSource,
};

pub fn analyze(code: &str) -> SemanticModel {
    let mut parser = Parser::new();
    let language = tree_sitter_lua::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Lua parser");
    let tree = parser.parse(code, None).unwrap();

    let mut analyzer = Analyzer::new(code.to_string());
    analyzer.walk(tree.root_node());

    if let Some(ref mut server) = analyzer.model.server {
        server.exported = true;
    }

    analyzer.model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_rest_api_basic() {
        let code = include_str!("../../examples/rest_api_basic.lua");
        let model = analyze(code);

        assert!(model.server.is_some(), "Server should be parsed");
        let server = model.server.unwrap();
        assert!(server.exported, "Server should be exported");
        assert_eq!(server.routes.len(), 4, "Should have 4 routes");

        // Route 1: GET /hello
        let route = &server.routes[0];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/hello");
        assert_eq!(route.responses[0].schema["message"], "Hello World");

        // Route 2: GET /hello/{id}
        let route = &server.routes[1];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/hello/{id}");
        assert!(!route.responses.is_empty());
        // Check path params tracking
        assert_eq!(route.request.path_params.len(), 1);
        assert_eq!(route.request.path_params[0].name, "id");
        assert!(
            route.request.path_params[0].used,
            "id param should be marked as used"
        );

        // Route 3: GET /users/{id}/posts/{postId}
        let route = &server.routes[2];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/users/{id}/posts/{postId}");
        assert!(!route.responses.is_empty());
        // Check multiple path params
        assert_eq!(route.request.path_params.len(), 2);
        assert_eq!(route.request.path_params[0].name, "id");
        assert_eq!(route.request.path_params[1].name, "postId");
        assert!(
            route.request.path_params[0].used,
            "id param should be marked as used"
        );
        assert!(
            route.request.path_params[1].used,
            "postId param should be marked as used"
        );

        // Route 4: GET /greet/{name}
        let route = &server.routes[3];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/greet/{name}");
        assert!(!route.responses.is_empty());
        assert_eq!(route.request.path_params.len(), 1);
        assert_eq!(route.request.path_params[0].name, "name");
        assert!(
            route.request.path_params[0].used,
            "name param should be marked as used"
        );
    }

    #[test]
    fn should_parse_rest_api_auth() {
        let code = include_str!("../../examples/rest_api_auth.lua");
        let model = analyze(code);

        assert!(model.server.is_some(), "Server should be parsed");
        let server = model.server.unwrap();
        assert!(server.exported, "Server should be exported");
        assert_eq!(server.routes.len(), 5, "Should have 5 routes");

        // Check the hello route with headers
        let hello_route = server.routes.iter().find(|r| r.path == "/hello").unwrap();
        assert_eq!(hello_route.method, "GET");
        assert_eq!(hello_route.request.headers.len(), 1);
        assert_eq!(hello_route.request.headers[0].name, "Authorization");
        assert_eq!(
            hello_route.request.headers[0].schema.guard_type,
            crate::analyzer::GuardType::String
        );
        assert!(!hello_route.request.headers[0].schema.required);

        // Check status code parsing
        assert_eq!(
            hello_route.responses.len(),
            2,
            "Should have 2 responses (200 and 401)"
        );
        let success_response = hello_route
            .responses
            .iter()
            .find(|r| r.status == 200)
            .unwrap();
        let error_response = hello_route
            .responses
            .iter()
            .find(|r| r.status == 401)
            .unwrap();
        assert_eq!(success_response.content_type, "application/json");
        assert_eq!(error_response.content_type, "application/json");
    }

    #[test]
    fn should_parse_context_requests() {
        let code = include_str!("../../examples/context_requests.lua");
        let model = analyze(code);

        assert!(model.server.is_some(), "Server should be parsed");
        let server = model.server.unwrap();
        assert_eq!(server.routes.len(), 2, "Should have 2 routes");

        // Check GET /echo route
        let echo_get = server.routes.iter().find(|r| r.method == "GET").unwrap();
        println!(
            "GET route query params: {}",
            echo_get.request.query_params.len()
        );
        println!("GET route headers: {}", echo_get.request.headers.len());
        for qp in &echo_get.request.query_params {
            println!("  Query param: {}", qp.name);
        }
        for header in &echo_get.request.headers {
            println!("  Header: {}", header.name);
        }
        assert_eq!(echo_get.request.query_params.len(), 2);
        assert_eq!(echo_get.request.query_params[0].name, "page");
        assert_eq!(echo_get.request.query_params[1].name, "limit");
        assert_eq!(echo_get.request.headers.len(), 1);
        assert_eq!(echo_get.request.headers[0].name, "user-agent");

        // Check POST /echo route
        let echo_post = server.routes.iter().find(|r| r.method == "POST").unwrap();
        assert_eq!(echo_post.request.headers.len(), 1);
        assert_eq!(echo_post.request.headers[0].name, "content-type");
    }

    #[test]
    fn should_parse_validation_guard() {
        let code = include_str!("../../examples/validation_guard.lua");
        let model = analyze(code);

        assert!(model.server.is_some(), "Server should be parsed");
        let server = model.server.unwrap();
        assert_eq!(server.routes.len(), 9, "Should have 9 routes");

        // Check basic route with body validation
        let basic_route = server.routes.iter().find(|r| r.path == "/basic").unwrap();
        assert!(
            basic_route.request.body_schema.is_some(),
            "Should have body schema"
        );
        let body_schema = basic_route.request.body_schema.as_ref().unwrap();
        assert_eq!(body_schema.schema["type"], "object");
        assert_eq!(body_schema.schema["properties"]["name"]["type"], "string");
        assert_eq!(body_schema.schema["properties"]["email"]["type"], "string");
        assert!(
            body_schema.schema["required"]
                .as_array()
                .unwrap()
                .contains(&"name".into())
        );
        assert!(
            body_schema.schema["required"]
                .as_array()
                .unwrap()
                .contains(&"email".into())
        );
    }

    #[test]
    fn should_parse_validation_reference() {
        let code = include_str!("../../examples/validation_reference.lua");
        let model = analyze(code);

        assert!(model.server.is_some(), "Server should be parsed");
        let server = model.server.unwrap();

        // Check arrays route
        let arrays_route = server.routes.iter().find(|r| r.path == "/arrays").unwrap();
        assert!(arrays_route.request.body_schema.is_some());
        let body_schema = arrays_route.request.body_schema.as_ref().unwrap();
        assert_eq!(body_schema.schema["properties"]["tags"]["type"], "array");
        assert_eq!(
            body_schema.schema["properties"]["tags"]["items"]["type"],
            "string"
        );

        // Check nested objects route
        let nested_route = server
            .routes
            .iter()
            .find(|r| r.path == "/nested_objects")
            .unwrap();
        assert!(nested_route.request.body_schema.is_some());
        let body_schema = nested_route.request.body_schema.as_ref().unwrap();
        assert_eq!(body_schema.schema["properties"]["user"]["type"], "object");
        assert_eq!(
            body_schema.schema["properties"]["user"]["properties"]["name"]["type"],
            "string"
        );
        assert_eq!(
            body_schema.schema["properties"]["user"]["properties"]["profile"]["type"],
            "object"
        );
    }

    #[test]
    fn should_warn_about_nonexistent_params() {
        let code = r#"
local api = rover.server {}

function api.hello.p_id.get(ctx)
    return api.json {
        message = "Hello " .. ctx:params().nonexistent
    }
end

return api
        "#;

        let model = analyze(code);
        assert!(model.server.is_some());
        let _server = model.server.unwrap();

        // Should have an error about accessing non-existent param
        assert!(
            !model.errors.is_empty(),
            "Should have errors about non-existent params"
        );
        let param_error = model
            .errors
            .iter()
            .find(|e| e.message.contains("nonexistent"))
            .unwrap();
        assert!(param_error.message.contains("nonexistent"));
    }
}
