#[derive(Debug, Clone)]
pub struct FormatterConfig {
    pub indent_size: usize,
    pub align_tables: bool,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            indent_size: 2,
            align_tables: true,
        }
    }
}

pub struct Formatter {
    config: FormatterConfig,
}

impl Formatter {
    pub fn new(config: FormatterConfig) -> Self {
        Self { config }
    }

    pub fn format(&self, input: &str) -> String {
        let mut output = String::new();
        let mut indent_level = 0;
        let mut chars = input.chars().peekable();
        let mut pending_indent = String::new();

        while let Some(c) = chars.next() {
            match c {
                '\n' => {
                    output.push(c);
                    let indent = " ".repeat(self.config.indent_size * indent_level);
                    pending_indent = indent;
                }
                '\r' => {
                    // Skip CR
                }
                '\t' => {
                    // Replace tabs with spaces
                    output.push_str(&" ".repeat(self.config.indent_size));
                }
                ' ' => {
                    // Only push space if not at start of line
                    if !pending_indent.is_empty() {
                        output.push(c);
                    }
                }
                '{' | '(' => {
                    output.push(c);
                    indent_level += 1;
                }
                '}' | ')' => {
                    if indent_level > 0 {
                        indent_level -= 1;
                    }
                    output.push(c);
                }
                '"' | '\'' => {
                    output.push(c);
                    let mut escaped = false;
                    while let Some(inner) = chars.next() {
                        output.push(inner);
                        if escaped {
                            escaped = false;
                        } else if inner == '\\' {
                            escaped = true;
                        } else if inner == c {
                            break;
                        }
                    }
                }
                '-' => {
                    if let Some(&n) = chars.peek() {
                        if n == '-' {
                            output.push_str("--");
                            chars.next();
                            if let Some(&n2) = chars.peek() {
                                if n2 == '-' {
                                    output.push('-');
                                    chars.next();
                                    // Skip rest of line comment
                                    while let Some(comment) = chars.next() {
                                        output.push(comment);
                                        if comment == '\n' {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        output.push(c);
                    }
                }
                _ => {
                    output.push(c);
                }
            }
        }

        output
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new(FormatterConfig::default())
    }
}

pub fn format_code(input: &str) -> String {
    Formatter::default().format(input)
}

pub fn format_code_with_config(input: &str, config: FormatterConfig) -> String {
    Formatter::new(config).format(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_formatting() {
        let input = "print('hello')";
        let output = format_code(input);
        assert_eq!(output, "print('hello')");
    }

    #[test]
    fn test_string_handling() {
        let input = "print('hello')";
        let output = format_code(input);
        assert!(output.contains("'hello'"));
    }

    #[test]
    fn test_comment_handling() {
        let input = "--- comment\nprint('hello')";
        let output = format_code(input);
        assert!(output.starts_with("---"));
    }
}
