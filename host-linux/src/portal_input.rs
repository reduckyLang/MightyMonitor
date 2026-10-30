//! Bounded input queue for the same system-authorized Wayland capture session.
use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use trinix_core::InputEvent;
struct State {
    queue: Mutex<VecDeque<InputEvent>>,
    reset: AtomicBool,
    alive: AtomicBool,
}
#[derive(Clone)]
pub struct PortalInput(Arc<State>);
impl Default for PortalInput {
    fn default() -> Self {
        Self::new()
    }
}
impl PortalInput {
    pub fn new() -> Self {
        Self(Arc::new(State {
            queue: Mutex::new(VecDeque::new()),
            reset: AtomicBool::new(false),
            alive: AtomicBool::new(false),
        }))
    }
    pub fn available(&self) -> bool {
        self.0.alive.load(Ordering::Acquire)
    }
    pub fn send(&self, event: InputEvent) -> io::Result<()> {
        if !self.available() || !event.validate() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Wayland control permission is unavailable",
            ));
        }
        if matches!(event, InputEvent::ReleaseAll) {
            self.release();
            return Ok(());
        }
        let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= 64 {
            queue.clear();
            self.0.reset.store(true, Ordering::Release);
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Wayland input queue is full",
            ));
        }
        queue.push_back(event);
        Ok(())
    }
    pub fn release(&self) {
        self.0
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.0.reset.store(true, Ordering::Release);
    }
    pub(crate) fn authorize(&self, allowed: bool) {
        self.0.alive.store(allowed, Ordering::Release);
    }
}
/// USB HID usages to standard XKB keysyms. No peer-supplied native scan codes.
fn keysym(code: u32) -> Option<i32> {
    Some(match code {
        4..=29 => 'a' as i32 + code as i32 - 4,
        30..=38 => '1' as i32 + code as i32 - 30,
        39 => '0' as i32,
        40 => 0xff0d,
        41 => 0xff1b,
        42 => 0xff08,
        43 => 0xff09,
        44 => 0x20,
        45 => 0x2d,
        46 => 0x3d,
        47 => 0x5b,
        48 => 0x5d,
        49 => 0x5c,
        51 => 0x3b,
        52 => 0x27,
        53 => 0x60,
        54 => 0x2c,
        55 => 0x2e,
        56 => 0x2f,
        57 => 0xffe5,
        58..=69 => 0xffbe + code as i32 - 58,
        73 => 0xff63,
        74 => 0xff50,
        75 => 0xff55,
        76 => 0xffff,
        77 => 0xff57,
        78 => 0xff56,
        79 => 0xff53,
        80 => 0xff51,
        81 => 0xff54,
        82 => 0xff52,
        224 => 0xffe3,
        228 => 0xffe4,
        225 => 0xffe1,
        229 => 0xffe2,
        226 => 0xffe9,
        230 => 0xffea,
        227 => 0xffeb,
        231 => 0xffec,
        _ => return None,
    })
}
#[cfg(feature = "pipewire")]
pub(crate) async fn drive(
    portal: &ashpd::desktop::remote_desktop::RemoteDesktop<'_>,
    session: &ashpd::desktop::Session<'_, ashpd::desktop::remote_desktop::RemoteDesktop<'_>>,
    node: u32,
    size: (i32, i32),
    input: &PortalInput,
) -> Result<(), Box<dyn std::error::Error>> {
    use ashpd::desktop::remote_desktop::{KeyState, RemoteDesktop};
    use std::{collections::BTreeSet, time::Duration};
    use trinix_core::{MouseButton, TouchPhase};
    struct Close(PortalInput);
    impl Drop for Close {
        fn drop(&mut self) {
            self.0.authorize(false);
        }
    }
    let _close = Close(input.clone());
    if !input.available() {
        std::future::pending::<()>().await;
    }
    let mut keys = BTreeSet::new();
    let mut buttons = BTreeSet::new();
    async fn release(
        portal: &RemoteDesktop<'_>,
        session: &ashpd::desktop::Session<'_, RemoteDesktop<'_>>,
        keys: &mut BTreeSet<i32>,
        buttons: &mut BTreeSet<i32>,
    ) -> Result<(), ashpd::Error> {
        for key in std::mem::take(keys) {
            portal
                .notify_keyboard_keysym(session, key, KeyState::Released)
                .await?;
        }
        for button in std::mem::take(buttons) {
            portal
                .notify_pointer_button(session, button, KeyState::Released)
                .await?;
        }
        Ok(())
    }
    loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let reset = input.0.reset.swap(false, Ordering::AcqRel);
        let event = input
            .0
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front();
        let operation = async {
            if reset {
                release(portal, session, &mut keys, &mut buttons).await?;
            }
            let Some(event) = event else {
                return Ok::<(), Box<dyn std::error::Error>>(());
            };
            match event {
                InputEvent::ReleaseAll => release(portal, session, &mut keys, &mut buttons).await?,
                InputEvent::MouseMove { x, y } => {
                    portal
                        .notify_pointer_motion_absolute(
                            session,
                            node,
                            f64::from(x) * f64::from(size.0 - 1),
                            f64::from(y) * f64::from(size.1 - 1),
                        )
                        .await?
                }
                InputEvent::MouseButton { button, pressed } => {
                    let code = match button {
                        MouseButton::Left => 0x110,
                        MouseButton::Right => 0x111,
                        MouseButton::Middle => 0x112,
                        MouseButton::Back => 0x116,
                        MouseButton::Forward => 0x115,
                    };
                    if pressed {
                        buttons.insert(code);
                    } else {
                        buttons.remove(&code);
                    }
                    portal
                        .notify_pointer_button(
                            session,
                            code,
                            if pressed {
                                KeyState::Pressed
                            } else {
                                KeyState::Released
                            },
                        )
                        .await?;
                }
                InputEvent::Key { code, pressed } => {
                    let symbol = keysym(code).ok_or("Unsupported remote key")?;
                    if pressed {
                        keys.insert(symbol);
                    } else {
                        keys.remove(&symbol);
                    }
                    portal
                        .notify_keyboard_keysym(
                            session,
                            symbol,
                            if pressed {
                                KeyState::Pressed
                            } else {
                                KeyState::Released
                            },
                        )
                        .await?;
                }
                InputEvent::Text(text) => {
                    release(portal, session, &mut keys, &mut buttons).await?;
                    for ch in text.chars() {
                        if input.0.reset.swap(false, Ordering::AcqRel) {
                            release(portal, session, &mut keys, &mut buttons).await?;
                            break;
                        }
                        let code = ch as u32;
                        let symbol = if code <= 0xff {
                            code
                        } else {
                            0x01000000 | code
                        } as i32;
                        keys.insert(symbol);
                        portal
                            .notify_keyboard_keysym(session, symbol, KeyState::Pressed)
                            .await?;
                        portal
                            .notify_keyboard_keysym(session, symbol, KeyState::Released)
                            .await?;
                        keys.remove(&symbol);
                    }
                }
                InputEvent::Scroll { dx, dy } => {
                    portal
                        .notify_pointer_axis(
                            session,
                            f64::from(dx) * 40.0,
                            f64::from(dy) * 40.0,
                            true,
                        )
                        .await?
                }
                InputEvent::Touch { id: 0, phase, x, y } => {
                    portal
                        .notify_pointer_motion_absolute(
                            session,
                            node,
                            f64::from(x) * f64::from(size.0 - 1),
                            f64::from(y) * f64::from(size.1 - 1),
                        )
                        .await?;
                    match phase {
                        TouchPhase::Start => {
                            buttons.insert(0x110);
                            portal
                                .notify_pointer_button(session, 0x110, KeyState::Pressed)
                                .await?;
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            buttons.remove(&0x110);
                            portal
                                .notify_pointer_button(session, 0x110, KeyState::Released)
                                .await?;
                        }
                        TouchPhase::Move => {}
                    }
                }
                InputEvent::Touch { .. } => {
                    return Err("Only one remote touch contact is supported".into())
                }
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(1), operation)
            .await
            .map_err(|_| "Wayland input request timed out")??;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_key_mapping_is_bounded() {
        assert_eq!(keysym(4), Some('a' as i32));
        assert_eq!(keysym(224), Some(0xffe3));
        assert!(keysym(u32::MAX).is_none());
    }
    #[test]
    fn input_queue_requires_authorization_and_discards_stale_commands() {
        let input = PortalInput::new();
        assert!(input
            .send(InputEvent::Key {
                code: 4,
                pressed: true
            })
            .is_err());
        input.authorize(true);
        input
            .send(InputEvent::Key {
                code: 4,
                pressed: true,
            })
            .unwrap();
        input.release();
        assert!(input.0.queue.lock().unwrap().is_empty());
        assert!(input.0.reset.load(Ordering::Acquire));
        input.authorize(false);
        assert!(input.send(InputEvent::ReleaseAll).is_err());
    }
}
