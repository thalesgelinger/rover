use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use objc2::{
    msg_send,
    runtime::{AnyClass, AnyObject, NSObject},
    sel,
};

use crate::lua::gretting_rs;

#[no_mangle]
pub extern "C" fn start(view: *mut NSObject) {
    let container_view: *mut AnyObject = unsafe {
        let bounds: *mut AnyObject = msg_send![view, bounds];
        let container_view: *mut AnyObject = msg_send![AnyClass::get("UIView").unwrap(), alloc];
        let container_view: *mut AnyObject = msg_send![container_view, initWithFrame: bounds];

        // Set background color to white
        let white_color: *mut AnyObject =
            msg_send![AnyClass::get("UIColor").unwrap(), performSelector: sel!(blueColor)];
        let _: () = msg_send![container_view, setBackgroundColor: white_color];

        container_view
    };

    let label: *mut AnyObject = unsafe {
        let label: *mut AnyObject = msg_send![AnyClass::get("UILabel").unwrap(), alloc];

        let _: () = msg_send![label, initWithFrame: [0.0, 0.0, 200.0, 50.0]];

        let text: *mut AnyObject = msg_send![AnyClass::get("NSString").unwrap(), stringWithUTF8String: b"Hello from Rust!\0".as_ptr()];
        let _: () = msg_send![label, setText: text];

        let text_alignment: u64 = 1; // NSTextAlignmentCenter
        let _: () = msg_send![label, setTextAlignment: text_alignment];

        let _: () = msg_send![label, sizeToFit];

        let center: bool = msg_send![container_view, center];

        let _: bool = msg_send![label, setCenter: center];

        label
    };

    unsafe {
        let _: () = msg_send![container_view, addSubview: label];
    }

    unsafe {
        let _: () = msg_send![view, addSubview: container_view];
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
