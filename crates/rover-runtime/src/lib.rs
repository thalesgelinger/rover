#![allow(dead_code)]

#[cfg(target_os = "android")]
mod android_vulkan;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use mlua::RegistryKey;
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
use ndk::native_window::NativeWindow;
#[cfg(target_os = "android")]
use std::ptr;

pub struct Runtime {
    lua: LuaEngine,
    renderer: SkiaRenderer,
    state: Option<RegistryKey>,
    entry: Option<PathBuf>,
    hits: Vec<rover_render::ActionHit>,
    dirty: bool,
    layer_tree: Option<LayerNode>,
    scale_factor: f32,
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
        })
    }

    pub fn load_entry(&mut self, path: &Path) -> Result<()> {
        self.entry = Some(path.to_path_buf());
        self.state = None;
        self.hits.clear();
        self.dirty = true;
        self.layer_tree = None;
        
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
        let next = self.lua.call_action(action, state)?;
        self.lua.replace_value(state_key, next)?;
        self.dirty = true;
        self.layer_tree = None;
        self.render_view()
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
    if !runtime.runtime.is_dirty() {
        return false;
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
    matches!(result, Ok(true))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rover_app_RoverNative_initVulkan(
    env: JNIEnv,
    _class: JClass,
    entry: JString,
    surface: JObject,
) -> jlong {
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
        return 0;
    }
    let window = match ndk::native_window::NativeWindow::from_surface(&env, surface) {
        Ok(w) => w,
        Err(_) => {
            unsafe { rover_destroy(runtime) };
            return 0;
        }
    };
    let session = match android_vulkan::VulkanSession::new(window) {
        Ok(s) => s,
        Err(_) => {
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
    env: JNIEnv,
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
    arr
}
