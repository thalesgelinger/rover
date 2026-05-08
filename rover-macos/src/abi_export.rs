use crate::abi::{
    AppendChildFn, CreateViewFn, HostCallbacks, RemoveViewFn, SetBoolFn, SetFrameFn, SetStyleFn,
    SetTextFn, SetWindowFn, StopAppFn,
};
use crate::runtime::MacosRuntime;
use std::ffi::{CStr, CString, c_char};

pub struct FfiRuntime {
    runtime: MacosRuntime,
    last_error: CString,
}

impl FfiRuntime {
    fn new(callbacks: HostCallbacks) -> Result<Self, String> {
        Ok(Self {
            runtime: MacosRuntime::new(callbacks).map_err(|e| e.to_string())?,
            last_error: CString::new("").expect("empty string has no nul"),
        })
    }

    fn clear_error(&mut self) {
        self.last_error = CString::new("").expect("empty string has no nul");
    }

    fn set_error(&mut self, error: impl AsRef<str>) {
        self.last_error = CString::new(error.as_ref().replace('\0', ""))
            .expect("nul bytes removed before CString");
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_init(callbacks: HostCallbacks) -> *mut FfiRuntime {
    match FfiRuntime::new(callbacks) {
        Ok(runtime) => Box::into_raw(Box::new(runtime)),
        Err(err) => {
            eprintln!("{err}");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_init_with_callbacks(
    create_view: Option<CreateViewFn>,
    append_child: Option<AppendChildFn>,
    remove_view: Option<RemoveViewFn>,
    set_frame: Option<SetFrameFn>,
    set_text: Option<SetTextFn>,
    set_bool: Option<SetBoolFn>,
    set_style: Option<SetStyleFn>,
    set_window: Option<SetWindowFn>,
    stop_app: Option<StopAppFn>,
) -> *mut FfiRuntime {
    unsafe {
        rover_macos_init(HostCallbacks {
            create_view,
            append_child,
            remove_view,
            set_frame,
            set_text,
            set_bool,
            set_style,
            set_window,
            stop_app,
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_free(runtime: *mut FfiRuntime) {
    if !runtime.is_null() {
        let _ = unsafe { Box::from_raw(runtime) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_load_lua(
    runtime: *mut FfiRuntime,
    source: *const c_char,
) -> i32 {
    if runtime.is_null() || source.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    let source = unsafe { CStr::from_ptr(source) }.to_string_lossy();
    match runtime.runtime.load_lua(source.as_ref()) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.to_string());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_tick(runtime: *mut FfiRuntime) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    match runtime.runtime.tick() {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.to_string());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_next_wake_ms(runtime: *mut FfiRuntime) -> i32 {
    if runtime.is_null() {
        return -1;
    }

    unsafe { &*runtime }.runtime.next_wake_ms()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_dispatch_click(runtime: *mut FfiRuntime, id: u32) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    unsafe { &mut *runtime }.runtime.dispatch_click(id);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_dispatch_input(
    runtime: *mut FfiRuntime,
    id: u32,
    value: *const c_char,
) -> i32 {
    if runtime.is_null() || value.is_null() {
        return 1;
    }
    let value = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .to_string();
    unsafe { &mut *runtime }.runtime.dispatch_input(id, value);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_dispatch_submit(
    runtime: *mut FfiRuntime,
    id: u32,
    value: *const c_char,
) -> i32 {
    if runtime.is_null() || value.is_null() {
        return 1;
    }
    let value = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .to_string();
    unsafe { &mut *runtime }.runtime.dispatch_submit(id, value);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_dispatch_toggle(
    runtime: *mut FfiRuntime,
    id: u32,
    checked: bool,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    unsafe { &mut *runtime }
        .runtime
        .dispatch_toggle(id, checked);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_set_viewport(
    runtime: *mut FfiRuntime,
    width: u16,
    height: u16,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    unsafe { &mut *runtime }.runtime.set_viewport(width, height);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_macos_last_error(runtime: *mut FfiRuntime) -> *const c_char {
    if runtime.is_null() {
        return std::ptr::null();
    }
    unsafe { &*runtime }.last_error.as_ptr()
}
