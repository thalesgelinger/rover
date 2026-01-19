use colored::*;
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorType {
    Assertion,
    NilAccess,
    TypeError,
    Validation,
    Runtime,
}

#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub file: String,
    pub line: usize,
    pub col: Option<usize>,
    pub error_type: ErrorType,
    pub message: String,
    pub code_lines: Vec<String>,
    pub error_line_idx: usize,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub file: String,
    pub line: Option<usize>,
    pub function: Option<String>,
}

pub fn parse_lua_error(error_str: &str, fallback_file: &str) -> (ErrorInfo, Option<String>) {
    let mut file = fallback_file.to_string();
    let mut line = 1;
    let mut col = None;

    if let Some(pos) = error_str.find(r#"[string ""#) {
        let after = &error_str[pos + 9..];

        if let Some(end_quote) = after.find('"') {
            file = after[..end_quote].to_string();
            let rest = &after[end_quote + 1..];

            if rest.starts_with("]:") {
                let after_colon = &rest[2..];

                if let Some(colon) = after_colon.find(':') {
                    let line_str = &after_colon[..colon];
                    if let Ok(num) = line_str.parse::<usize>() {
                        line = num;

                        let after_line = &after_colon[colon + 1..];
                        if after_line.starts_with(" ") || !after_line.starts_with(":") {
                            // No column, just message after line number
                        } else if after_line.starts_with(":") {
                            let after_line_col = &after_line[1..];
                            if let Some(end_msg) =
                                after_line_col.find(|c: char| c.is_whitespace() || c == ':')
                            {
                                let col_str = &after_line_col[..end_msg];
                                col = col_str.parse().ok();
                            }
                        }
                    }
                }
            }
        }
    }

    // Split error string into first line (error) and rest (stack trace)
    let (first_line, rest) = if let Some(newline_pos) = error_str.find('\n') {
        (&error_str[..newline_pos], Some(&error_str[newline_pos..]))
    } else {
        (error_str, None)
    };

    // Extract clean error message from first line
    // Pattern: runtime error: [string "file"]:line: message
    let message = {
        if let Some(bracket_pos) = first_line.find(r#"[string ""#) {
            let after_bracket = &first_line[bracket_pos..];

            // Find the closing quote and bracket "]:
            if let Some(quote_pos) = after_bracket.find(r#""]:"#) {
                let after_file = &after_bracket[quote_pos + 3..]; // Skip "]:

                // Skip line number (digits followed by colon)
                if let Some(msg_colon) = after_file.find(": ") {
                    after_file[msg_colon + 2..].trim().to_string()
                } else {
                    first_line.trim().to_string()
                }
            } else {
                first_line.trim().to_string()
            }
        } else {
            first_line.trim().to_string()
        }
    };

    let stack_trace = if let Some(rest_str) = rest {
        if rest_str.contains("stack traceback:") {
            Some(rest_str.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let error_type = detect_error_type(&message);
    let hint = generate_hint(&error_type, &message);

    let (code_lines, error_line_idx) = extract_code_snippet(&file, line);

    (
        ErrorInfo {
            file,
            line,
            col,
            error_type,
            message,
            code_lines,
            error_line_idx,
            hint,
        },
        stack_trace,
    )
}

fn detect_error_type(message: &str) -> ErrorType {
    let msg_lower = message.to_lowercase();

    if msg_lower.contains("assertion") {
        return ErrorType::Assertion;
    }

    if msg_lower.contains("nil value") {
        return ErrorType::NilAccess;
    }

    if msg_lower.contains("arithmetic on")
        && (msg_lower.contains("string")
            || msg_lower.contains("boolean")
            || msg_lower.contains("table"))
    {
        return ErrorType::TypeError;
    }

    if msg_lower.contains("validation") {
        return ErrorType::Validation;
    }

    if msg_lower.contains("attempt to call global") {
        return ErrorType::Runtime;
    }

    ErrorType::Runtime
}

fn generate_hint(error_type: &ErrorType, message: &str) -> Option<String> {
    match error_type {
        ErrorType::Assertion => Some("Check condition and provide a valid value".to_string()),
        ErrorType::NilAccess => {
            if message.contains("index") {
                Some("Ensure variable is not nil before accessing fields".to_string())
            } else if message.contains("call") {
                Some("Ensure function is not nil before calling".to_string())
            } else {
                Some("Check that value is initialized before use".to_string())
            }
        }
        ErrorType::TypeError => {
            if message.contains("string") {
                Some("Convert to number with tonumber()".to_string())
            } else if message.contains("boolean") {
                Some("Convert boolean to number explicitly: tonumber(bool)".to_string())
            } else if message.contains("table") {
                Some("Use appropriate indexing or convert table to number".to_string())
            } else {
                Some("Ensure both operands have compatible types".to_string())
            }
        }
        ErrorType::Validation => Some("Review required fields and value types".to_string()),
        ErrorType::Runtime => {
            if message.contains("call global") {
                Some("Check function spelling or define it first".to_string())
            } else {
                Some("Review error message and fix the runtime issue".to_string())
            }
        }
    }
}

fn extract_code_snippet(file: &str, line: usize) -> (Vec<String>, usize) {
    if file.starts_with(r#"[string"#) || !file.ends_with(".lua") {
        return (vec![], 0);
    }

    match fs::read_to_string(file) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let context = 2;
            let start = if line > context + 1 {
                line - context - 1
            } else {
                0
            };
            let end = (line + context).min(lines.len());

            let code_lines: Vec<String> = lines[start..end].iter().map(|s| s.to_string()).collect();
            let error_line_idx = if line > context + 1 {
                context
            } else {
                line - 1
            };

            (code_lines, error_line_idx)
        }
        Err(_) => (vec![], 0),
    }
}

fn format_caret(
    line: &str,
    col_start: Option<usize>,
    col_end: Option<usize>,
    line_num_width: usize,
) -> String {
    if line.is_empty() {
        return String::new();
    }

    // If no column specified, don't show caret
    if col_start.is_none() {
        return String::new();
    }

    let start = col_start.unwrap().saturating_sub(1);
    let end = col_end.unwrap_or(start + 1);

    let num_spaces = line
        .chars()
        .take(start)
        .map(|c| if c == '\t' { 8 } else { 1 })
        .sum::<usize>();
    let num_carets = if end > start {
        line.chars()
            .skip(start)
            .take(end - start)
            .map(|c| if c == '\t' { 8 } else { 1 })
            .sum::<usize>()
    } else {
        1
    };

    let caret_str = "^".repeat(num_carets.max(1));

    format!(
        "{:width$} | {}{}",
        "",
        " ".repeat(num_spaces),
        caret_str.red(),
        width = line_num_width
    )
}

fn generate_clickable_link(file: &str, line: usize, col: Option<usize>) -> String {
    let col_str = col.map(|c| format!(":{}", c)).unwrap_or_default();
    format!(
        "{}:{}{}",
        file.bright_white(),
        line.to_string().yellow(),
        col_str.to_string().yellow()
    )
}

pub fn format_error_display(error: &ErrorInfo) -> String {
    let type_str = match error.error_type {
        ErrorType::Assertion => "assertion",
        ErrorType::NilAccess => "nil_access",
        ErrorType::TypeError => "type_error",
        ErrorType::Validation => "validation",
        ErrorType::Runtime => "runtime",
    };

    let mut output = String::new();

    output.push_str(&format!(
        "{}[{}]: {}\n",
        "error".bright_red().bold(),
        type_str,
        error.message
    ));
    output.push_str(&format!(
        "   → {}\n",
        generate_clickable_link(&error.file, error.line, error.col)
    ));

    if !error.code_lines.is_empty() {
        // Calculate max line number width for alignment
        let max_line_num = if error.line > 3 {
            error.line + 2
        } else {
            error.code_lines.len()
        };
        let line_num_width = max_line_num.to_string().len();

        output.push_str("\n");

        for (i, line) in error.code_lines.iter().enumerate() {
            let display_num = if error.line > 3 {
                error.line - 3 + i + 1
            } else {
                i + 1
            };
            output.push_str(&format!(
                " {:width$} | {}\n",
                display_num,
                line.dimmed(),
                width = line_num_width
            ));

            if i == error.error_line_idx {
                let caret = format_caret(line, error.col, error.col, line_num_width);
                if !caret.is_empty() {
                    output.push_str(&format!("{}\n", caret));
                }
            }
        }

        output.push_str("\n");
    }

    if let Some(ref hint) = error.hint {
        output.push_str(&format!(" {}: {}\n", "help".cyan().bold(), hint.cyan()));
    }

    output
}

fn parse_stack_frame(line: &str) -> Option<StackFrame> {
    if line.starts_with("[C]:") || line.is_empty() {
        return None;
    }

    if let Some(pos) = line.find(r#"[string ""#) {
        let after = &line[pos + 9..];

        if let Some(end_quote) = after.find('"') {
            let file = after[..end_quote].to_string();
            let rest = &after[end_quote + 1..];

            if rest.starts_with("]:") {
                let after_colon = &rest[2..];

                if let Some(colon) = after_colon.find(':') {
                    let line_str = &after_colon[..colon];
                    let line_num = line_str.parse().ok();

                    let function = if line.contains(" in ") {
                        // Extract function name from patterns like:
                        // "in function 'name'" or "in upvalue 'name'" or "in local 'name'"
                        if let Some(in_pos) = line.find(" in ") {
                            let after_in = &line[in_pos + 4..];
                            if let Some(start) = after_in.find("'") {
                                if let Some(end) = after_in[start + 1..].find("'") {
                                    Some(after_in[start + 1..start + 1 + end].to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    return Some(StackFrame {
                        file,
                        line: line_num,
                        function,
                    });
                }
            }
        }
    }

    None
}

pub fn format_simplified_stack_trace(stack_str: &str) -> Vec<StackFrame> {
    let mut frames = Vec::new();
    let lines: Vec<&str> = stack_str.lines().collect();

    for line in lines.iter().rev() {
        if let Some(frame) = parse_stack_frame(line) {
            frames.push(frame);
        }
    }

    frames.truncate(3);
    frames.reverse();
    frames
}

pub fn display_error(error: &ErrorInfo) {
    eprintln!("{}", format_error_display(error));
}

pub fn display_error_with_stack(error: &ErrorInfo, stack_str: Option<&str>) {
    display_error(error);

    if let Some(stack) = stack_str {
        let frames = format_simplified_stack_trace(stack);

        if !frames.is_empty() {
            eprintln!("    {}", "Called from:".dimmed());

            for frame in frames {
                let line_str = frame.line.map(|l| format!(":{}", l)).unwrap_or_default();
                let func_str = frame
                    .function
                    .map(|f| format!(" in function '{}'", f))
                    .unwrap_or_default();

                eprintln!(
                    "    {}{}",
                    " → ".dimmed(),
                    format!("{}{}{}", frame.file, line_str, func_str).bright_white()
                );
            }

            eprintln!();
        }
    }
}
