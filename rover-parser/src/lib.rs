use std::{any::Any, collections::HashMap};

use serde_json::Value;
use tree_sitter::{Node, Parser};

#[derive(Debug)]
pub struct SemanticModel {
    server: Option<RoverServer>,
}

#[derive(Debug)]
struct RoverServer {
    id: u8,
    exported: bool,
    routes: Vec<Route>,
}

type FunctionId = u16;

#[derive(Debug)]
struct Route {
    method: String,
    path: String,
    handler: FunctionId,
    request: Request,
    responses: Vec<Response>,
}

#[derive(Debug)]
struct Request {}

#[derive(Debug)]
struct Response {
    status: u16,
    content_type: String,
    schema: Value,
}

struct Analyzer {
    model: SemanticModel,
}

impl Analyzer {
    pub fn new() -> Self {
        Analyzer {
            model: SemanticModel { server: None },
        }
    }

    pub fn walk(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child)
        }
    }
}

pub fn analyze(code: &str) -> SemanticModel {
    let mut parser = Parser::new();
    let language = tree_sitter_lua::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Lua parser");
    let tree = parser.parse(code, None).unwrap();

    let mut analyzer = Analyzer::new();
    analyzer.walk(tree.root_node());
    analyzer.model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_routes() {
        let code = r#"
            return 42
        "#;
        let model = analyze(code);
        assert_eq!(model.server, None);
    }
}
