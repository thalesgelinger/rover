use mlua::prelude::*;
use std::path::PathBuf;
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::{BufReader, BufRead, Read, Write, Seek, SeekFrom};

pub struct SyncFile {
    file: Option<File>,
    path: PathBuf,
    _mode: String,
}

impl SyncFile {
    fn open(path: String, mode: Option<String>) -> LuaResult<Self> {
        let mode = mode.unwrap_or_else(|| "r".to_string());
        let path_buf = PathBuf::from(&path);

        let file = match mode.as_str() {
            "r" | "rb" => {
                StdOpenOptions::new()
                    .read(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            "w" | "wb" => {
                StdOpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            "a" | "ab" => {
                StdOpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            "r+" | "rb+" | "r+b" => {
                StdOpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            "w+" | "wb+" | "w+b" => {
                StdOpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            "a+" | "ab+" | "a+b" => {
                StdOpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&path_buf)
                    .map_err(|e| LuaError::external(e))?
            }
            _ => {
                return Err(LuaError::RuntimeError(format!(
                    "Invalid mode: {}",
                    mode
                )))
            }
        };

        Ok(SyncFile {
            file: Some(file),
            path: path_buf,
            _mode: mode,
        })
    }
}

impl LuaUserData for SyncFile {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("read", |_lua, this, format: Option<LuaValue>| {
            let file = this
                .file
                .as_mut()
                .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

            let format_str = match format {
                Some(LuaValue::String(s)) => s.to_str()?.to_string(),
                Some(LuaValue::Integer(n)) => n.to_string(),
                Some(LuaValue::Number(n)) => (n as i64).to_string(),
                None => "*l".to_string(),
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Invalid read format".to_string(),
                    ))
                }
            };

            match format_str.as_str() {
                "*a" | "*all" => {
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)
                        .map_err(|e| LuaError::external(e))?;
                    Ok(contents)
                }
                "*l" | "*line" => {
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(String::new());
                    }

                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }

                    Ok(line)
                }
                "*L" => {
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(String::new());
                    }

                    Ok(line)
                }
                num_str => {
                    let n = num_str
                        .parse::<usize>()
                        .map_err(|_| LuaError::RuntimeError("Invalid byte count".to_string()))?;

                    let mut buffer = vec![0u8; n];
                    let bytes_read = file
                        .read(&mut buffer)
                        .map_err(|e| LuaError::external(e))?;

                    buffer.truncate(bytes_read);
                    String::from_utf8(buffer)
                        .map_err(|e| LuaError::RuntimeError(format!("Invalid UTF-8: {}", e)))
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
                    ))
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
            Ok(())
        });

        methods.add_method("lines", |lua, this, ()| {
            let path = this.path.clone();
            let file = File::open(&path)
                .map_err(|e| LuaError::external(e))?;

            let reader = BufReader::new(file);
            let results = lua.create_table()?;
            let mut index = 1;

            for line in reader.lines() {
                let line = line.map_err(|e| LuaError::external(e))?;
                results.set(index, line)?;
                index += 1;
            }

            Ok(results)
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
                    "set" => {
                        file.seek(SeekFrom::Start(offset as u64))
                            .map_err(|e| LuaError::external(e))?
                    }
                    "cur" => {
                        file.seek(SeekFrom::Current(offset))
                            .map_err(|e| LuaError::external(e))?
                    }
                    "end" => {
                        file.seek(SeekFrom::End(offset))
                            .map_err(|e| LuaError::external(e))?
                    }
                    _ => {
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid whence: {}",
                            whence
                        )))
                    }
                };

                Ok(pos)
            },
        );
    }
}

pub fn create_io_module(lua: &Lua) -> LuaResult<LuaTable> {
    let io = lua.create_table()?;

    io.set(
        "open",
        lua.create_function(|_lua, (path, mode): (String, Option<String>)| {
            SyncFile::open(path, mode)
        })?,
    )?;

    io.set(
        "read",
        lua.create_function(|_lua, _format: Option<String>| {
            Err::<(), _>(LuaError::RuntimeError(
                "io.read is not supported. Use io.open(path):read() instead"
                    .to_string(),
            ))
        })?,
    )?;

    io.set(
        "write",
        lua.create_function(|_lua, _data: LuaValue| {
            Err::<(), _>(LuaError::RuntimeError(
                "io.write is not supported. Use io.open(path):write() instead"
                    .to_string(),
            ))
        })?,
    )?;

    io.set(
        "close",
        lua.create_function(|_lua, file: LuaAnyUserData| {
            file.borrow_mut::<SyncFile>()?
                .file
                .take()
                .ok_or_else(|| LuaError::RuntimeError("File already closed".to_string()))?;
            Ok(())
        })?,
    )?;

    io.set(
        "lines",
        lua.create_function(|lua, filename: String| {
            let file = File::open(&filename)
                .map_err(|e| LuaError::external(e))?;

            let reader = BufReader::new(file);
            let results = lua.create_table()?;
            let mut index = 1;

            for line in reader.lines() {
                let line = line.map_err(|e| LuaError::external(e))?;
                results.set(index, line)?;
                index += 1;
            }

            Ok(results)
        })?,
    )?;

    Ok(io)
}
