use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow};
use mlua::{Lua, MultiValue, Table, Value};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context as RustylineContext, Editor, Helper};

pub struct ReplOptions {
    pub path: Option<PathBuf>,
    pub eval: Option<String>,
}

pub fn run_repl(options: ReplOptions) -> Result<()> {
    let mut session = ReplSession::new(options.path)?;

    if let Some(code) = options.eval {
        let values = session.eval(&code)?;
        print_values(&values)?;
        return Ok(());
    }

    session.run_interactive()
}

struct ReplSession {
    lua: Lua,
    loaded_paths: Vec<PathBuf>,
    source: Rc<RefCell<String>>,
}

impl ReplSession {
    fn new(path: Option<PathBuf>) -> Result<Self> {
        let lua = rover_core::create_lua_runtime(&[], "repl")?;
        let source = Rc::new(RefCell::new(String::new()));
        let mut session = Self {
            lua,
            loaded_paths: Vec::new(),
            source,
        };

        if let Some(path) = path {
            session.load_path(&path, true)?;
        }

        Ok(session)
    }

    fn reset(&mut self) -> Result<()> {
        self.lua = rover_core::create_lua_runtime(&[], "repl")?;
        self.source.borrow_mut().clear();
        let paths = self.loaded_paths.clone();
        self.loaded_paths.clear();
        for path in paths {
            self.load_path(&path, true)?;
        }
        Ok(())
    }

    fn run_interactive(&mut self) -> Result<()> {
        print_banner();

        let helper = ReplHelper {
            source: self.source.clone(),
        };
        let mut editor = Editor::<ReplHelper, DefaultHistory>::new()?;
        editor.set_helper(Some(helper));

        if let Some(path) = history_path()? {
            let _ = editor.load_history(&path);
        }

        let mut buffer = String::new();
        loop {
            let prompt = if buffer.is_empty() {
                "rover> "
            } else {
                "....> "
            };
            match editor.readline(prompt) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if buffer.is_empty() && trimmed.starts_with('.') {
                        if self.handle_command(trimmed)? {
                            break;
                        }
                        continue;
                    }

                    let continues = trimmed.ends_with('\\');
                    if continues {
                        buffer.push_str(trimmed.trim_end_matches('\\'));
                    } else {
                        buffer.push_str(&line);
                    }
                    buffer.push('\n');

                    if continues || is_incomplete(&self.lua, &buffer) {
                        continue;
                    }

                    let code = buffer.trim_end().to_string();
                    buffer.clear();
                    if code.trim().is_empty() {
                        continue;
                    }

                    if !looks_secret(&code) {
                        let _ = editor.add_history_entry(code.as_str());
                    }

                    match self.eval(&code) {
                        Ok(values) => print_values(&values)?,
                        Err(err) => eprintln!("{err}"),
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    buffer.clear();
                    println!("^C");
                }
                Err(ReadlineError::Eof) => break,
                Err(err) => return Err(err.into()),
            }
        }

        if let Some(path) = history_path()? {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let _ = editor.save_history(&path);
        }

        Ok(())
    }

    fn handle_command(&mut self, line: &str) -> Result<bool> {
        let mut parts = line.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or_default().trim();

        match command {
            ".exit" | ".quit" => Ok(true),
            ".help" => {
                print_help();
                Ok(false)
            }
            ".clear" => {
                print!("\x1b[2J\x1b[H");
                Ok(false)
            }
            ".load" => {
                if arg.is_empty() {
                    return Err(anyhow!("usage: .load <path>"));
                }
                self.load_path(Path::new(arg), true)?;
                Ok(false)
            }
            ".reload" => {
                self.reset()?;
                println!("reloaded");
                Ok(false)
            }
            ".vars" => {
                self.print_vars()?;
                Ok(false)
            }
            ".doc" => {
                if arg.is_empty() {
                    return Err(anyhow!("usage: .doc <symbol>"));
                }
                match rover_lsp::repl_symbol_doc(arg) {
                    Some(doc) => println!("{doc}"),
                    None => println!("no docs for {arg}"),
                }
                Ok(false)
            }
            _ => Err(anyhow!("unknown command: {command}. Try .help")),
        }
    }

    fn load_path(&mut self, path: &Path, remember: bool) -> Result<()> {
        let path = fs::canonicalize(path)
            .with_context(|| format!("failed to resolve path: {}", path.display()))?;
        if remember && !self.loaded_paths.contains(&path) {
            self.loaded_paths.push(path.clone());
        }

        if path.is_dir() {
            self.add_package_path(&path)?;
            for entry in ["init.lua", "main.lua"] {
                let candidate = path.join(entry);
                if candidate.is_file() {
                    self.load_file(&candidate)?;
                    println!("loaded {}", candidate.display());
                    return Ok(());
                }
            }
            println!("added {} to package.path", path.display());
            return Ok(());
        }

        self.add_package_path(path.parent().unwrap_or_else(|| Path::new(".")))?;
        self.load_file(&path)?;
        println!("loaded {}", path.display());
        Ok(())
    }

    fn add_package_path(&self, dir: &Path) -> Result<()> {
        let package: Table = self.lua.globals().get("package")?;
        let current: String = package.get("path")?;
        let dir = dir.to_string_lossy().replace('\\', "/");
        let prefix = format!("{dir}/?.lua;{dir}/?/init.lua;");
        if !current.contains(&prefix) {
            package.set("path", format!("{prefix}{current}"))?;
        }
        Ok(())
    }

    fn load_file(&mut self, path: &Path) -> Result<()> {
        let code = fs::read_to_string(path)
            .with_context(|| format!("failed to read file: {}", path.display()))?;
        self.lua
            .load(&code)
            .set_name(path.to_string_lossy().as_ref())
            .eval::<MultiValue>()?;
        self.source.borrow_mut().push_str(&code);
        self.source.borrow_mut().push('\n');
        Ok(())
    }

    fn eval(&mut self, code: &str) -> Result<MultiValue> {
        let values = match self.eval_expr(code) {
            Ok(values) => values,
            Err(_) => self.lua.load(code).set_name("repl").eval::<MultiValue>()?,
        };
        self.source.borrow_mut().push_str(code);
        self.source.borrow_mut().push('\n');
        Ok(values)
    }

    fn eval_expr(&self, code: &str) -> mlua::Result<MultiValue> {
        self.lua
            .load(format!("return {code}"))
            .set_name("repl")
            .eval::<MultiValue>()
    }

    fn print_vars(&self) -> Result<()> {
        let globals = self.lua.globals();
        let hidden = hidden_globals();
        let mut names = Vec::new();
        for pair in globals.pairs::<String, Value>() {
            let (name, _) = pair?;
            if !hidden.contains(name.as_str()) && !name.starts_with('_') {
                names.push(name);
            }
        }
        names.sort();
        for name in names {
            println!("{name}");
        }
        Ok(())
    }
}

struct ReplHelper {
    source: Rc<RefCell<String>>,
}

impl Helper for ReplHelper {}
impl Highlighter for ReplHelper {}
impl Hinter for ReplHelper {
    type Hint = String;
}
impl Validator for ReplHelper {
    fn validate(&self, _: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &RustylineContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = completion_start(line, pos);
        let mut text = self.source.borrow().clone();
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&line[..pos]);
        let line_number = text.lines().count().saturating_sub(1) as u32;
        let character = line[..pos].chars().count() as u32;
        let mut seen = HashSet::new();
        let pairs = rover_lsp::repl_completions(&text, line_number, character)
            .into_iter()
            .filter_map(|item| {
                if seen.insert(item.label.clone()) {
                    Some(Pair {
                        display: item.detail.unwrap_or_else(|| item.label.clone()),
                        replacement: item.label,
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok((start, pairs))
    }
}

fn completion_start(line: &str, pos: usize) -> usize {
    line[..pos]
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                None
            } else {
                Some(idx + ch.len_utf8())
            }
        })
        .unwrap_or(0)
}

fn is_incomplete(lua: &Lua, code: &str) -> bool {
    match lua.load(code).set_name("repl").into_function() {
        Ok(_) => false,
        Err(err) => {
            let msg = err.to_string();
            msg.contains("<eof>") || msg.contains("incomplete")
        }
    }
}

fn print_values(values: &MultiValue) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }

    let rendered = values
        .iter()
        .map(|value| format_value(value, 0))
        .collect::<Result<Vec<_>>>()?;
    println!("{}", rendered.join(", "));
    Ok(())
}

fn format_value(value: &Value, depth: usize) -> Result<String> {
    Ok(match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("{:?}", value.to_str()?),
        Value::Table(table) => format_table(table, depth)?,
        Value::Function(_) => "<function>".to_string(),
        Value::Thread(_) => "<thread>".to_string(),
        Value::UserData(_) => "<userdata>".to_string(),
        Value::LightUserData(_) => "<lightuserdata>".to_string(),
        Value::Error(err) => format!("<error: {err}>",),
        Value::Other(_) => "<value>".to_string(),
    })
}

fn format_table(table: &Table, depth: usize) -> Result<String> {
    if depth >= 2 {
        return Ok("{ ... }".to_string());
    }

    let mut parts = Vec::new();
    for pair in table.clone().pairs::<Value, Value>().take(12) {
        let (key, value) = pair?;
        parts.push(format!(
            "{} = {}",
            format_key(&key)?,
            format_value(&value, depth + 1)?
        ));
    }

    let suffix = if parts.len() == 12 { ", ..." } else { "" };
    Ok(format!("{{ {}{} }}", parts.join(", "), suffix))
}

fn format_key(value: &Value) -> Result<String> {
    Ok(match value {
        Value::String(value) => value.to_str()?.to_string(),
        _ => format!("[{}]", format_value(value, 2)?),
    })
}

fn print_banner() {
    println!("Rover REPL. Type .help for commands, .exit to quit.");
}

fn print_help() {
    println!(".help          show this help");
    println!(".load <path>   load a file or dir");
    println!(".reload        fresh reload loaded paths");
    println!(".doc <symbol>  show Rover docs");
    println!(".vars          list globals");
    println!(".clear         clear screen");
    println!(".exit          quit");
}

fn history_path() -> Result<Option<PathBuf>> {
    Ok(Some(std::env::current_dir()?.join(".rover/repl_history")))
}

fn looks_secret(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    [
        "password",
        "token",
        "secret",
        "api_key",
        "authorization",
        ".env",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

fn hidden_globals() -> HashSet<&'static str> {
    [
        "assert",
        "collectgarbage",
        "coroutine",
        "debug",
        "dofile",
        "error",
        "getmetatable",
        "io",
        "ipairs",
        "load",
        "loadfile",
        "math",
        "next",
        "os",
        "package",
        "pairs",
        "pcall",
        "print",
        "rawequal",
        "rawget",
        "rawlen",
        "rawset",
        "require",
        "select",
        "setmetatable",
        "string",
        "table",
        "tonumber",
        "tostring",
        "type",
        "utf8",
        "xpcall",
    ]
    .into_iter()
    .collect()
}
