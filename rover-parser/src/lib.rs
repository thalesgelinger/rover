use tree_sitter::{Node, Parser};

pub struct SemanticModel {
    node_count: u8,
}

struct Analyzer {
    model: SemanticModel,
}

impl Analyzer {
    pub fn new() -> Self {
        Analyzer {
            model: SemanticModel { node_count: 0 },
        }
    }

    pub fn walk(&mut self, node: Node) {
        println!("{:?}", node);
        self.model.node_count += 1;

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
    fn it_works() {
        let code = r#"
            return 42
        "#;
        let model = analyze(code);
        assert_eq!(model.node_count, 5);
    }
}
