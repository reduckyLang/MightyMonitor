//! Explicitly authorized remote input. View-only sessions never create a backend.
use crate::Error;
use trinix_core::{InputEvent, MouseButton, TouchPhase};
pub trait InputTarget: Send {
    fn activate(&mut self) -> bool {
        true
    }
    fn deactivate(&mut self) {
        self.release_all();
    }
    fn apply(&mut self, event: &InputEvent) -> Result<(), Error>;
    fn release_all(&mut self);
}
/// Releases held input even when a session future is cancelled or an error unwinds.
pub struct InputLease(pub Box<dyn InputTarget>);
impl Drop for InputLease {
    fn drop(&mut self) {
        self.0.deactivate();
    }
}
/// Several viewers may watch, but only one owns the keyboard/mouse at a time.
pub struct ExclusiveInput {
    backend: Box<dyn InputTarget>,
    slot: std::sync::Arc<tokio::sync::Semaphore>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
impl ExclusiveInput {
    pub fn new(
        backend: Box<dyn InputTarget>,
        slot: std::sync::Arc<tokio::sync::Semaphore>,
    ) -> Self {
        Self {
            backend,
            slot,
            permit: None,
        }
    }
}
impl InputTarget for ExclusiveInput {
    fn activate(&mut self) -> bool {
        if self.permit.is_none() {
            self.permit = self.slot.clone().try_acquire_owned().ok();
        }
        self.permit.is_some()
    }
    fn apply(&mut self, event: &InputEvent) -> Result<(), Error> {
        if self.permit.is_none() {
            return Err("Another viewer owns remote control".into());
        }
        self.backend.apply(event)
    }
    fn release_all(&mut self) {
        if self.permit.is_some() {
            self.backend.release_all();
        }
    }
    fn deactivate(&mut self) {
        self.release_all();
        self.permit = None;
    }
}
impl Drop for ExclusiveInput {
    fn drop(&mut self) {
        self.deactivate();
    }
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod desktop {
    use super::*;
    use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
    pub struct DesktopInput {
        backend: Enigo,
        width: u32,
        height: u32,
        keys: Vec<Key>,
        buttons: Vec<Button>,
        touch: bool,
        scroll: (f32, f32),
    }
    impl DesktopInput {
        pub fn new(width: u32, height: u32) -> Result<Self, Error> {
            if width < 2 || height < 2 || width > 8192 || height > 8192 {
                return Err("Unsupported control surface dimensions".into());
            }
            Ok(Self {
                backend: Enigo::new(&Settings {
                    linux_delay: 0,
                    ..Default::default()
                })?,
                width,
                height,
                keys: vec![],
                buttons: vec![],
                touch: false,
                scroll: (0.0, 0.0),
            })
        }
        fn point(&mut self, x: f32, y: f32) -> Result<(), Error> {
            self.backend.move_mouse(
                (x * (self.width - 1) as f32).round() as i32,
                (y * (self.height - 1) as f32).round() as i32,
                Coordinate::Abs,
            )?;
            Ok(())
        }
        fn button(&mut self, button: Button, pressed: bool) -> Result<(), Error> {
            if pressed {
                if !self.buttons.contains(&button) {
                    self.buttons.push(button);
                }
            } else {
                self.buttons.retain(|b| *b != button);
            }
            self.backend.button(
                button,
                if pressed {
                    Direction::Press
                } else {
                    Direction::Release
                },
            )?;
            Ok(())
        }
    }
    impl InputTarget for DesktopInput {
        fn apply(&mut self, event: &InputEvent) -> Result<(), Error> {
            if !event.validate() {
                return Err("Invalid remote input".into());
            }
            match event {
                InputEvent::MouseMove { x, y } => self.point(*x, *y)?,
                InputEvent::MouseButton { button, pressed } => self.button(
                    match button {
                        MouseButton::Left => Button::Left,
                        MouseButton::Right => Button::Right,
                        MouseButton::Middle => Button::Middle,
                        MouseButton::Back => Button::Back,
                        MouseButton::Forward => Button::Forward,
                    },
                    *pressed,
                )?,
                InputEvent::Scroll { dx, dy } => {
                    self.scroll.0 += dx;
                    self.scroll.1 += dy;
                    let x = self.scroll.0.trunc() as i32;
                    let y = self.scroll.1.trunc() as i32;
                    self.scroll.0 -= x as f32;
                    self.scroll.1 -= y as f32;
                    if x != 0 {
                        self.backend.scroll(x, Axis::Horizontal)?;
                    }
                    if y != 0 {
                        self.backend.scroll(y, Axis::Vertical)?;
                    }
                }
                InputEvent::Key { code, pressed } => {
                    let key = key(*code).ok_or("Unsupported remote key")?;
                    if *pressed {
                        if !self.keys.contains(&key) {
                            self.keys.push(key);
                        }
                    } else {
                        self.keys.retain(|k| *k != key);
                    }
                    self.backend.key(
                        key,
                        if *pressed {
                            Direction::Press
                        } else {
                            Direction::Release
                        },
                    )?;
                }
                InputEvent::Text(text) => {
                    self.release_all();
                    self.backend.text(text)?;
                }
                InputEvent::ReleaseAll => self.release_all(),
                InputEvent::Touch { id: 0, phase, x, y } => {
                    self.point(*x, *y)?;
                    match phase {
                        TouchPhase::Start => {
                            self.button(Button::Left, true)?;
                            self.touch = true;
                        }
                        TouchPhase::Move => {}
                        TouchPhase::End | TouchPhase::Cancel => {
                            if self.touch {
                                self.button(Button::Left, false)?;
                            }
                            self.touch = false;
                            self.scroll = (0.0, 0.0);
                        }
                    }
                }
                InputEvent::Touch { .. } => {
                    return Err("This desktop supports one remote touch contact".into())
                }
            }
            Ok(())
        }
        fn release_all(&mut self) {
            for key in self.keys.drain(..) {
                let _ = self.backend.key(key, Direction::Release);
            }
            for button in self.buttons.drain(..) {
                let _ = self.backend.button(button, Direction::Release);
            }
            self.touch = false;
            self.scroll = (0.0, 0.0);
        }
    }
    impl Drop for DesktopInput {
        fn drop(&mut self) {
            self.release_all();
        }
    }
    /// Portable USB HID usages, never OS-specific raw scan codes from the peer.
    fn key(code: u32) -> Option<Key> {
        Some(match code {
            4..=29 => Key::Unicode(char::from_u32('a' as u32 + code - 4)?),
            30..=38 => Key::Unicode(char::from_u32('1' as u32 + code - 30)?),
            39 => Key::Unicode('0'),
            40 => Key::Return,
            41 => Key::Escape,
            42 => Key::Backspace,
            43 => Key::Tab,
            44 => Key::Space,
            45 => Key::Unicode('-'),
            46 => Key::Unicode('='),
            47 => Key::Unicode('['),
            48 => Key::Unicode(']'),
            49 => Key::Unicode('\\'),
            51 => Key::Unicode(';'),
            52 => Key::Unicode('\''),
            53 => Key::Unicode('`'),
            54 => Key::Unicode(','),
            55 => Key::Unicode('.'),
            56 => Key::Unicode('/'),
            57 => Key::CapsLock,
            58 => Key::F1,
            59 => Key::F2,
            60 => Key::F3,
            61 => Key::F4,
            62 => Key::F5,
            63 => Key::F6,
            64 => Key::F7,
            65 => Key::F8,
            66 => Key::F9,
            67 => Key::F10,
            68 => Key::F11,
            69 => Key::F12,
            73 => Key::Insert,
            74 => Key::Home,
            75 => Key::PageUp,
            76 => Key::Delete,
            77 => Key::End,
            78 => Key::PageDown,
            79 => Key::RightArrow,
            80 => Key::LeftArrow,
            81 => Key::DownArrow,
            82 => Key::UpArrow,
            224 | 228 => Key::Control,
            225 | 229 => Key::Shift,
            226 | 230 => Key::Alt,
            227 | 231 => Key::Meta,
            _ => return None,
        })
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn portable_keys_reject_unmapped_codes() {
            assert_eq!(key(4), Some(Key::Unicode('a')));
            assert_eq!(key(29), Some(Key::Unicode('z')));
            assert_eq!(key(224), Some(Key::Control));
            assert!(key(u32::MAX).is_none());
        }
    }
}

#[cfg(all(target_os = "linux", feature = "pipewire"))]
impl InputTarget for host_linux::PortalInput {
    fn apply(&mut self, event: &InputEvent) -> Result<(), Error> {
        self.send(event.clone())?;
        Ok(())
    }
    fn release_all(&mut self) {
        self.release();
    }
}
