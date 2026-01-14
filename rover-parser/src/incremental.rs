use std::collections::HashMap;
use std::time::Instant;
use tree_sitter::{Parser, Tree};

#[derive(Debug, Clone)]
pub struct CachedParse {
    pub tree: Tree,
    pub hash: u64,
    pub timestamp: Instant,
}

pub struct IncrementalParser {
    cache: HashMap<String, CachedParse>,
    parser: Parser,
}

impl IncrementalParser {
    pub fn new() -> Self {
        let language = tree_sitter_lua::LANGUAGE;
        let mut parser = Parser::new();
        parser
            .set_language(&language.into())
            .expect("Error loading Lua parser");

        Self {
            cache: HashMap::new(),
            parser,
        }
    }

    pub fn parse_incremental(&mut self, uri: &str, source: &str) -> Option<Tree> {
        let hash = compute_hash(source);

        if let Some(cached) = self.cache.get(uri) {
            if cached.hash == hash {
                return Some(cached.tree.clone());
            }

            let old_tree = &cached.tree;
            if let Some(new_tree) = self.parser.parse(source, Some(old_tree)) {
                self.cache.insert(
                    uri.to_string(),
                    CachedParse {
                        tree: new_tree.clone(),
                        hash,
                        timestamp: Instant::now(),
                    },
                );
                return Some(new_tree);
            }
        }

        if let Some(tree) = self.parser.parse(source, None) {
            self.cache.insert(
                uri.to_string(),
                CachedParse {
                    tree: tree.clone(),
                    hash,
                    timestamp: Instant::now(),
                },
            );
            Some(tree)
        } else {
            None
        }
    }

    pub fn invalidate(&mut self, uri: &str) {
        self.cache.remove(uri);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for IncrementalParser {
    fn default() -> Self {
        Self::new()
    }
}

fn compute_hash(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash() {
        let source = "print('hello')";
        let hash1 = compute_hash(source);
        let hash2 = compute_hash(source);
        assert_eq!(hash1, hash2);

        let different = "print('world')";
        let hash3 = compute_hash(different);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_incremental_parser_cache() {
        let mut parser = IncrementalParser::new();
        let uri = "test.lua";
        let source = "local x = 42";

        let tree1 = parser.parse_incremental(uri, source);
        assert!(tree1.is_some());

        let tree2 = parser.parse_incremental(uri, source);
        assert!(tree2.is_some());

        assert_eq!(
            tree1.unwrap().root_node().kind(),
            tree2.unwrap().root_node().kind()
        );
    }

    #[test]
    fn test_incremental_parser_invalidate() {
        let mut parser = IncrementalParser::new();
        let uri = "test.lua";
        let source = "local x = 42";

        parser.parse_incremental(uri, source);
        assert!(parser.cache.contains_key(uri));

        parser.invalidate(uri);
        assert!(!parser.cache.contains_key(uri));
    }

    #[test]
    fn test_incremental_parser_clear() {
        let mut parser = IncrementalParser::new();
        parser.parse_incremental("test1.lua", "local x = 1");
        parser.parse_incremental("test2.lua", "local y = 2");

        assert_eq!(parser.cache.len(), 2);

        parser.clear();
        assert_eq!(parser.cache.len(), 0);
    }
}
