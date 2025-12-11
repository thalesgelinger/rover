#![allow(dead_code)]

#[cfg(target_os = "android")]
mod android_vulkan;

#[cfg(target_os = "ios")]
#[link(name = "Foundation", kind = "framework")]
extern "C" {}

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use mlua::RegistryKey;
use rover_devserver::{read_config, DevClient, DEFAULT_PORT};
use rover_lua::LuaEngine;
use rover_render::{LayerNode, RenderSurface, SkiaRenderer, ViewNode};
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::ffi::c_void;
#[cfg(target_os = "android")]
use jni::objects::{JClass, JObject, JString};
#[cfg(target_os = "android")]
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jlong};
#[cfg(target_os = "android")]
use jni::JNIEnv;
#[cfg(target_os = "android")]
use ndk_sys;

#[cfg(target_os = "android")]
fn log_android(tag: &str, msg: &str) {
    if let (Ok(tag), Ok(msg)) = (CString::new(tag), CString::new(msg)) {
        unsafe {
            ndk_sys::__android_log_write(
                6, // ANDROID_LOG_ERROR
                tag.as_ptr(),
                msg.as_ptr(),
            );
        }
    }
}

use std::collections::HashMap;

pub struct Runtime {
    lua: LuaEngine,
    renderer: SkiaRenderer,
    state: Option<RegistryKey>,
    entry: Option<PathBuf>,
    hits: Vec<rover_render::ActionHit>,
    dirty: bool,
    layer_tree: Option<LayerNode>,
    scale_factor: f32,
    dev_client: Option<DevClient>,
    dev_host: String,
    dev_port: u16,
    is_reloading: bool,
    scroll_offsets: HashMap<usize, f32>,
    last_touch_y: Option<f32>,
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
            hits: Vec::new(),
            dirty: true,
            layer_tree: None,
            scale_factor: 1.0,
            dev_client: None,
            dev_host: {
                #[cfg(target_os = "android")]
                { "10.0.2.2".to_string() }
                #[cfg(not(target_os = "android"))]
                { "127.0.0.1".to_string() }
            },
            dev_port: DEFAULT_PORT,
            is_reloading: false,
            scroll_offsets: HashMap::new(),
            last_touch_y: None,
        })
    }

    pub fn enable_hot_reload(&mut self) -> Result<()> {
        match DevClient::connect(self.dev_host.clone(), self.dev_port) {
            Ok(client) => {
                self.dev_client = Some(client);
                Ok(())
            }
            Err(e) => {
                eprintln!("[runtime] devserver connection failed: {e}");
                eprintln!("[runtime] hot reload disabled - is devserver running?");
                Ok(())
            }
        }
    }

    pub fn check_and_reload(&mut self) -> Result<bool> {
        let should_reload = {
            let Some(client) = &mut self.dev_client else {
                return Ok(false);
            };
            client.check_reload()?
        };

        if !should_reload {
            return Ok(false);
        }

        self.is_reloading = true;
        println!("[runtime] reloading lua with state preservation...");
        
        // Get synced files and write to cache
        if let Some(client) = &mut self.dev_client {
            if let Some(files) = client.take_sync() {
                if let Some(entry) = &self.entry {
                    let root = entry.parent().unwrap_or(entry.as_path());
                    let cache_dir = root.join("rover-dev");
                    std::fs::create_dir_all(&cache_dir).ok();
                    
                    for (rel_path, content) in files {
                        let file_path = cache_dir.join(&rel_path);
                        if let Some(parent) = file_path.parent() {
                            std::fs::create_dir_all(parent).ok();
                        }
                        std::fs::write(&file_path, content).ok();
                    }
                }
            }
        }
        
        // Preserve current state by keeping its registry key
        let old_state_key = self.state.take();

        // Reload lua (creates new engine)
        if let Some(entry) = self.entry.clone() {
            // Update entry to rover-dev if it exists
            let root = entry.parent().unwrap_or(entry.as_path());
            let dev_entry = root.join("rover-dev/main.lua");
            let reload_entry = if dev_entry.exists() {
                dev_entry
            } else {
                entry.clone()
            };
            
            self.lua = LuaEngine::new()?;
            self.lua.load_app(&reload_entry)?;
            self.entry = Some(reload_entry);
            
            // Try to restore state, fallback to init
            if old_state_key.is_some() {
                // State was preserved but in old registry, init fresh state
                self.init_state()?;
            } else {
                self.init_state()?;
            }
            
            // Mark dirty to trigger re-render
            self.dirty = true;
            self.layer_tree = None;
            
            // Send ack and clear reload flag after completion
            if let Some(client) = &mut self.dev_client {
                client.ack_reload().ok();
            }
            self.is_reloading = false;
            
            println!("[runtime] reload complete");
            Ok(true)
        } else {
            self.is_reloading = false;
            Ok(false)
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.is_reloading
    }

    pub fn load_entry(&mut self, path: &Path) -> Result<()> {
        self.entry = Some(path.to_path_buf());
        self.state = None;
        self.hits.clear();
        self.dirty = true;
        self.layer_tree = None;

        // Load dev config
        if let Some(root) = path.parent() {
            if let Some(cfg) = read_config(root) {
                self.dev_host = cfg.host;
                self.dev_port = cfg.port;
            } else {
                self.dev_host = "127.0.0.1".to_string();
                self.dev_port = DEFAULT_PORT;
            }
        }
        
        // Load custom fonts if available
        if let Some(root) = path.parent() {
            let fonts_dir = root.join("assets").join("fonts");
            self.renderer.load_custom_fonts(&fonts_dir).ok();
        }
        
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

    pub fn render_png(&mut self, width: i32, height: i32) -> Result<rover_render::RenderResult> {
        self.ensure_state()?;
        let view = self.render_view()?;
        let result = self.renderer.render_rgba(&view, width, height)?;
        self.hits = result.hits.clone();
        self.dirty = false;
        Ok(result)
    }

    pub fn render_into_surface(&mut self, surface: &mut RenderSurface) -> Result<()> {
        self.ensure_state()?;
        
        if self.layer_tree.is_none() || self.dirty {
            let view = self.render_view()?;
            let (width, height) = surface.size();
            let bounds = skia_safe::Rect::from_xywh(0.0, 0.0, width as f32, height as f32);
            self.layer_tree = Some(self.renderer.build_layer_tree(&view, bounds)?);
        }
        
        if let Some(ref layer) = self.layer_tree {
            let result = self.renderer.render_layer_tree(layer, surface)?;
            self.hits = result.hits;
        }
        
        self.dirty = false;
        Ok(())
    }

    pub fn render_if_dirty(&mut self, surface: &mut RenderSurface) -> Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        self.render_into_surface(surface)?;
        Ok(true)
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
        
        // Parse action: either plain string or JSON {"action":"name","param":value}
        let (action_name, param_json) = if action.starts_with('{') {
            // JSON format: parse action and param
            let parsed: serde_json::Value = serde_json::from_str(action)
                .with_context(|| format!("parse action JSON: {action}"))?;
            let action_name = parsed["action"]
                .as_str()
                .ok_or_else(|| anyhow!("action missing in JSON"))?
                .to_string();
            let param = parsed.get("param").cloned();
            (action_name, param)
        } else {
            // Plain string action
            (action.to_string(), None)
        };
        
        // Convert param JSON to Lua if present
        let param = if let Some(p) = param_json {
            Some(self.lua.json_to_lua_value(&p)?)
        } else {
            None
        };
        
        let next = self.lua.call_action(&action_name, state, param)?;
        self.lua.replace_value(state_key, next)?;
        self.dirty = true;
        self.layer_tree = None;
        self.render_view()
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) {
        self.last_touch_y = Some(y);
    }
    
    pub fn pointer_move(&mut self, _x: f32, y: f32) {
        if let Some(last_y) = self.last_touch_y {
            let delta = y - last_y;
            // Only scroll if movement > threshold (avoid accidental scrolls)
            if delta.abs() > 3.0 {
                // Find scroll_area in layer tree and update offset
                if let Some(tree) = self.layer_tree.as_mut() {
                    Self::update_scroll_offset_recursive(tree, delta);
                }
                self.dirty = true;
                self.last_touch_y = Some(y);
            }
        }
    }
    
    pub fn pointer_up(&mut self, _x: f32, y: f32) {
        // Check if this was a tap (minimal movement) vs scroll
        if let Some(last_y) = self.last_touch_y {
            let delta = (y - last_y).abs();
            if delta < 10.0 {
                // This was a tap, not a scroll
                if let Some(hit) = self
                    .hits
                    .iter()
                    .find(|h| _x >= h.x && _x <= h.x + h.w && y >= h.y && y <= h.y + h.h)
                {
                    let action = hit.action.clone();
                    let _ = self.dispatch_action(&action);
                }
            }
        }
        self.last_touch_y = None;
    }

    pub fn pointer_tap(&mut self, x: f32, y: f32) -> Result<bool> {
        if let Some(hit) = self
            .hits
            .iter()
            .find(|h| x >= h.x && x <= h.x + h.w && y >= h.y && y <= h.y + h.h)
        {
            let action = hit.action.clone();
            self.dispatch_action(&action)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    fn update_scroll_offset_recursive(node: &mut LayerNode, delta: f32) {
        if node.kind == "scroll_area" {
            // Apply scroll delta, clamp to valid range
            let new_offset = (node.scroll_offset - delta).max(0.0);
            // TODO: clamp to max content height
            node.scroll_offset = new_offset;
        }
        for child in &mut node.children {
            Self::update_scroll_offset_recursive(child, delta);
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn entry(&self) -> Option<&PathBuf> {
        self.entry.as_ref()
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        if (self.scale_factor - scale).abs() > 0.01 {
            self.scale_factor = scale;
            self.renderer.set_scale_factor(scale);
            self.dirty = true;
            self.layer_tree = None;
        }
    }
}

pub struct RuntimeHandle {
    runtime: Runtime,
}

#[cfg(target_os = "android")]
struct AndroidVulkanState {
    runtime: *mut RuntimeHandle,
    session: android_vulkan::VulkanSession,
}

fn runtime_from_entry_dir(root: &Path) -> Result<Runtime> {
    // Prefer hot-reloaded cache over bundled files
    let dev_entry = root.join("rover-dev/main.lua");
    let entry = if dev_entry.exists() {
        println!("[runtime] loading from rover-dev cache");
        dev_entry
    } else {
        root.join("main.lua")
    };
    
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
        Err(err) => {
            #[cfg(target_os = "android")]
            log_android("Rover", &format!("runtime_from_entry_dir failed: {err}"));
            let _ = err;
            std::ptr::null_mut()
        }
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
pub extern "C" fn rover_enable_hot_reload(handle: *mut RuntimeHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.enable_hot_reload().is_ok()
}

#[no_mangle]
pub extern "C" fn rover_is_reloading(handle: *mut RuntimeHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let runtime = unsafe { &*handle };
    runtime.runtime.is_reloading()
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
    match runtime.runtime.render_or_init().and_then(encode_view) {
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

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn rover_render_metal(
    handle: *mut RuntimeHandle,
    device: *mut c_void,
    queue: *mut c_void,
    texture: *mut c_void,
    width: i32,
    height: i32,
    scale: f32,
) -> bool {
    if handle.is_null() {
        return false;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.set_scale_factor(scale);
    
    // Check for hot reload
    if runtime.runtime.check_and_reload().unwrap_or(false) {
        runtime.runtime.mark_dirty();
    }
    
    if !runtime.runtime.is_dirty() {
        return false;
    }
    let result = runtime.runtime.ensure_state().and_then(|_| {
        let mut surface = unsafe { RenderSurface::metal(device, queue, texture, width, height)? };
        runtime.runtime.render_if_dirty(&mut surface)
    });
    matches!(result, Ok(true))
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn rover_render_vulkan(
    handle: *mut RuntimeHandle,
    instance: *const c_void,
    physical_device: *const c_void,
    device: *const c_void,
    queue: *const c_void,
    queue_family_index: u32,
    image: *const c_void,
    image_format: u32,
    image_layout: u32,
    image_usage_flags: u32,
    width: i32,
    height: i32,
    sample_count: i32,
    scale: f32,
    vk_get_instance_proc_addr: unsafe extern "system" fn(*const c_void, *const c_char) -> *const c_void,
    vk_get_device_proc_addr: unsafe extern "system" fn(*const c_void, *const c_char) -> *const c_void,
) -> bool {
    if handle.is_null() {
        return false;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.set_scale_factor(scale);
    log_android("Rover", "render_vulkan start");
    
    // Check for hot reload
    if runtime.runtime.check_and_reload().unwrap_or(false) {
        log_android("Rover", "hot reload triggered");
        runtime.runtime.mark_dirty();
    }
    
    if !runtime.runtime.is_dirty() {
        log_android("Rover", "render_vulkan skipped: not dirty; forcing");
        runtime.runtime.mark_dirty();
    }
    let result = runtime.runtime.ensure_state().and_then(|_| {
        let mut surface = unsafe {
            RenderSurface::vulkan(
                instance,
                physical_device,
                device,
                queue,
                queue_family_index,
                image,
                image_format,
                image_layout,
                image_usage_flags,
                width,
                height,
                sample_count,
                vk_get_instance_proc_addr,
                vk_get_device_proc_addr,
            )?
        };
        runtime.runtime.render_if_dirty(&mut surface)
    });
    match result {
        Ok(true) => {
            log_android("Rover", "render_vulkan ok: rendered");
            true
        }
        Ok(false) => {
            log_android("Rover", "render_vulkan finished: not dirty");
            false
        }
        Err(err) => {
            log_android("Rover", &format!("render_vulkan failed: {err}"));
            false
        }
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_initVulkan(
    mut env: JNIEnv,
    _class: JClass,
    entry: JString,
    surface: JObject,
    scale: jfloat,
) -> jlong {
    log_android("Rover", "initVulkan enter");
    log_android("Rover", "initVulkan enter");
    let path: String = match env.get_string(&entry) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return 0,
    };
    let c_path = match CString::new(path) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let runtime = rover_create(c_path.as_ptr());
    if runtime.is_null() {
        log_android("Rover", "rover_create returned null");
        return 0;
    }
    let dirty = unsafe { (&*runtime).runtime.is_dirty() };
    log_android("Rover", &format!("runtime dirty after create={dirty}"));
    let raw_env = env.get_native_interface();
    let raw_surface = surface.into_raw();
    let window = match unsafe { ndk::native_window::NativeWindow::from_surface(raw_env, raw_surface) } {
        Some(w) => w,
        None => {
            log_android("Rover", "NativeWindow::from_surface returned null");
            unsafe { rover_destroy(runtime) };
            return 0;
        }
    };
    let session = match android_vulkan::VulkanSession::new(window, scale as f32) {
        Ok(s) => s,
        Err(err) => {
            log_android("Rover", &format!("VulkanSession::new failed: {err}"));
            unsafe { rover_destroy(runtime) };
            return 0;
        }
    };
    let state = AndroidVulkanState { runtime, session };
    Box::into_raw(Box::new(state)) as jlong
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_renderVulkan(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let state = unsafe { &mut *(handle as *mut AndroidVulkanState) };
    let rendered = state.session.render_rgba(state.runtime).unwrap_or(false);
    if rendered { 1 } else { 0 }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_surfaceChanged(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    width: jint,
    height: jint,
) {
    if handle == 0 {
        return;
    }
    let state = unsafe { &mut *(handle as *mut AndroidVulkanState) };
    state
        .session
        .request_resize(width as u32, height as u32);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_pointerTap(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let state = unsafe { &mut *(handle as *mut AndroidVulkanState) };
    if rover_pointer_tap(state.runtime, x, y) {
        1
    } else {
        0
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_destroyVulkan(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    unsafe {
        let state = Box::from_raw(handle as *mut AndroidVulkanState);
        rover_destroy(state.runtime);
        // session drops with Box
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_enableHotReload(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let state = unsafe { &mut *(handle as *mut AndroidVulkanState) };
    if rover_enable_hot_reload(state.runtime) {
        1
    } else {
        0
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_isReloading(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let state = unsafe { &*(handle as *mut AndroidVulkanState) };
    if rover_is_reloading(state.runtime) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn rover_pointer_down(handle: *mut RuntimeHandle, x: f32, y: f32) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.pointer_down(x, y);
}

#[no_mangle]
pub extern "C" fn rover_pointer_move(handle: *mut RuntimeHandle, x: f32, y: f32) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.pointer_move(x, y);
}

#[no_mangle]
pub extern "C" fn rover_pointer_up(handle: *mut RuntimeHandle, x: f32, y: f32) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.pointer_up(x, y);
}

#[no_mangle]
pub extern "C" fn rover_pointer_tap(handle: *mut RuntimeHandle, x: f32, y: f32) -> bool {
    if handle.is_null() {
        return false;
    }
    let runtime = unsafe { &mut *handle };
    runtime.runtime.pointer_tap(x, y).unwrap_or(false)
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

// JNI bridge (Android only)
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_init(
    mut env: JNIEnv,
    _class: JClass,
    entry: JString,
) -> jlong {

    let path: String = match env.get_string(&entry) {

        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return 0,
    };
    let c_path = CString::new(path).unwrap_or_default();
    let handle = rover_create(c_path.as_ptr());
    handle as jlong
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_destroy(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    rover_destroy(handle as *mut RuntimeHandle);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_renderRgba(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    width: jint,
    height: jint,
) -> jbyteArray {
    let img = rover_render_rgba(handle as *mut RuntimeHandle, width, height);
    if img.data.is_null() || img.len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(img.data, img.len) };
    let arr = env
        .byte_array_from_slice(slice)
        .unwrap_or_else(|_| env.new_byte_array(0).unwrap());
    rover_image_free(img);
    arr.into_raw()
}
