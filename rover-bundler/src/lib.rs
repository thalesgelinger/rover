use anyhow::{Context, Result};
use rover_parser::analyze;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Bundle result containing all Lua files and metadata
#[derive(Debug, Clone)]
pub struct Bundle {
    pub entrypoint: PathBuf,
    pub files: HashMap<String, String>, // relative path -> content
    pub features: AppFeatures,
}

/// Detected app features
#[derive(Debug, Clone, Default)]
pub struct AppFeatures {
    pub server: bool,
    pub ui: bool,
    pub db: bool,
}

/// Bundle options
#[derive(Debug, Clone)]
pub struct BundleOptions {
    pub entrypoint: PathBuf,
    pub base_path: PathBuf,
}

/// Create a bundle from entrypoint
pub fn bundle(options: BundleOptions) -> Result<Bundle> {
    let mut bundle = Bundle {
        entrypoint: options.entrypoint.clone(),
        files: HashMap::new(),
        features: AppFeatures::default(),
    };

    let mut visited = HashSet::new();
    let mut queue = vec![options.entrypoint.clone()];

    while let Some(path) = queue.pop() {
        if visited.contains(&path) {
            continue;
        }
        visited.insert(path.clone());

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        // Detect features
        detect_features(&content, &mut bundle.features);

        // Add to bundle
        let relative_path = make_relative(&path, &options.base_path)?;
        bundle.files.insert(relative_path, content.clone());

        // Find requires
        let requires = find_requires(&content)?;
        for req in requires {
            if let Some(module_path) = resolve_module(&req, &options.base_path) {
                if !visited.contains(&module_path) {
                    queue.push(module_path);
                }
            }
        }
    }

    Ok(bundle)
}

/// Detect app features from code
fn detect_features(code: &str, features: &mut AppFeatures) {
    // Check for server usage
    if code.contains("rover.server") || code.contains("rover.server(") {
        features.server = true;
    }

    // Check for UI usage (rover.render)
    if code.contains("rover.render") || code.contains("rover.render(") {
        features.ui = true;
    }

    // Check for DB usage
    if code.contains("rover.db") || code.contains("rover.db.") {
        features.db = true;
    }
}

/// Find all require() calls in code
fn find_requires(code: &str) -> Result<Vec<String>> {
    let mut requires = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_lua::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Lua parser");

    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    find_requires_recursive(&root, code, &mut requires);

    Ok(requires)
}

fn find_requires_recursive(node: &tree_sitter::Node, code: &str, requires: &mut Vec<String>) {
    if node.kind() == "function_call" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name = &code[child.start_byte()..child.end_byte()];
                if name == "require" {
                    // Find the argument
                    let mut arg_cursor = node.walk();
                    for arg in node.children(&mut arg_cursor) {
                        if arg.kind() == "arguments" {
                            let mut str_cursor = arg.walk();
                            for str_node in arg.children(&mut str_cursor) {
                                if str_node.kind() == "string" {
                                    let module_name =
                                        &code[str_node.start_byte()..str_node.end_byte()];
                                    // Remove quotes
                                    let module_name = module_name
                                        .trim_start_matches('"')
                                        .trim_start_matches("'")
                                        .trim_end_matches('"')
                                        .trim_end_matches("'");
                                    requires.push(module_name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_requires_recursive(&child, code, requires);
    }
}

/// Resolve module name to path
fn resolve_module(module_name: &str, base_path: &Path) -> Option<PathBuf> {
    let path_str = module_name.replace('.', "/");
    let extensions = [".lua", "/init.lua"];

    for ext in &extensions {
        let path = base_path.join(format!("{}{}", path_str, ext));
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Make path relative to base
fn make_relative(path: &Path, base: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(base)
        .with_context(|| format!("Path {} not under base {}", path.display(), base.display()))?;
    Ok(relative.to_string_lossy().to_string())
}

/// Serialize bundle to a single Lua file
pub fn serialize_bundle(bundle: &Bundle) -> String {
    let mut output = String::new();

    // Add bundle loader
    output.push_str(
        r#"-- Rover Bundled Application
local __rover_modules = {}
local __rover_cache = {}

local function __rover_require(name)
    if __rover_cache[name] then
        return __rover_cache[name]
    end
    local mod = __rover_modules[name]
    if not mod then
        error("Module not found: " .. name)
    end
    local result = mod()
    __rover_cache[name] = result
    return result
end

"#,
    );

    // Add each module
    for (path, content) in &bundle.files {
        output.push_str(&format!("__rover_modules['{}'] = function()\n", path));
        output.push_str(content);
        output.push_str("\nend\n\n");
    }

    // Add entrypoint execution
    let entrypoint_relative = bundle
        .entrypoint
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    output.push_str(&format!(
        "return __rover_require('{}')\n",
        entrypoint_relative
    ));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_features_server() {
        let code = r#"
local api = rover.server {}
return api
"#;
        let mut features = AppFeatures::default();
        detect_features(code, &mut features);
        assert!(features.server);
        assert!(!features.ui);
    }

    #[test]
    fn test_detect_features_ui() {
        let code = r#"
rover.render(function()
    return {}
end)
"#;
        let mut features = AppFeatures::default();
        detect_features(code, &mut features);
        assert!(features.ui);
        assert!(!features.server);
    }
}
