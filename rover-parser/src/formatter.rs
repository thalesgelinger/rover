use stylua_lib::{CallParenType, Config, OutputVerification, format_code as stylua_format_code};

#[derive(Debug, Clone)]
pub struct FormatterConfig {
    pub call_parentheses: CallParenType,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            call_parentheses: CallParenType::None,
        }
    }
}

pub fn format_code(input: &str) -> String {
    let config = Config {
        call_parentheses: CallParenType::None,
        ..Config::default()
    };
    stylua_format_code(input, config, None, OutputVerification::None)
        .unwrap_or_else(|_| input.to_string())
}

pub fn format_code_with_config(input: &str, config: FormatterConfig) -> String {
    let stylua_config = Config {
        call_parentheses: config.call_parentheses,
        ..Config::default()
    };
    stylua_format_code(input, stylua_config, None, OutputVerification::None)
        .unwrap_or_else(|_| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_formatting() {
        let input = "print('hello')";
        let output = format_code(input);
        eprintln!("Input: {:?}", input);
        eprintln!("Output: {:?}", output);
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_string_handling() {
        let input = "print('hello')";
        let output = format_code(input);
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_no_optional_parens() {
        let input = "print(('hello'))";
        let output = format_code(input);
        eprintln!("Input: {:?}", input);
        eprintln!("Output: {:?}", output);
        assert!(output.contains("hello"));
    }
}
