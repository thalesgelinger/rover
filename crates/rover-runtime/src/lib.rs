#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use mlua::RegistryKey;
use rover_lua::LuaEngine;
use rover_render::ViewNode;

pub struct Runtime {
    lua: LuaEngine,
    state: Option<RegistryKey>,
    entry: Option<PathBuf>,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        let lua = LuaEngine::new()?;
        Ok(Self {
            lua,
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
pub extern "C" fn rover_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}
