mod analyzer;
use tree_sitter::Parser;

use crate::analyzer::{Analyzer, SemanticModel};

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

        // Route 3: GET /users/{id}/posts/{postId}
        let route = &server.routes[2];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/users/{id}/posts/{postId}");
        assert!(!route.responses.is_empty());

        // Route 4: GET /greet/{name}
        let route = &server.routes[3];
        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/greet/{name}");
        assert!(!route.responses.is_empty());
    }
}
