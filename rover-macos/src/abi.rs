use std::ffi::{c_char, c_void};

pub type NativeViewHandle = *mut c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeViewKind {
    Window = 0,
    View = 1,
    Column = 2,
    Row = 3,
    Text = 4,
    Button = 5,
    Input = 6,
    Checkbox = 7,
    Image = 8,
    ScrollView = 9,
}

pub type CreateViewFn = extern "C" fn(node_id: u32, kind: NativeViewKind) -> NativeViewHandle;
pub type AppendChildFn = extern "C" fn(parent: NativeViewHandle, child: NativeViewHandle);
pub type RemoveViewFn = extern "C" fn(view: NativeViewHandle);
pub type SetFrameFn =
    extern "C" fn(view: NativeViewHandle, x: f32, y: f32, width: f32, height: f32);
pub type SetTextFn = extern "C" fn(view: NativeViewHandle, ptr: *const c_char, len: usize);
pub type SetBoolFn = extern "C" fn(view: NativeViewHandle, value: bool);
pub type SetWindowFn = extern "C" fn(
    view: NativeViewHandle,
    title: *const c_char,
    len: usize,
    width: f32,
    height: f32,
);
pub type StopAppFn = extern "C" fn();

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HostCallbacks {
    pub create_view: Option<CreateViewFn>,
    pub append_child: Option<AppendChildFn>,
    pub remove_view: Option<RemoveViewFn>,
    pub set_frame: Option<SetFrameFn>,
    pub set_text: Option<SetTextFn>,
    pub set_bool: Option<SetBoolFn>,
    pub set_window: Option<SetWindowFn>,
    pub stop_app: Option<StopAppFn>,
}
