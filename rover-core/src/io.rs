use mlua::prelude::*;
use std::cell::RefCell;
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

thread_local! {
    static CURRENT_INPUT: RefCell<Option<LuaAnyUserData>> = RefCell::new(None);
    static CURRENT_OUTPUT: RefCell<Option<LuaAnyUserData>> = RefCell::new(None);
}

pub struct SyncFile {
    file: Option<File>,
    reader: Option<BufReader<File>>,
    path: PathBuf,
    _mode: String,
}

pub struct StdinHandle;
pub struct StdoutHandle;
pub struct StderrHandle;

impl StdinHandle {
    fn new() -> Self {
        StdinHandle
    }
}

impl StdoutHandle {
    fn new() -> Self {
        StdoutHandle
    }
}

impl StderrHandle {
    fn new() -> Self {
        StderrHandle
    }
}

impl LuaUserData for StdinHandle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, _this, format: Option<LuaValue>| {
            let mut stdin = std::io::stdin();

            let format_str = match format {
                Some(LuaValue::String(s)) => s.to_str()?.to_string(),
                Some(LuaValue::Integer(n)) => n.to_string(),
                Some(LuaValue::Number(n)) => (n as i64).to_string(),
                None => "*l".to_string(),
                _ => return Err(LuaError::RuntimeError("Invalid read format".to_string())),
            };

            match format_str.as_str() {
                "*a" | "*all" => {
                    let mut contents = String::new();
                    stdin
                        .read_to_string(&mut contents)
                        .map_err(|e| LuaError::external(e))?;
                    Ok(LuaValue::String(lua.create_string(contents)?))
                }
                "*l" | "*line" => {
                    let mut line = String::new();
                    let bytes_read = stdin
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(LuaValue::Nil);
                    }

                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }

                    Ok(LuaValue::String(lua.create_string(line)?))
                }
                "*L" => {
                    let mut line = String::new();
                    let bytes_read = stdin
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(LuaValue::Nil);
                    }

                    Ok(LuaValue::String(lua.create_string(line)?))
                }
                "*n" => {
                    let mut line = String::new();
                    stdin
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    let trimmed = line.trim();
                    trimmed
                        .parse::<f64>()
                        .map(LuaValue::Number)
                        .map_err(|_| LuaError::RuntimeError("Not a number".to_string()))
                }
                num_str => {
                    let n = num_str
                        .parse::<usize>()
                        .map_err(|_| LuaError::RuntimeError("Invalid byte count".to_string()))?;

                    let mut buffer = vec![0u8; n];
                    let bytes_read = stdin.read(&mut buffer).map_err(|e| LuaError::external(e))?;

                    buffer.truncate(bytes_read);
                    match String::from_utf8(buffer) {
                        Ok(s) => match lua.create_string(s) {
                            Ok(lua_str) => Ok(LuaValue::String(lua_str)),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(LuaError::RuntimeError(format!("Invalid UTF-8: {}", e))),
                    }
                }
            }
        });

        methods.add_method("lines", |_lua, _this, ()| {
            Err::<(), _>(LuaError::RuntimeError(
                "Stdin does not support lines()".to_string(),
            ))
        });

        methods.add_method_mut("close", |_lua, _this, ()| {
            Err::<(), _>(LuaError::RuntimeError("Cannot close stdin".to_string()))
        });
    }
}

impl LuaUserData for StdoutHandle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("write", |_lua, this, data: LuaValue| {
            let mut stdout = std::io::stdout();

            let text = match data {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Integer(i) => i.to_string(),
                LuaValue::Number(n) => n.to_string(),
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Can only write strings or numbers".to_string(),
                    ));
                }
            };

            stdout
                .write_all(text.as_bytes())
                .map_err(|e| LuaError::external(e))?;

            Ok(())
        });

        methods.add_method_mut("flush", |_lua, _this, ()| {
            let mut stdout = std::io::stdout();
            stdout.flush().map_err(|e| LuaError::external(e))?;
            Ok(())
        });

        methods.add_method_mut("close", |_lua, _this, ()| {
            Err::<(), _>(LuaError::RuntimeError("Cannot close stdout".to_string()))
        });
    }
}

impl LuaUserData for StderrHandle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("write", |_lua, this, data: LuaValue| {
            let mut stderr = std::io::stderr();

            let text = match data {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Integer(i) => i.to_string(),
                LuaValue::Number(n) => n.to_string(),
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Can only write strings or numbers".to_string(),
                    ));
                }
            };

            stderr
                .write_all(text.as_bytes())
                .map_err(|e| LuaError::external(e))?;

            Ok(())
        });

        methods.add_method_mut("flush", |_lua, _this, ()| {
            let mut stderr = std::io::stderr();
            stderr.flush().map_err(|e| LuaError::external(e))?;
            Ok(())
        });

        methods.add_method_mut("close", |_lua, _this, ()| {
            Err::<(), _>(LuaError::RuntimeError("Cannot close stderr".to_string()))
        });
    }
}

impl SyncFile {
    fn open(path: String, mode: Option<String>) -> LuaResult<Self> {
        let mode = mode.unwrap_or_else(|| "r".to_string());
        let path_buf = PathBuf::from(&path);

        let file = match mode.as_str() {
            "r" | "rb" => StdOpenOptions::new()
                .read(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            "w" | "wb" => StdOpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            "a" | "ab" => StdOpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            "r+" | "rb+" | "r+b" => StdOpenOptions::new()
                .read(true)
                .write(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            "w+" | "wb+" | "w+b" => StdOpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            "a+" | "ab+" | "a+b" => StdOpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .append(true)
                .open(&path_buf)
                .map_err(|e| LuaError::external(e))?,
            _ => return Err(LuaError::RuntimeError(format!("Invalid mode: {}", mode))),
        };

        let reader = BufReader::new(file.try_clone().map_err(|e| LuaError::external(e))?);

        Ok(SyncFile {
            file: Some(file),
            reader: Some(reader),
            path: path_buf,
            _mode: mode,
        })
    }
}

impl LuaUserData for SyncFile {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("read", |lua, this, format: Option<LuaValue>| {
            let reader = this.reader.as_mut().ok_or_else(|| {
                LuaError::RuntimeError("File is closed or read mode not set".to_string())
            })?;

            let format_str = match format {
                Some(LuaValue::String(s)) => s.to_str()?.to_string(),
                Some(LuaValue::Integer(n)) => n.to_string(),
                Some(LuaValue::Number(n)) => (n as i64).to_string(),
                None => "*l".to_string(),
                _ => return Err(LuaError::RuntimeError("Invalid read format".to_string())),
            };

            match format_str.as_str() {
                "*a" | "*all" => {
                    let file = this
                        .file
                        .as_mut()
                        .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)
                        .map_err(|e| LuaError::external(e))?;
                    Ok(LuaValue::String(lua.create_string(contents)?))
                }
                "*n" => {
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(LuaValue::Nil);
                    }

                    let trimmed = line.trim();
                    trimmed
                        .parse::<f64>()
                        .map(LuaValue::Number)
                        .map_err(|_| LuaError::RuntimeError("Not a number".to_string()))
                }
                "*l" | "*line" => {
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(LuaValue::Nil);
                    }

                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }

                    Ok(LuaValue::String(lua.create_string(line)?))
                }
                "*L" => {
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(LuaValue::Nil);
                    }

                    Ok(LuaValue::String(lua.create_string(line)?))
                }
                num_str => {
                    let file = this
                        .file
                        .as_mut()
                        .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;
                    let n = num_str
                        .parse::<usize>()
                        .map_err(|_| LuaError::RuntimeError("Invalid byte count".to_string()))?;

                    let mut buffer = vec![0u8; n];
                    let bytes_read = file.read(&mut buffer).map_err(|e| LuaError::external(e))?;

                    buffer.truncate(bytes_read);
                    match String::from_utf8(buffer) {
                        Ok(s) => match lua.create_string(s) {
                            Ok(lua_str) => Ok(LuaValue::String(lua_str)),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(LuaError::RuntimeError(format!("Invalid UTF-8: {}", e))),
                    }
                }
            }
        });

        methods.add_method_mut("write", |_lua, this, data: LuaValue| {
            let file = this
                .file
                .as_mut()
                .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

            let text = match data {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Integer(i) => i.to_string(),
                LuaValue::Number(n) => n.to_string(),
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Can only write strings or numbers".to_string(),
                    ));
                }
            };

            file.write_all(text.as_bytes())
                .map_err(|e| LuaError::external(e))?;

            Ok(())
        });

        methods.add_method_mut("flush", |_lua, this, ()| {
            let file = this
                .file
                .as_mut()
                .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

            file.flush().map_err(|e| LuaError::external(e))?;
            Ok(())
        });

        methods.add_method_mut("close", |_lua, this, ()| {
            if let Some(mut file) = this.file.take() {
                file.flush().map_err(|e| LuaError::external(e))?;
            }
            this.reader.take();
            Ok(())
        });

        methods.add_method("lines", |lua, this, ()| {
            let file = SyncFile::open(
                this.path.to_string_lossy().to_string(),
                Some("r".to_string()),
            )?;
            let file_ud = lua.create_userdata(file)?;

            let registry_key = lua.create_registry_value(file_ud)?;

            let lines_iterator = lua.create_function(move |lua, (): ()| {
                let file_ud: LuaAnyUserData = lua.registry_value(&registry_key)?;

                let read_method: LuaFunction =
                    file_ud.get::<LuaFunction>("read").map_err(|_| {
                        LuaError::RuntimeError("File does not support read".to_string())
                    })?;

                let result = read_method.call::<LuaValue>((file_ud.clone(), "*l"))?;

                match result {
                    LuaValue::String(s) => {
                        let s_str = s.to_str()?;
                        if s_str.is_empty() {
                            Ok(LuaValue::Nil)
                        } else {
                            Ok(LuaValue::String(s))
                        }
                    }
                    LuaValue::Nil => Ok(LuaValue::Nil),
                    _ => Ok(result),
                }
            })?;

            Ok(lines_iterator)
        });

        methods.add_method_mut(
            "seek",
            |_lua, this, (whence, offset): (Option<String>, Option<i64>)| {
                let file = this
                    .file
                    .as_mut()
                    .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

                let whence = whence.unwrap_or_else(|| "cur".to_string());
                let offset = offset.unwrap_or(0);

                let pos = match whence.as_str() {
                    "set" => file
                        .seek(SeekFrom::Start(offset as u64))
                        .map_err(|e| LuaError::external(e))?,
                    "cur" => file
                        .seek(SeekFrom::Current(offset))
                        .map_err(|e| LuaError::external(e))?,
                    "end" => file
                        .seek(SeekFrom::End(offset))
                        .map_err(|e| LuaError::external(e))?,
                    _ => {
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid whence: {}",
                            whence
                        )));
                    }
                };

                Ok(pos)
            },
        );
    }
}

pub fn create_io_module(lua: &Lua) -> LuaResult<LuaTable> {
    let io = lua.create_table()?;

    let stdin = StdinHandle::new();
    let stdout = StdoutHandle::new();
    let stderr = StderrHandle::new();

    let stdin_ud = lua.create_userdata(stdin)?;
    let stdout_ud = lua.create_userdata(stdout)?;
    let stderr_ud = lua.create_userdata(stderr)?;

    CURRENT_INPUT.with(|current| {
        *current.borrow_mut() = Some(stdin_ud.clone());
    });

    CURRENT_OUTPUT.with(|current| {
        *current.borrow_mut() = Some(stdout_ud.clone());
    });

    io.set("stdin", stdin_ud)?;
    io.set("stdout", stdout_ud)?;
    io.set("stderr", stderr_ud)?;

    io.set(
        "open",
        lua.create_function(|_lua, (path, mode): (String, Option<String>)| {
            SyncFile::open(path, mode)
        })?,
    )?;

    io.set(
        "input",
        lua.create_function(|lua, file: Option<LuaValue>| match file {
            Some(LuaValue::UserData(ud)) => {
                let new_input = ud.clone();
                CURRENT_INPUT.with(|current| {
                    *current.borrow_mut() = Some(new_input);
                });
                Ok(LuaValue::UserData(ud))
            }
            Some(LuaValue::String(s)) => {
                let path = s.to_str()?.to_string();
                let file = SyncFile::open(path, Some("r".to_string()))?;
                let ud = lua.create_userdata(file)?;
                CURRENT_INPUT.with(|current| {
                    *current.borrow_mut() = Some(ud.clone());
                });
                Ok(LuaValue::UserData(ud))
            }
            None => {
                let result =
                    CURRENT_INPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));
                match result {
                    Some(ud) => Ok(LuaValue::UserData(ud)),
                    None => Ok(LuaValue::Nil),
                }
            }
            _ => Err(LuaError::RuntimeError("Invalid input file".to_string())),
        })?,
    )?;

    io.set(
        "input",
        lua.create_function(|lua, file: Option<LuaValue>| match file {
            Some(LuaValue::UserData(ud)) => {
                let new_input = ud.clone();
                CURRENT_INPUT.with(|current| {
                    *current.borrow_mut() = Some(new_input);
                });
                Ok(LuaValue::UserData(ud))
            }
            Some(LuaValue::String(s)) => {
                let path = s.to_str()?.to_string();
                let file = SyncFile::open(path, Some("r".to_string()))?;
                let ud = lua.create_userdata(file)?;
                CURRENT_INPUT.with(|current| {
                    *current.borrow_mut() = Some(ud.clone());
                });
                Ok(LuaValue::UserData(ud))
            }
            None => {
                let result =
                    CURRENT_INPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));
                match result {
                    Some(ud) => Ok(LuaValue::UserData(ud)),
                    None => Ok(LuaValue::Nil),
                }
            }
            _ => Err(LuaError::RuntimeError("Invalid input file".to_string())),
        })?,
    )?;

    io.set(
        "output",
        lua.create_function(|lua, file: Option<LuaValue>| match file {
            Some(LuaValue::UserData(ud)) => {
                let new_output = ud.clone();
                CURRENT_OUTPUT.with(|current| {
                    *current.borrow_mut() = Some(new_output);
                });
                Ok(LuaValue::UserData(ud))
            }
            Some(LuaValue::String(s)) => {
                let path = s.to_str()?.to_string();
                let file = SyncFile::open(path, Some("w".to_string()))?;
                let ud = lua.create_userdata(file)?;
                CURRENT_OUTPUT.with(|current| {
                    *current.borrow_mut() = Some(ud.clone());
                });
                Ok(LuaValue::UserData(ud))
            }
            None => {
                let result =
                    CURRENT_OUTPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));
                match result {
                    Some(ud) => Ok(LuaValue::UserData(ud)),
                    None => Ok(LuaValue::Nil),
                }
            }
            _ => Err(LuaError::RuntimeError("Invalid output file".to_string())),
        })?,
    )?;

    io.set(
        "read",
        lua.create_function(|lua, format: Option<LuaValue>| {
            let input_ud =
                CURRENT_INPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));

            let input_ud =
                input_ud.ok_or_else(|| LuaError::RuntimeError("No input file set".to_string()))?;

            let read_method: LuaFunction = input_ud
                .get::<LuaFunction>("read")
                .map_err(|_| LuaError::RuntimeError("File does not support read".to_string()))?;

            match format {
                Some(fmt) => read_method.call::<LuaValue>((input_ud, fmt)),
                None => read_method.call::<LuaValue>((input_ud,)),
            }
        })?,
    )?;

    io.set(
        "write",
        lua.create_function(|lua, values: LuaMultiValue| {
            let output_ud =
                CURRENT_OUTPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));

            let output_ud = output_ud
                .ok_or_else(|| LuaError::RuntimeError("No output file set".to_string()))?;

            let write_method: LuaFunction = output_ud
                .get::<LuaFunction>("write")
                .map_err(|_| LuaError::RuntimeError("File does not support write".to_string()))?;

            for value in values {
                write_method.call::<LuaValue>((output_ud.clone(), value))?;
            }

            Ok(())
        })?,
    )?;

    io.set(
        "flush",
        lua.create_function(|lua, ()| {
            let output_ud =
                CURRENT_OUTPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));

            if let Some(ud) = output_ud {
                let flush_method: LuaFunction = ud.get::<LuaFunction>("flush").map_err(|_| {
                    LuaError::RuntimeError("File does not support flush".to_string())
                })?;
                flush_method.call::<LuaValue>((ud,))?;
            }

            Ok(())
        })?,
    )?;

    io.set(
        "type",
        lua.create_function(|lua, obj: LuaValue| match obj {
            LuaValue::UserData(ud) => {
                if ud.is::<SyncFile>() {
                    let file = ud.borrow::<SyncFile>()?;
                    if file.file.is_some() {
                        return Ok(LuaValue::String(lua.create_string("file")?));
                    } else {
                        return Ok(LuaValue::String(lua.create_string("closed file")?));
                    }
                } else if ud.is::<StdinHandle>()
                    || ud.is::<StdoutHandle>()
                    || ud.is::<StderrHandle>()
                {
                    return Ok(LuaValue::String(lua.create_string("file")?));
                }
                Ok(LuaValue::Nil)
            }
            _ => Ok(LuaValue::Nil),
        })?,
    )?;

    io.set(
        "close",
        lua.create_function(|_lua, file: Option<LuaAnyUserData>| match file {
            Some(ud) => {
                if ud.is::<SyncFile>() {
                    ud.borrow_mut::<SyncFile>()?
                        .file
                        .take()
                        .ok_or_else(|| LuaError::RuntimeError("File already closed".to_string()))?;
                    Ok(())
                } else {
                    Err(LuaError::RuntimeError(
                        "Cannot close standard file handle".to_string(),
                    ))
                }
            }
            None => {
                let input_ud =
                    CURRENT_INPUT.with(|current| current.borrow().as_ref().map(|ud| ud.clone()));

                if let Some(ud) = input_ud {
                    if ud.is::<SyncFile>() {
                        ud.borrow_mut::<SyncFile>()?.file.take().ok_or_else(|| {
                            LuaError::RuntimeError("File already closed".to_string())
                        })?;
                        CURRENT_INPUT.with(|current| {
                            *current.borrow_mut() = None;
                        });
                    }
                }

                Ok(())
            }
        })?,
    )?;

    io.set(
        "lines",
        lua.create_function(|lua, filename: LuaValue| {
            let file = SyncFile::open(filename.to_string()?.to_string(), Some("r".to_string()))?;
            let file_ud = lua.create_userdata(file)?;
            let file_ud_clone = file_ud.clone();

            let lines_iterator = lua.create_function(move |lua, (): ()| {
                let read_method: LuaFunction =
                    file_ud_clone.get::<LuaFunction>("read").map_err(|_| {
                        LuaError::RuntimeError("File does not support read".to_string())
                    })?;

                let result = read_method.call::<LuaValue>((file_ud_clone.clone(), "*l"))?;

                match result {
                    LuaValue::String(s) => {
                        let s_str = s.to_str()?;
                        if s_str.is_empty() {
                            Ok(LuaValue::Nil)
                        } else {
                            Ok(LuaValue::String(s))
                        }
                    }
                    LuaValue::Nil => Ok(LuaValue::Nil),
                    _ => Ok(result),
                }
            })?;

            Ok((lines_iterator, LuaValue::Nil, LuaValue::Nil, file_ud))
        })?,
    )?;

    Ok(io)
}
