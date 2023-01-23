use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::time::Duration;
use tauri::{Manager, Window};

use crate::data::Point;

#[derive(Debug)]
pub enum KeyMode {
    Press,
    Hold,
    Release,
}
#[derive(Clone)]
pub struct KeyManager {
    pub handle: tauri::AppHandle,
}

// For visual recognition: Avoids mouse clicks outside the window by ignoring monster names that are too close to the bottom of the GUI
pub const IGNORE_AREA_BOTTOM: u32 = 110;
impl KeyManager {
    /// Get Tauri window
    pub fn get_window(&self) -> Window {
        self.handle.get_window("client").unwrap()
    }
    /// Get the native window id.
    pub fn get_window_id(&self) -> Option<u64> {
        let window = self.get_window();
        #[allow(unused_variables)]
        match window.raw_window_handle() {
            RawWindowHandle::Xlib(handle) => Some(handle.window as u64),
            RawWindowHandle::Win32(handle) => Some(handle.hwnd as u64),
            RawWindowHandle::AppKit(handle) => {
                #[cfg(target_os = "macos")]
                unsafe {
                    use std::ffi::c_void;
                    let ns_window_ptr = handle.ns_window as *const c_void;
                    libscreenshot::platform::macos::macos_helper::ns_window_to_window_id(
                        ns_window_ptr,
                    )
                    .map(|id| id as u64)
                }
                #[cfg(not(target_os = "macos"))]
                unreachable!()
            }
            _ => Some(0_u64),
        }
    }

    pub fn eval_send_key(&self, key: &str, mode: KeyMode) {
        let window = self.get_window();
        match mode {
        KeyMode::Press => {
            drop(window.eval(format!("
                document.querySelector('canvas').dispatchEvent(new KeyboardEvent('keydown', {{'key': '{0}'}}))
                document.querySelector('canvas').dispatchEvent(new KeyboardEvent('keyup', {{'key': '{0}'}}))"
            , key).as_str()))
        },
        KeyMode::Hold => {
            drop(window.eval(format!("
                document.querySelector('canvas').dispatchEvent(new KeyboardEvent('keydown', {{'key': '{0}'}}))"
            , key).as_str()))
        },
        KeyMode::Release => {
            drop(window.eval(format!("
                document.querySelector('canvas').dispatchEvent(new KeyboardEvent('keyup', {{'key': '{0}'}}))"
            , key).as_str()))
        },
    }
    }

    pub fn send_slot_eval(&self, slot_bar_index: usize, k: usize) {
        let window = self.get_window();
        self.eval_send_key(
            format!("F{}", slot_bar_index + 1).to_string().as_str(),
            KeyMode::Press,
        );
        self.eval_send_key(k.to_string().as_str(), KeyMode::Press);
        //std::thread::sleep(Duration::from_millis(100));
    }

    /* pub fn eval_mouse_click_at_point(&self, pos: Point) {
        drop(
            window.eval(
                format!(
                    "
            document.querySelector('canvas').dispatchEvent(new MouseEvent('mousedown', {{
                clientX: {0},
                clientY: {1}
            }}))

            document.querySelector('canvas').dispatchEvent(new MouseEvent('mouseup', {{
                clientX: {0},
                clientY: {1}
            }}))",
                    pos.x, pos.y
                )
                .as_str(),
            ),
        );
    } */

    pub fn eval_mouse_move(&self, pos: Point) {
        let window = self.get_window();
        drop(
            window.eval(
                format!(
                    "
        document.querySelector('canvas').dispatchEvent(new MouseEvent('mousemove', {{
            clientX: {0},
            clientY: {1}
        }}))",
                    pos.x, pos.y
                )
                .as_str(),
            ),
        );
    }

    pub fn eval_mob_click(&self, pos: Point) {
        let window = self.get_window();
        //eval_mouse_move(window, pos);
        //std::thread::sleep(Duration::from_millis(25));
        drop(
        window.eval(
            format!(
                "
                    document.querySelector('canvas').dispatchEvent(new MouseEvent('mousemove', {{
                        clientX: {0},
                        clientY: {1}
                    }}))
                    setTimeout(()=>{{
                        if (document.body.style.cursor.indexOf('curattack') > 0) {{
                            document.querySelector('canvas').dispatchEvent(new MouseEvent('mousedown', {{
                                clientX: {0},
                                clientY: {1}
                            }}))

                            document.querySelector('canvas').dispatchEvent(new MouseEvent('mouseup', {{
                                clientX: {0},
                                clientY: {1}
                            }}))
                        }}
                        setTimeout(()=>{{
                            document.querySelector('canvas').dispatchEvent(new MouseEvent('mousemove', {{
                                clientX: 0,
                                clientY: 0
                            }}))
                        }}, 20)

                    }}, 15)
                    global.gc();;",
                pos.x, pos.y
            )
            .as_str(),
        ),
    );
    }

    pub fn eval_avoid_mob_click(&self, pos: Point) {
        let window = self.get_window();
        self.eval_mouse_move(pos);
        std::thread::sleep(Duration::from_millis(25));
        drop(
        window.eval(
            format!(
                "
                    document.querySelector('canvas').dispatchEvent(new MouseEvent('mousemove', {{
                        clientX: {0},
                        clientY: {1}
                    }}))

                    if (document.body.style.cursor.indexOf('curattack') < 0) {{
                        document.querySelector('canvas').dispatchEvent(new MouseEvent('mousedown', {{
                            clientX: {0},
                            clientY: {1}
                        }}))

                        document.querySelector('canvas').dispatchEvent(new MouseEvent('mouseup', {{
                            clientX: {0},
                            clientY: {1}
                        }}))
                    }}

                    global.gc();",
                pos.x, pos.y
            )
            .as_str(),
        ),
    );
    }

    pub fn eval_send_message(&self, text: &str) {
        let window = self.get_window();
        drop(
            window.eval(
                format!(
                    "
    document.querySelector('input').value = '{0}';
    document.querySelector('input').select();",
                    text
                )
                .as_str(),
            ),
        );
    }
}
