use crate::runtime::AndroidRuntime;
use jni::JNIEnv;
use jni::objects::{JObject, JString};
use jni::sys::{jboolean, jint, jlong, jstring};
use std::ffi::CString;

pub struct JniRuntime {
    runtime: AndroidRuntime,
    last_error: CString,
}

impl JniRuntime {
    fn new(env: &mut JNIEnv<'_>, host: JObject<'_>) -> Result<Self, String> {
        let vm = env.get_java_vm().map_err(|e| e.to_string())?;
        let host = env.new_global_ref(host).map_err(|e| e.to_string())?;
        Ok(Self {
            runtime: AndroidRuntime::new(vm, host).map_err(|e| e.to_string())?,
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
pub extern "system" fn Java_lu_rover_host_RoverRuntime_nativeInit(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    host: JObject<'_>,
) -> jlong {
    match JniRuntime::new(&mut env, host) {
        Ok(runtime) => Box::into_raw(Box::new(runtime)) as jlong,
        Err(err) => {
            eprintln!("{err}");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeFree(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
) {
    if runtime != 0 {
        let _ = unsafe { Box::from_raw(runtime as *mut JniRuntime) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeLoadLua(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    source: JString<'_>,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
    runtime.clear_error();
    let source = match env.get_string(&source) {
        Ok(source) => source.to_string_lossy().to_string(),
        Err(err) => {
            runtime.set_error(err.to_string());
            return 2;
        }
    };
    match runtime.runtime.load_lua(&source) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.to_string());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeTick(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
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
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeNextWakeMs(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
) -> jint {
    let Some(runtime) = runtime_ref(runtime) else {
        return -1;
    };
    runtime.runtime.next_wake_ms()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeDispatchClick(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    id: jint,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
    runtime.runtime.dispatch_click(id as u32);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeDispatchInput(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    id: jint,
    value: JString<'_>,
) -> jint {
    dispatch_text(&mut env, runtime, id, value, false)
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeDispatchSubmit(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    id: jint,
    value: JString<'_>,
) -> jint {
    dispatch_text(&mut env, runtime, id, value, true)
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeDispatchToggle(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    id: jint,
    checked: jboolean,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
    runtime.runtime.dispatch_toggle(id as u32, checked != 0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeSetViewport(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
    width: jint,
    height: jint,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
    runtime.runtime.set_viewport(width as u16, height as u16);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_lu_rover_host_RoverRuntime_nativeLastError(
    env: JNIEnv<'_>,
    _class: JObject<'_>,
    runtime: jlong,
) -> jstring {
    let Some(runtime) = runtime_ref(runtime) else {
        return std::ptr::null_mut();
    };
    let error = runtime.last_error.to_string_lossy();
    match env.new_string(error.as_ref()) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn dispatch_text(
    env: &mut JNIEnv<'_>,
    runtime: jlong,
    id: jint,
    value: JString<'_>,
    submit: bool,
) -> jint {
    let Some(runtime) = runtime_mut(runtime) else {
        return 1;
    };
    let value = match env.get_string(&value) {
        Ok(value) => value.to_string_lossy().to_string(),
        Err(err) => {
            runtime.set_error(err.to_string());
            return 2;
        }
    };
    if submit {
        runtime.runtime.dispatch_submit(id as u32, value);
    } else {
        runtime.runtime.dispatch_input(id as u32, value);
    }
    0
}

fn runtime_ref(runtime: jlong) -> Option<&'static JniRuntime> {
    if runtime == 0 {
        None
    } else {
        Some(unsafe { &*(runtime as *const JniRuntime) })
    }
}

fn runtime_mut(runtime: jlong) -> Option<&'static mut JniRuntime> {
    if runtime == 0 {
        None
    } else {
        Some(unsafe { &mut *(runtime as *mut JniRuntime) })
    }
}
