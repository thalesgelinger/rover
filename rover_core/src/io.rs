use mlua::prelude::*;
use std::path::PathBuf;
use tokio::fs::{File as TokioFile, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};

/// Async file handle that overrides Lua's native io
pub struct AsyncFile {
    file: Option<TokioFile>,
    path: PathBuf,
    mode: String,
}

impl AsyncFile {
    async fn open(path: String, mode: Option<String>) -> LuaResult<Self> {
        let mode = mode.unwrap_or_else(|| "r".to_string());
        let path_buf = PathBuf::from(&path);

        let file = match mode.as_str() {
            "r" | "rb" => {
                // Read mode
                OpenOptions::new()
                    .read(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            "w" | "wb" => {
                // Write mode (truncate)
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            "a" | "ab" => {
                // Append mode
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            "r+" | "rb+" | "r+b" => {
                // Read and write mode
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            "w+" | "wb+" | "w+b" => {
                // Read and write mode (truncate)
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            "a+" | "ab+" | "a+b" => {
                // Read and append mode
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&path_buf)
                    .await
                    .map_err(|e| LuaError::external(e))?
            }
            _ => {
                return Err(LuaError::RuntimeError(format!(
                    "Invalid mode: {}",
                    mode
                )))
            }
        };

        Ok(AsyncFile {
            file: Some(file),
            path: path_buf,
            mode,
        })
    }
}

impl LuaUserData for AsyncFile {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // file:read([format])
        // Formats: "*a" (all), "*l" (line), "*L" (line with newline), number (bytes)
        methods.add_async_method_mut("read", |_lua, mut this, format: Option<LuaValue>| async move {
            let file = this
                .file
                .as_mut()
                .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

            let format_str = match format {
                Some(LuaValue::String(s)) => s.to_str()?.to_string(),
                Some(LuaValue::Integer(n)) => n.to_string(),
                Some(LuaValue::Number(n)) => (n as i64).to_string(),
                None => "*l".to_string(), // Default: read line
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Invalid read format".to_string(),
                    ))
                }
            };

            match format_str.as_str() {
                "*a" | "*all" => {
                    // Read entire file
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)
                        .await
                        .map_err(|e| LuaError::external(e))?;
                    Ok(contents)
                }
                "*l" | "*line" => {
                    // Read line without newline
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .await
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(String::new()); // EOF
                    }

                    // Remove trailing newline
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }

                    Ok(line)
                }
                "*L" => {
                    // Read line with newline
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    let bytes_read = reader
                        .read_line(&mut line)
                        .await
                        .map_err(|e| LuaError::external(e))?;

                    if bytes_read == 0 {
                        return Ok(String::new()); // EOF
                    }

                    Ok(line)
                }
                num_str => {
                    // Read N bytes
                    let n = num_str
                        .parse::<usize>()
                        .map_err(|_| LuaError::RuntimeError("Invalid byte count".to_string()))?;

                    let mut buffer = vec![0u8; n];
                    let bytes_read = file
                        .read(&mut buffer)
                        .await
                        .map_err(|e| LuaError::external(e))?;

                    buffer.truncate(bytes_read);
                    String::from_utf8(buffer)
                        .map_err(|e| LuaError::RuntimeError(format!("Invalid UTF-8: {}", e)))
                }
            }
        });

        // file:write(...)
        methods.add_async_method_mut("write", |_lua, mut this, data: LuaValue| async move {
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
                .await
                .map_err(|e| LuaError::external(e))?;

            Ok(())
        });

        // file:flush()
        methods.add_async_method_mut("flush", |_lua, mut this, ()| async move {
            let file = this
                .file
                .as_mut()
                .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

            file.flush().await.map_err(|e| LuaError::external(e))?;
            Ok(())
        });

        // file:close()
        methods.add_async_method_mut("close", |_lua, mut this, ()| async move {
            if let Some(mut file) = this.file.take() {
                file.flush().await.map_err(|e| LuaError::external(e))?;
                // File is automatically closed when dropped
            }
            Ok(())
        });

        // file:lines()
        methods.add_async_method("lines", |lua, this, ()| async move {
            let path = this.path.clone();
            let file = TokioFile::open(&path)
                .await
                .map_err(|e| LuaError::external(e))?;

            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let results = lua.create_table()?;
            let mut index = 1;

            while let Some(line) = lines.next_line().await.map_err(|e| LuaError::external(e))? {
                results.set(index, line)?;
                index += 1;
            }

            Ok(results)
        });

        // file:seek(whence, offset)
        methods.add_async_method_mut(
            "seek",
            |_lua, mut this, (whence, offset): (Option<String>, Option<i64>)| async move {
                let file = this
                    .file
                    .as_mut()
                    .ok_or_else(|| LuaError::RuntimeError("File is closed".to_string()))?;

                let whence = whence.unwrap_or_else(|| "cur".to_string());
                let offset = offset.unwrap_or(0);

                let pos = match whence.as_str() {
                    "set" => {
                        file.seek(std::io::SeekFrom::Start(offset as u64))
                            .await
                            .map_err(|e| LuaError::external(e))?
                    }
                    "cur" => {
                        file.seek(std::io::SeekFrom::Current(offset))
                            .await
                            .map_err(|e| LuaError::external(e))?
                    }
                    "end" => {
                        file.seek(std::io::SeekFrom::End(offset))
                            .await
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

/// Create the async io module that overrides Lua's native io
pub fn create_io_module(lua: &Lua) -> LuaResult<LuaTable> {
    let io = lua.create_table()?;

    // io.open(filename, mode)
    io.set(
        "open",
        lua.create_async_function(|_lua, (path, mode): (String, Option<String>)| async move {
            AsyncFile::open(path, mode).await
        })?,
    )?;

    // io.read(...) - reads from stdin (not implemented for async, would need special handling)
    io.set(
        "read",
        lua.create_function(|_lua, _format: Option<String>| {
            Err::<(), _>(LuaError::RuntimeError(
                "io.read is not supported in async mode. Use io.open(path):read() instead"
                    .to_string(),
            ))
        })?,
    )?;

    // io.write(...) - writes to stdout (not implemented for async, would need special handling)
    io.set(
        "write",
        lua.create_function(|_lua, _data: LuaValue| {
            Err::<(), _>(LuaError::RuntimeError(
                "io.write is not supported in async mode. Use io.open(path):write() instead"
                    .to_string(),
            ))
        })?,
    )?;

    // io.close(file) - helper to close file
    io.set(
        "close",
        lua.create_async_function(|_lua, file: LuaAnyUserData| async move {
            file.borrow_mut::<AsyncFile>()?
                .file
                .take()
                .ok_or_else(|| LuaError::RuntimeError("File already closed".to_string()))?;
            Ok(())
        })?,
    )?;

    // io.lines(filename)
    io.set(
        "lines",
        lua.create_async_function(|lua, filename: String| async move {
            let file = TokioFile::open(&filename)
                .await
                .map_err(|e| LuaError::external(e))?;

            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let results = lua.create_table()?;
            let mut index = 1;

            while let Some(line) = lines.next_line().await.map_err(|e| LuaError::external(e))? {
                results.set(index, line)?;
                index += 1;
            }

            Ok(results)
        })?,
    )?;

    Ok(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let path_str = file_path.to_str().unwrap().to_string();

        // Write to file
        let mut file = AsyncFile::open(path_str.clone(), Some("w".to_string()))
            .await
            .unwrap();

        let _lua = Lua::new();

        // Simulate write
        if let Some(ref mut f) = file.file {
            f.write_all(b"Hello, async I/O!")
                .await
                .unwrap();
            f.flush().await.unwrap();
        }

        // Close file
        file.file.take();

        // Read from file
        let mut file = AsyncFile::open(path_str, Some("r".to_string()))
            .await
            .unwrap();

        if let Some(ref mut f) = file.file {
            let mut contents = String::new();
            f.read_to_string(&mut contents).await.unwrap();
            assert_eq!(contents, "Hello, async I/O!");
        }
    }

    #[tokio::test]
    async fn test_file_modes() {
        let temp_dir = TempDir::new().unwrap();

        // Test write mode
        let path = temp_dir.path().join("write_test.txt");
        let mut file = AsyncFile::open(
            path.to_str().unwrap().to_string(),
            Some("w".to_string()),
        )
        .await
        .unwrap();

        if let Some(ref mut f) = file.file {
            f.write_all(b"test content").await.unwrap();
            f.flush().await.unwrap();
        }
        file.file.take();

        // Verify file was written
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "test content");

        // Test append mode
        let mut file = AsyncFile::open(
            path.to_str().unwrap().to_string(),
            Some("a".to_string()),
        )
        .await
        .unwrap();

        if let Some(ref mut f) = file.file {
            f.write_all(b" appended").await.unwrap();
            f.flush().await.unwrap();
        }
        file.file.take();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "test content appended");
    }

    #[tokio::test]
    async fn test_read_modes() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("read_test.txt");

        // Create test file
        fs::write(&path, "line1\nline2\nline3").unwrap();

        // Test read all
        let mut file = AsyncFile::open(
            path.to_str().unwrap().to_string(),
            Some("r".to_string()),
        )
        .await
        .unwrap();

        if let Some(ref mut f) = file.file {
            let mut contents = String::new();
            f.read_to_string(&mut contents).await.unwrap();
            assert_eq!(contents, "line1\nline2\nline3");
        }
    }

    #[tokio::test]
    async fn test_seek() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("seek_test.txt");

        // Create test file
        fs::write(&path, "0123456789").unwrap();

        let mut file = AsyncFile::open(
            path.to_str().unwrap().to_string(),
            Some("r".to_string()),
        )
        .await
        .unwrap();

        if let Some(ref mut f) = file.file {
            // Seek to position 5
            let pos = f.seek(std::io::SeekFrom::Start(5)).await.unwrap();
            assert_eq!(pos, 5);

            // Read from position 5
            let mut buffer = [0u8; 5];
            f.read_exact(&mut buffer).await.unwrap();
            assert_eq!(&buffer, b"56789");
        }
    }
}
