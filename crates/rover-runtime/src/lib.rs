#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use mlua::RegistryKey;
use rover_lua::LuaEngine;
use rover_render::{SkiaRenderer, ViewNode};

pub struct Runtime {
    lua: LuaEngine,
    renderer: SkiaRenderer,
    state: Option<RegistryKey>,
    entry: Option<PathBuf>,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        let lua = LuaEngine::new()?;
        let renderer = SkiaRenderer::new();
        Ok(Self {
            lua,
            renderer,
            state: None,
            entry: None,
        })
    }

    pub fn load_entry(&mut self, path: &Path) -> Result<()> {
        self.entry = Some(path.to_path_buf());
        self.state = None;
        self.lua.load_app(path)
    }

    pub fn init_state(&mut self) -> Result<()> {
        let state = self.lua.init_state()?;
        let key = self.lua.store_value(state)?;
        self.state = Some(key);
        Ok(())
    }

    pub fn ensure_state(&mut self) -> Result<()> {
        if self.state.is_none() {
            self.init_state()?;
        }
        Ok(())
    }

    pub fn render_view(&self) -> Result<ViewNode> {
        let state_key = self
            .state
            .as_ref()
            .ok_or_else(|| anyhow!("state not initialized"))?;
        let state = self.lua.load_value(state_key)?;
        let view = self.lua.render(state)?;
        let debug = format!("{view:?}");
        ViewNode::from_value(&view).with_context(|| format!("render value {debug}"))
    }

    pub fn render_png(&self, width: i32, height: i32) -> Result<rover_render::RenderResult> {
        let view = self.render_view()?;
        self.renderer.render(&view, width, height)
    }

    pub fn render_or_init(&mut self) -> Result<ViewNode> {
        self.ensure_state()?;
        self.render_view()
    }

    pub fn dispatch_action(&mut self, action: &str) -> Result<ViewNode> {
        self.ensure_state()?;
        let state_key = self
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("state not initialized"))?;
        let state = self.lua.load_value(state_key)?;
        let next = self.lua.call_action(action, state)?;
        self.lua.replace_value(state_key, next)?;
        self.render_view()
    }

    pub fn entry(&self) -> Option<&PathBuf> {
        self.entry.as_ref()
    }
}

pub struct RuntimeHandle {
    runtime: Runtime,
}

fn runtime_from_entry_dir(root: &Path) -> Result<Runtime> {
    let entry = root.join("main.lua");
    if !entry.exists() {
        return Err(anyhow!("entry missing at {}", entry.display()));
    }
    let mut runtime = Runtime::new()?;
    runtime
        .load_entry(&entry)
        .with_context(|| format!("load {}", entry.display()))?;
    runtime.ensure_state()?;
    Ok(runtime)
}

fn encode_view(view: ViewNode) -> Result<CString> {
    let json = serde_json::to_string(&view)?;
    Ok(CString::new(json)?)
}

fn encode_hits(json: String) -> Result<CString> {
    Ok(CString::new(json)?)
}

fn ptr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string()) }
}

#[no_mangle]
pub extern "C" fn rover_create(root: *const c_char) -> *mut RuntimeHandle {
    let root = match ptr_to_string(root) {
        Some(p) => PathBuf::from(p),
        None => return std::ptr::null_mut(),
    };

    match runtime_from_entry_dir(&root) {
        Ok(runtime) => Box::into_raw(Box::new(RuntimeHandle { runtime })),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rover_destroy(handle: *mut RuntimeHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

#[repr(C)]
pub struct RoverImage {
    pub data: *mut u8,
    pub len: usize,
    pub width: i32,
    pub height: i32,
    pub row_bytes: usize,
    pub hits_json: *mut c_char,
}


#[no_mangle]
pub extern "C" fn rover_render_json(handle: *mut RuntimeHandle) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let runtime = unsafe { &mut *handle };
    match runtime
        .runtime
        .render_or_init()
        .and_then(encode_view)
    {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rover_dispatch_action_json(
    handle: *mut RuntimeHandle,
    action: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let action = match ptr_to_string(action) {
        Some(a) => a,
        None => return std::ptr::null_mut(),
    };
    let runtime = unsafe { &mut *handle };
    match runtime
        .runtime
        .dispatch_action(&action)
        .and_then(encode_view)
    {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rover_render_rgba(
    handle: *mut RuntimeHandle,
    width: i32,
    height: i32,
) -> RoverImage {
    if handle.is_null() {
        return RoverImage {
            data: std::ptr::null_mut(),
            len: 0,
            width: 0,
            height: 0,
            row_bytes: 0,
            hits_json: std::ptr::null_mut(),
        };
    }
    let runtime = unsafe { &mut *handle };
    match runtime.runtime.render_png(width, height) {
        Ok(out) => {
            let len = out.buffer.len();
            let mut buf = out.buffer;
            let data = buf.as_mut_ptr();
            std::mem::forget(buf);
            let hits_json = encode_hits(out.hits_json)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut());
            RoverImage {
                data,
                len,
                width: out.width,
                height: out.height,
                row_bytes: out.row_bytes,
                hits_json,
            }
        }
        Err(_) => RoverImage {
            data: std::ptr::null_mut(),
            len: 0,
            width: 0,
            height: 0,
            row_bytes: 0,
            hits_json: std::ptr::null_mut(),
        },
    }
}

#[no_mangle]
pub extern "C" fn rover_image_free(img: RoverImage) {
    if !img.data.is_null() && img.len > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(img.data, img.len, img.len);
        }
    }
    if !img.hits_json.is_null() {
        rover_string_free(img.hits_json);
    }
}

#[no_mangle]
pub extern "C" fn rover_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}
