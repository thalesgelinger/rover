use mlua::{Lua, Table, Value};

/// Template segment types
#[derive(Debug, PartialEq)]
pub enum Segment {
    /// Static text to output as-is
    Text(String),
    /// Lua expression to evaluate and insert
    Expression(String),
    /// Control flow statement (if/for/else/elseif/end)
    Control(String),
}

/// Parse a template string into segments
pub fn parse_template(template: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_pos = 0;
    let bytes = template.as_bytes();
    let len = bytes.len();

    while current_pos < len {
        // Find next {{
        if let Some(start) = find_pattern(bytes, current_pos, b"{{") {
            // Add text before {{
            if start > current_pos {
                let text = &template[current_pos..start];
                if !text.is_empty() {
                    segments.push(Segment::Text(text.to_string()));
                }
            }

            // Find matching }}
            if let Some(end) = find_pattern(bytes, start + 2, b"}}") {
                let content = template[start + 2..end].trim();

                // Classify the content
                let segment = classify_content(content);
                segments.push(segment);

                current_pos = end + 2;
            } else {
                // No closing }}, treat {{ and everything after as literal text
                let text = &template[start..];
                if !text.is_empty() {
                    segments.push(Segment::Text(text.to_string()));
                }
                break;
            }
        } else {
            // No more {{, add remaining text
            let text = &template[current_pos..];
            if !text.is_empty() {
                segments.push(Segment::Text(text.to_string()));
            }
            break;
        }
    }

    segments
}

/// Find a byte pattern in a byte slice starting from offset
fn find_pattern(bytes: &[u8], start: usize, pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || start + pattern.len() > bytes.len() {
        return None;
    }

    for i in start..=bytes.len() - pattern.len() {
        if &bytes[i..i + pattern.len()] == pattern {
            return Some(i);
        }
    }
    None
}

/// Classify content as expression or control flow
fn classify_content(content: &str) -> Segment {
    let trimmed = content.trim();

    // Check for control flow keywords at the start
    if trimmed.starts_with("if ") && trimmed.ends_with(" then") {
        Segment::Control(trimmed.to_string())
    } else if trimmed.starts_with("elseif ") && trimmed.ends_with(" then") {
        Segment::Control(trimmed.to_string())
    } else if trimmed == "else" {
        Segment::Control(trimmed.to_string())
    } else if trimmed == "end" {
        Segment::Control(trimmed.to_string())
    } else if trimmed.starts_with("for ") && trimmed.ends_with(" do") {
        Segment::Control(trimmed.to_string())
    } else {
        Segment::Expression(trimmed.to_string())
    }
}

/// Escape a string for use in Lua string literal
fn escape_lua_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 10);
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

/// Generate Lua code from parsed segments
pub fn generate_lua_code(segments: &[Segment]) -> String {
    let mut code = String::with_capacity(1024);

    // Start building the function
    code.push_str("local __out = {}\n");
    code.push_str("local __n = 0\n");
    code.push_str("local function __emit(v)\n");
    code.push_str("  __n = __n + 1\n");
    code.push_str("  __out[__n] = tostring(v)\n");
    code.push_str("end\n");

    for segment in segments {
        match segment {
            Segment::Text(text) => {
                if !text.is_empty() {
                    code.push_str("__emit(\"");
                    code.push_str(&escape_lua_string(text));
                    code.push_str("\")\n");
                }
            }
            Segment::Expression(expr) => {
                code.push_str("__emit(");
                code.push_str(expr);
                code.push_str(")\n");
            }
            Segment::Control(ctrl) => {
                code.push_str(ctrl);
                code.push('\n');
            }
        }
    }

    // Return concatenated output
    code.push_str("return table.concat(__out)\n");

    code
}

/// Render a template with the given data context
pub fn render_template(lua: &Lua, template: &str, data: &Table) -> mlua::Result<String> {
    // Parse template into segments
    let segments = parse_template(template);

    // Generate Lua code
    let lua_code = generate_lua_code(&segments);

    // Create a new environment with data as the fallback
    let env = lua.create_table()?;

    // Copy standard library functions we need
    let globals = lua.globals();
    env.set("tostring", globals.get::<Value>("tostring")?)?;
    env.set("tonumber", globals.get::<Value>("tonumber")?)?;
    env.set("ipairs", globals.get::<Value>("ipairs")?)?;
    env.set("pairs", globals.get::<Value>("pairs")?)?;
    env.set("table", globals.get::<Value>("table")?)?;
    env.set("string", globals.get::<Value>("string")?)?;
    env.set("math", globals.get::<Value>("math")?)?;
    env.set("type", globals.get::<Value>("type")?)?;
    env.set("next", globals.get::<Value>("next")?)?;
    env.set("select", globals.get::<Value>("select")?)?;
    env.set("unpack", globals.get::<Value>("unpack")?)?;
    env.set("pcall", globals.get::<Value>("pcall")?)?;
    env.set("error", globals.get::<Value>("error")?)?;

    // Copy all data fields into environment
    for pair in data.pairs::<Value, Value>() {
        let (key, value) = pair?;
        env.set(key, value)?;
    }

    // Add rover.html to environment for component support
    if let Ok(rover) = globals.get::<Table>("rover") {
        env.set("rover", rover)?;
    }

    // Load and execute the generated code
    let chunk = lua.load(&lua_code).set_environment(env);

    let result: String = chunk.eval().map_err(|e| {
        mlua::Error::RuntimeError(format!(
            "Template rendering failed: {}\nGenerated code:\n{}",
            e, lua_code
        ))
    })?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Parsing Tests ====================

    #[test]
    fn test_parse_simple_text() {
        let segments = parse_template("<h1>Hello World</h1>");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Text("<h1>Hello World</h1>".to_string()));
    }

    #[test]
    fn test_parse_empty_template() {
        let segments = parse_template("");
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn test_parse_expression() {
        let segments = parse_template("<h1>{{ name }}</h1>");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Text("<h1>".to_string()));
        assert_eq!(segments[1], Segment::Expression("name".to_string()));
        assert_eq!(segments[2], Segment::Text("</h1>".to_string()));
    }

    #[test]
    fn test_parse_nested_object_access() {
        let segments = parse_template("{{ user.profile.name }}");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Expression("user.profile.name".to_string()));
    }

    #[test]
    fn test_parse_method_call() {
        let segments = parse_template("{{ user.name:upper() }}");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Expression("user.name:upper()".to_string()));
    }

    #[test]
    fn test_parse_function_call() {
        let segments = parse_template("{{ greet(name) }}");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Expression("greet(name)".to_string()));
    }

    #[test]
    fn test_parse_arithmetic() {
        let segments = parse_template("{{ 1 + 2 * 3 }}");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Expression("1 + 2 * 3".to_string()));
    }

    #[test]
    fn test_parse_control_flow() {
        let segments = parse_template("{{ if show then }}visible{{ end }}");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Control("if show then".to_string()));
        assert_eq!(segments[1], Segment::Text("visible".to_string()));
        assert_eq!(segments[2], Segment::Control("end".to_string()));
    }

    #[test]
    fn test_parse_if_else() {
        let segments = parse_template("{{ if cond then }}yes{{ else }}no{{ end }}");
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], Segment::Control("if cond then".to_string()));
        assert_eq!(segments[1], Segment::Text("yes".to_string()));
        assert_eq!(segments[2], Segment::Control("else".to_string()));
        assert_eq!(segments[3], Segment::Text("no".to_string()));
        assert_eq!(segments[4], Segment::Control("end".to_string()));
    }

    #[test]
    fn test_parse_elseif() {
        let segments = parse_template("{{ if a then }}A{{ elseif b then }}B{{ else }}C{{ end }}");
        assert_eq!(segments.len(), 7);
        assert_eq!(segments[0], Segment::Control("if a then".to_string()));
        assert_eq!(segments[2], Segment::Control("elseif b then".to_string()));
        assert_eq!(segments[4], Segment::Control("else".to_string()));
        assert_eq!(segments[6], Segment::Control("end".to_string()));
    }

    #[test]
    fn test_parse_for_loop() {
        let segments = parse_template("{{ for i, v in ipairs(items) do }}{{ v }}{{ end }}");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Control("for i, v in ipairs(items) do".to_string()));
        assert_eq!(segments[1], Segment::Expression("v".to_string()));
        assert_eq!(segments[2], Segment::Control("end".to_string()));
    }

    #[test]
    fn test_parse_numeric_for() {
        let segments = parse_template("{{ for i = 1, 10 do }}{{ i }}{{ end }}");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Control("for i = 1, 10 do".to_string()));
    }

    #[test]
    fn test_parse_multiple_expressions() {
        let segments = parse_template("{{ a }} and {{ b }}");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Expression("a".to_string()));
        assert_eq!(segments[1], Segment::Text(" and ".to_string()));
        assert_eq!(segments[2], Segment::Expression("b".to_string()));
    }

    #[test]
    fn test_parse_unclosed_braces() {
        // When {{ is found but no }}, the text before {{ is one segment,
        // and the unclosed {{ with content is treated as literal text
        let segments = parse_template("Hello {{ name");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], Segment::Text("Hello ".to_string()));
        assert_eq!(segments[1], Segment::Text("{{ name".to_string()));
    }

    #[test]
    fn test_parse_nested_braces_in_table() {
        // Component call with table argument
        let segments = parse_template("{{ card { title = \"Hello\" } }}");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], Segment::Expression("card { title = \"Hello\" }".to_string()));
    }

    // ==================== Escape Tests ====================

    #[test]
    fn test_escape_lua_string() {
        assert_eq!(escape_lua_string("hello"), "hello");
        assert_eq!(escape_lua_string("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_lua_string("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_escape_backslash() {
        assert_eq!(escape_lua_string("path\\to\\file"), "path\\\\to\\\\file");
    }

    #[test]
    fn test_escape_tabs() {
        assert_eq!(escape_lua_string("col1\tcol2"), "col1\\tcol2");
    }

    #[test]
    fn test_escape_carriage_return() {
        assert_eq!(escape_lua_string("line1\r\nline2"), "line1\\r\\nline2");
    }

    // ==================== Code Generation Tests ====================

    #[test]
    fn test_generate_lua_code() {
        let segments = vec![
            Segment::Text("<h1>".to_string()),
            Segment::Expression("name".to_string()),
            Segment::Text("</h1>".to_string()),
        ];
        let code = generate_lua_code(&segments);
        assert!(code.contains("__emit(\"<h1>\")"));
        assert!(code.contains("__emit(name)"));
        assert!(code.contains("__emit(\"</h1>\")"));
    }

    #[test]
    fn test_generate_lua_code_with_control_flow() {
        let segments = vec![
            Segment::Control("if show then".to_string()),
            Segment::Text("visible".to_string()),
            Segment::Control("end".to_string()),
        ];
        let code = generate_lua_code(&segments);
        assert!(code.contains("if show then"));
        assert!(code.contains("__emit(\"visible\")"));
        assert!(code.contains("end"));
    }

    // ==================== Rendering Tests ====================

    #[test]
    fn test_render_simple_text() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let result = render_template(&lua, "<h1>Hello World</h1>", &data).unwrap();
        assert_eq!(result, "<h1>Hello World</h1>");
    }

    #[test]
    fn test_render_variable() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("name", "Thales").unwrap();
        let result = render_template(&lua, "Hello {{ name }}!", &data).unwrap();
        assert_eq!(result, "Hello Thales!");
    }

    #[test]
    fn test_render_nested_object() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let user = lua.create_table().unwrap();
        user.set("name", "Thales").unwrap();
        data.set("user", user).unwrap();
        let result = render_template(&lua, "Hello {{ user.name }}!", &data).unwrap();
        assert_eq!(result, "Hello Thales!");
    }

    #[test]
    fn test_render_deeply_nested_object() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let user = lua.create_table().unwrap();
        let profile = lua.create_table().unwrap();
        profile.set("name", "Thales").unwrap();
        user.set("profile", profile).unwrap();
        data.set("user", user).unwrap();
        let result = render_template(&lua, "{{ user.profile.name }}", &data).unwrap();
        assert_eq!(result, "Thales");
    }

    #[test]
    fn test_render_arithmetic() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let result = render_template(&lua, "{{ 1 + 2 * 3 }}", &data).unwrap();
        assert_eq!(result, "7");
    }

    #[test]
    fn test_render_string_method() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("name", "thales").unwrap();
        let result = render_template(&lua, "{{ name:upper() }}", &data).unwrap();
        assert_eq!(result, "THALES");
    }

    #[test]
    fn test_render_function_call() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let greet = lua.create_function(|_, name: String| {
            Ok(format!("Hello, {}!", name))
        }).unwrap();
        data.set("greet", greet).unwrap();
        data.set("name", "World").unwrap();
        let result = render_template(&lua, "{{ greet(name) }}", &data).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_if_true() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("show", true).unwrap();
        let result = render_template(&lua, "{{ if show then }}visible{{ end }}", &data).unwrap();
        assert_eq!(result, "visible");
    }

    #[test]
    fn test_render_if_false() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("show", false).unwrap();
        let result = render_template(&lua, "{{ if show then }}visible{{ end }}", &data).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_if_else() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("admin", false).unwrap();
        let result = render_template(
            &lua,
            "{{ if admin then }}Admin{{ else }}User{{ end }}",
            &data
        ).unwrap();
        assert_eq!(result, "User");
    }

    #[test]
    fn test_render_elseif() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("role", "editor").unwrap();
        let template = r#"{{ if role == "admin" then }}Admin{{ elseif role == "editor" then }}Editor{{ else }}User{{ end }}"#;
        let result = render_template(&lua, template, &data).unwrap();
        assert_eq!(result, "Editor");
    }

    #[test]
    fn test_render_for_ipairs() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let items = lua.create_table().unwrap();
        items.set(1, "A").unwrap();
        items.set(2, "B").unwrap();
        items.set(3, "C").unwrap();
        data.set("items", items).unwrap();
        let result = render_template(
            &lua,
            "{{ for _, v in ipairs(items) do }}{{ v }}{{ end }}",
            &data
        ).unwrap();
        assert_eq!(result, "ABC");
    }

    #[test]
    fn test_render_for_with_index() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let items = lua.create_table().unwrap();
        items.set(1, "A").unwrap();
        items.set(2, "B").unwrap();
        data.set("items", items).unwrap();
        let result = render_template(
            &lua,
            "{{ for i, v in ipairs(items) do }}{{ i }}:{{ v }} {{ end }}",
            &data
        ).unwrap();
        assert_eq!(result, "1:A 2:B ");
    }

    #[test]
    fn test_render_nested_for() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let rows = lua.create_table().unwrap();

        let row1 = lua.create_table().unwrap();
        row1.set(1, 1).unwrap();
        row1.set(2, 2).unwrap();
        rows.set(1, row1).unwrap();

        let row2 = lua.create_table().unwrap();
        row2.set(1, 3).unwrap();
        row2.set(2, 4).unwrap();
        rows.set(2, row2).unwrap();

        data.set("rows", rows).unwrap();

        let template = "{{ for _, row in ipairs(rows) do }}[{{ for _, cell in ipairs(row) do }}{{ cell }}{{ end }}]{{ end }}";
        let result = render_template(&lua, template, &data).unwrap();
        assert_eq!(result, "[12][34]");
    }

    #[test]
    fn test_render_for_with_table_items() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let items = lua.create_table().unwrap();

        let item1 = lua.create_table().unwrap();
        item1.set("title", "First").unwrap();
        items.set(1, item1).unwrap();

        let item2 = lua.create_table().unwrap();
        item2.set("title", "Second").unwrap();
        items.set(2, item2).unwrap();

        data.set("items", items).unwrap();

        let result = render_template(
            &lua,
            "<ul>{{ for _, item in ipairs(items) do }}<li>{{ item.title }}</li>{{ end }}</ul>",
            &data
        ).unwrap();
        assert_eq!(result, "<ul><li>First</li><li>Second</li></ul>");
    }

    #[test]
    fn test_render_numeric_for() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let result = render_template(
            &lua,
            "{{ for i = 1, 3 do }}{{ i }}{{ end }}",
            &data
        ).unwrap();
        assert_eq!(result, "123");
    }

    #[test]
    fn test_render_empty_data() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let result = render_template(&lua, "<div>Static content</div>", &data).unwrap();
        assert_eq!(result, "<div>Static content</div>");
    }

    #[test]
    fn test_render_nil_variable() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let result = render_template(&lua, "Value: {{ missing }}", &data).unwrap();
        assert_eq!(result, "Value: nil");
    }

    #[test]
    fn test_render_concat() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("first", "Hello").unwrap();
        data.set("second", "World").unwrap();
        let result = render_template(&lua, "{{ first .. \" \" .. second }}", &data).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_render_html_in_data() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("content", "<strong>Bold</strong>").unwrap();
        let result = render_template(&lua, "<div>{{ content }}</div>", &data).unwrap();
        assert_eq!(result, "<div><strong>Bold</strong></div>");
    }

    #[test]
    fn test_render_complex_html() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();

        let user = lua.create_table().unwrap();
        user.set("name", "Thales").unwrap();
        data.set("user", user).unwrap();

        let items = lua.create_table().unwrap();
        let item1 = lua.create_table().unwrap();
        item1.set("title", "Item 1").unwrap();
        items.set(1, item1).unwrap();
        let item2 = lua.create_table().unwrap();
        item2.set("title", "Item 2").unwrap();
        items.set(2, item2).unwrap();
        data.set("items", items).unwrap();

        let template = r#"<h1>Hello {{ user.name }}</h1>
<ul>
{{ for _, item in ipairs(items) do }}
  <li>{{ item.title }}</li>
{{ end }}
</ul>"#;

        let result = render_template(&lua, template, &data).unwrap();
        assert!(result.contains("Hello Thales"));
        assert!(result.contains("<li>Item 1</li>"));
        assert!(result.contains("<li>Item 2</li>"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_consecutive_expressions() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("a", "X").unwrap();
        data.set("b", "Y").unwrap();
        let result = render_template(&lua, "{{ a }}{{ b }}", &data).unwrap();
        assert_eq!(result, "XY");
    }

    #[test]
    fn test_whitespace_in_expression() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("name", "Test").unwrap();
        let result = render_template(&lua, "{{   name   }}", &data).unwrap();
        assert_eq!(result, "Test");
    }

    #[test]
    fn test_multiline_template() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("title", "Hello").unwrap();
        let template = r#"<html>
<head>
  <title>{{ title }}</title>
</head>
</html>"#;
        let result = render_template(&lua, template, &data).unwrap();
        assert!(result.contains("<title>Hello</title>"));
    }

    #[test]
    fn test_boolean_in_condition() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("count", 5).unwrap();
        let result = render_template(
            &lua,
            "{{ if count > 3 then }}many{{ else }}few{{ end }}",
            &data
        ).unwrap();
        assert_eq!(result, "many");
    }

    #[test]
    fn test_table_length() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let items = lua.create_table().unwrap();
        items.set(1, "a").unwrap();
        items.set(2, "b").unwrap();
        items.set(3, "c").unwrap();
        data.set("items", items).unwrap();
        let result = render_template(&lua, "Count: {{ #items }}", &data).unwrap();
        assert_eq!(result, "Count: 3");
    }
}
