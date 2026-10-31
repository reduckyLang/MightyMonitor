//! Translate focused preview events into bounded portable remote input.
use eframe::egui::{self, Event, Key};
use trinix_core::{InputEvent, MouseButton};
pub fn hid(key: Key) -> Option<u32> {
    Some(match key {
        Key::A => 4,
        Key::B => 5,
        Key::C => 6,
        Key::D => 7,
        Key::E => 8,
        Key::F => 9,
        Key::G => 10,
        Key::H => 11,
        Key::I => 12,
        Key::J => 13,
        Key::K => 14,
        Key::L => 15,
        Key::M => 16,
        Key::N => 17,
        Key::O => 18,
        Key::P => 19,
        Key::Q => 20,
        Key::R => 21,
        Key::S => 22,
        Key::T => 23,
        Key::U => 24,
        Key::V => 25,
        Key::W => 26,
        Key::X => 27,
        Key::Y => 28,
        Key::Z => 29,
        Key::Num1 => 30,
        Key::Num2 => 31,
        Key::Num3 => 32,
        Key::Num4 => 33,
        Key::Num5 => 34,
        Key::Num6 => 35,
        Key::Num7 => 36,
        Key::Num8 => 37,
        Key::Num9 => 38,
        Key::Num0 => 39,
        Key::Enter => 40,
        Key::Escape => 41,
        Key::Backspace => 42,
        Key::Tab => 43,
        Key::Space => 44,
        Key::Minus => 45,
        Key::Equals => 46,
        Key::OpenBracket => 47,
        Key::CloseBracket => 48,
        Key::Backslash => 49,
        Key::Semicolon => 51,
        Key::Quote => 52,
        Key::Backtick => 53,
        Key::Comma => 54,
        Key::Period => 55,
        Key::Slash => 56,
        Key::F1 => 58,
        Key::F2 => 59,
        Key::F3 => 60,
        Key::F4 => 61,
        Key::F5 => 62,
        Key::F6 => 63,
        Key::F7 => 64,
        Key::F8 => 65,
        Key::F9 => 66,
        Key::F10 => 67,
        Key::F11 => 68,
        Key::F12 => 69,
        Key::Insert => 73,
        Key::Home => 74,
        Key::PageUp => 75,
        Key::Delete => 76,
        Key::End => 77,
        Key::PageDown => 78,
        Key::ArrowRight => 79,
        Key::ArrowLeft => 80,
        Key::ArrowDown => 81,
        Key::ArrowUp => 82,
        _ => return None,
    })
}
pub fn translate(
    events: &[Event],
    rect: egui::Rect,
    old: &mut egui::Modifiers,
    modifiers: egui::Modifiers,
) -> Vec<InputEvent> {
    if events.iter().any(|event| matches!(event, Event::Paste(_))) {
        let mut output = vec![InputEvent::ReleaseAll];
        for event in events {
            if let Event::Paste(text) = event {
                let mut chunk = String::new();
                for ch in text.chars() {
                    if chunk.len() + ch.len_utf8() > 1024 {
                        output.push(InputEvent::Text(std::mem::take(&mut chunk)));
                    }
                    chunk.push(ch);
                }
                if !chunk.is_empty() {
                    output.push(InputEvent::Text(chunk));
                }
            }
        }
        *old = egui::Modifiers::NONE;
        return output;
    }
    let mut output = vec![];
    for (code, before, after) in [
        (224, old.ctrl, modifiers.ctrl),
        (225, old.shift, modifiers.shift),
        (226, old.alt, modifiers.alt),
        (227, old.mac_cmd, modifiers.mac_cmd),
    ] {
        if before != after {
            output.push(InputEvent::Key {
                code,
                pressed: after,
            });
        }
    }
    *old = modifiers;
    let point = |pos: egui::Pos2| {
        (
            (pos.x - rect.min.x) / rect.width(),
            (pos.y - rect.min.y) / rect.height(),
        )
    };
    let mapped_text=events.iter().any(|event| matches!(event,Event::Key{key,physical_key,pressed:true,..} if hid(physical_key.unwrap_or(*key)).is_some_and(|code|(4..=56).contains(&code))));
    for event in events {
        match event {
            Event::PointerMoved(pos) if rect.contains(*pos) => {
                let (x, y) = point(*pos);
                output.push(InputEvent::MouseMove { x, y });
            }
            Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } if rect.contains(*pos) || !pressed => {
                let (x, y) = point(*pos);
                output.push(InputEvent::MouseMove {
                    x: x.clamp(0.0, 1.0),
                    y: y.clamp(0.0, 1.0),
                });
                output.push(InputEvent::MouseButton {
                    button: match button {
                        egui::PointerButton::Primary => MouseButton::Left,
                        egui::PointerButton::Secondary => MouseButton::Right,
                        egui::PointerButton::Middle => MouseButton::Middle,
                        egui::PointerButton::Extra1 => MouseButton::Back,
                        egui::PointerButton::Extra2 => MouseButton::Forward,
                    },
                    pressed: *pressed,
                });
            }
            Event::MouseWheel { unit, delta, .. } => {
                let factor = match unit {
                    egui::MouseWheelUnit::Point => 1.0 / 40.0,
                    egui::MouseWheelUnit::Line => 1.0,
                    egui::MouseWheelUnit::Page => 10.0,
                };
                output.push(InputEvent::Scroll {
                    dx: (-delta.x * factor).clamp(-120.0, 120.0),
                    dy: (-delta.y * factor).clamp(-120.0, 120.0),
                });
            }
            Event::Key {
                key,
                physical_key,
                pressed,
                ..
            } => {
                if let Some(code) = hid(physical_key.unwrap_or(*key)) {
                    output.push(InputEvent::Key {
                        code,
                        pressed: *pressed,
                    });
                }
            }
            Event::Text(text)
                if !mapped_text
                    && !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.mac_cmd
                    && !text.is_empty() =>
            {
                output.push(InputEvent::Text(text.clone()))
            }
            Event::Paste(text) if !text.is_empty() => {
                let mut chunk = String::new();
                for ch in text.chars() {
                    if chunk.len() + ch.len_utf8() > 1024 {
                        output.push(InputEvent::Text(std::mem::take(&mut chunk)));
                    }
                    chunk.push(ch);
                }
                if !chunk.is_empty() {
                    output.push(InputEvent::Text(chunk));
                }
            }
            _ => {}
        }
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paste_releases_shortcuts_and_preserves_unicode_across_chunks() {
        let text = "🙂".repeat(600);
        let mut modifiers = egui::Modifiers::CTRL;
        let events = [
            Event::Key {
                key: Key::V,
                physical_key: Some(Key::V),
                pressed: true,
                repeat: false,
                modifiers,
            },
            Event::Paste(text.clone()),
        ];
        let mapped = translate(
            &events,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
            &mut modifiers,
            egui::Modifiers::CTRL,
        );
        assert_eq!(mapped[0], InputEvent::ReleaseAll);
        let restored: String = mapped
            .iter()
            .filter_map(|event| {
                if let InputEvent::Text(text) = event {
                    assert!(text.len() <= 1024);
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(restored, text);
        assert!(!mapped
            .iter()
            .any(|event| matches!(event, InputEvent::Key { .. })));
    }
    #[test]
    fn letterboxed_preview_maps_only_image_and_releases_outside_buttons() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 100.0));
        let mut modifiers = egui::Modifiers::NONE;
        let events = [
            Event::PointerMoved(egui::pos2(50.0, 30.0)),
            Event::PointerMoved(egui::pos2(200.0, 100.0)),
            Event::PointerButton {
                pos: egui::pos2(400.0, 200.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            },
        ];
        let mapped = translate(&events, rect, &mut modifiers, egui::Modifiers::NONE);
        assert_eq!(mapped[0], InputEvent::MouseMove { x: 0.5, y: 0.5 });
        assert_eq!(mapped[1], InputEvent::MouseMove { x: 1.0, y: 1.0 });
        assert_eq!(
            mapped[2],
            InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: false
            }
        );
    }
}
