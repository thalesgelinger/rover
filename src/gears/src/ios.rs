use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use objc2::{
    msg_send,
    runtime::{AnyClass, NSObject},
};

use crate::lua::gretting_rs;

#[no_mangle]
pub extern "C" fn start(view: *mut NSObject) {
    unsafe {
        let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");

        let _: () = msg_send![gears_ios, createView: view];
    }
}

#[no_mangle]
pub extern "C" fn gretting(name_ptr: *const c_char) -> *mut c_char {
    let name = unsafe {
        assert!(!name_ptr.is_null());
        CStr::from_ptr(name_ptr)
            .to_str()
            .expect("Invalid UTF-8 in input")
            .to_owned()
    };

    let result = gretting_rs(name);

    CString::new(result)
        .expect("Failed to create CString")
        .into_raw()
}

#[no_mangle]
pub extern "C" fn greeting_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        let _ = CString::from_raw(s);
    };
}
