//! JNI exports. Handles index owned Rust clients; no pointer is stored in Java.
//! JNI entry names require no_mangle; all actual operations use safe jni APIs.
use crate::Client;
use jni::{
    objects::{JClass, JString},
    sys::{jbyteArray, jlong},
    JNIEnv,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, Ordering},
        Mutex, OnceLock,
    },
};
static CLIENTS: OnceLock<Mutex<HashMap<i64, Client>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);
fn clients() -> &'static Mutex<HashMap<i64, Client>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_connect(
    mut env: JNIEnv,
    _: JClass,
    address: JString,
) -> jlong {
    let result = (|| -> Result<i64, String> {
        let address: String = env.get_string(&address).map_err(|e| e.to_string())?.into();
        let mut clients = clients().lock().unwrap_or_else(|e| e.into_inner());
        if clients.len() >= 4 {
            return Err("Too many active connections".into());
        }
        let client = Client::connect(&address).map_err(|e| e.to_string())?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        if id <= 0 {
            return Err("Connection handles exhausted".into());
        }
        clients.insert(id, client);
        Ok(id)
    })();
    match result {
        Ok(id) => id,
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error);
            0
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_poll(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jbyteArray {
    let event = clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .and_then(Client::poll);
    match event {
        Some(bytes) => match env.byte_array_from_slice(&bytes) {
            Ok(array) => array.into_raw(),
            Err(error) => {
                let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
                std::ptr::null_mut()
            }
        },
        None => std::ptr::null_mut(),
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_disconnect(
    _env: JNIEnv,
    _: JClass,
    id: jlong,
) {
    // Release the table lock before joining the worker; other clients can poll.
    let client = clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    drop(client);
}

use crate::hosting::ScreenHost;
use jni::objects::JByteArray;
use jni::sys::{jboolean, jint, jstring};
static HOSTS: OnceLock<Mutex<HashMap<i64, ScreenHost>>> = OnceLock::new();
fn hosts() -> &'static Mutex<HashMap<i64, ScreenHost>> {
    HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_startHost(
    mut env: JNIEnv,
    _: JClass,
    address: JString,
    width: jint,
    height: jint,
    fps: jint,
) -> jlong {
    let result = (|| -> Result<i64, String> {
        let address: String = env.get_string(&address).map_err(|e| e.to_string())?.into();
        let mut table = hosts().lock().unwrap_or_else(|e| e.into_inner());
        if !table.is_empty() {
            return Err("Screen sharing is already active".into());
        }
        let host = ScreenHost::start(&address, width as u32, height as u32, fps as u32)
            .map_err(|e| e.to_string())?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        if id <= 0 {
            return Err("Session handles exhausted".into());
        }
        table.insert(id, host);
        Ok(id)
    })();
    match result {
        Ok(id) => id,
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
            0
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_publish(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    bytes: JByteArray,
    timestamp: jlong,
    keyframe: jboolean,
) -> jboolean {
    let length = env.get_array_length(&bytes).unwrap_or(0);
    if length <= 0 || length > 8 * 1024 * 1024 || timestamp < 0 {
        return 0;
    }
    match env.convert_byte_array(&bytes) {
        Ok(data) => hosts()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .is_some_and(|host| host.publish(data, timestamp as u64, keyframe != 0))
            as jboolean,
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            0
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_hostNeedsKeyframe(
    _env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jboolean {
    hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .is_some_and(ScreenHost::needs_keyframe) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_hostBitrate(
    _env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jint {
    hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .map_or(4_000_000, ScreenHost::bitrate) as jint
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_hostStatus(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jstring {
    let text = hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .map_or_else(|| "Not sharing".into(), ScreenHost::status);
    match env.new_string(text) {
        Ok(s) => s.into_raw(),
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", e.to_string());
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_stopHost(
    _env: JNIEnv,
    _: JClass,
    id: jlong,
) {
    let host = hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    drop(host);
}

#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_hostInvite(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    address: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let address: String = env.get_string(&address).map_err(|e| e.to_string())?.into();
        let address = address
            .parse()
            .map_err(|_| "Invalid sharing address".to_owned())?;
        let table = hosts().lock().unwrap_or_else(|e| e.into_inner());
        let host = table.get(&id).ok_or("Sharing stopped")?;
        Ok(host
            .invitation(address)
            .map_err(|e| e.to_string())?
            .to_link())
    })();
    match result {
        Ok(link) => env
            .new_string(link)
            .map_or(std::ptr::null_mut(), |s| s.into_raw()),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
            std::ptr::null_mut()
        }
    }
}

fn dispatch_input(env: &mut JNIEnv, id: jlong, event: trinix_core::InputEvent) -> jboolean {
    let result = clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .ok_or_else(|| "Viewer disconnected".to_owned())
        .and_then(|client| client.input(event).map_err(|e| e.to_string()));
    match result {
        Ok(()) => 1,
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
            0
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_requestControl(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    enabled: jboolean,
) -> jboolean {
    let result = clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .ok_or_else(|| "Viewer disconnected".to_owned())
        .and_then(|client| {
            client
                .request_control(enabled != 0)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => 1,
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
            0
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_pointer(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
    phase: jint,
) -> jboolean {
    use trinix_core::{InputEvent, TouchPhase};
    let phase = match phase {
        0 => TouchPhase::Start,
        1 => TouchPhase::Move,
        2 => TouchPhase::End,
        3 => TouchPhase::Cancel,
        _ => return 0,
    };
    dispatch_input(&mut env, id, InputEvent::Touch { id: 0, phase, x, y })
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_key(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    code: jint,
    pressed: jboolean,
) -> jboolean {
    dispatch_input(
        &mut env,
        id,
        trinix_core::InputEvent::Key {
            code: code as u32,
            pressed: pressed != 0,
        },
    )
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_typeText(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    text: JString,
) -> jboolean {
    let Ok(text) = env.get_string(&text) else {
        return 0;
    };
    let text: String = text.into();
    dispatch_input(&mut env, id, trinix_core::InputEvent::Text(text))
}

#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_allowHostControl(
    _env: JNIEnv,
    _: JClass,
    id: jlong,
    allowed: jboolean,
) {
    if let Some(host) = hosts().lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
        host.set_control_allowed(allowed != 0);
    }
}
#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_pollHostInput(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jbyteArray {
    use trinix_core::{InputEvent, MouseButton, TouchPhase};
    let event = hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .and_then(ScreenHost::poll_input);
    let Some(event) = event else {
        return std::ptr::null_mut();
    };
    let mut data = Vec::new();
    match event {
        InputEvent::MouseMove { x, y } => {
            data.push(1);
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
        }
        InputEvent::MouseButton { button, pressed } => data.extend_from_slice(&[
            2,
            match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                MouseButton::Back => 3,
                MouseButton::Forward => 4,
            },
            u8::from(pressed),
        ]),
        InputEvent::Touch { phase, x, y, .. } => {
            data.extend_from_slice(&[
                3,
                match phase {
                    TouchPhase::Start => 0,
                    TouchPhase::Move => 1,
                    TouchPhase::End => 2,
                    TouchPhase::Cancel => 3,
                },
            ]);
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
        }
        InputEvent::Key { code, pressed } => {
            data.push(4);
            data.extend_from_slice(&code.to_le_bytes());
            data.push(u8::from(pressed));
        }
        InputEvent::Text(text) => {
            data.push(5);
            data.extend_from_slice(text.as_bytes());
        }
        InputEvent::Scroll { dx, dy } => {
            data.push(6);
            data.extend_from_slice(&dx.to_le_bytes());
            data.extend_from_slice(&dy.to_le_bytes());
        }
        InputEvent::ReleaseAll => data.push(7),
    }
    match env.byte_array_from_slice(&data) {
        Ok(array) => array.into_raw(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_mightymonitor_app_NativeBridge_releaseInput(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jboolean {
    dispatch_input(&mut env, id, trinix_core::InputEvent::ReleaseAll)
}
