//! Serializable remote input events.

use serde::{Deserialize, Serialize};

/// Remote input. Mouse and touch coordinates are normalized to `0..=1`.
/// Coordinate ranges are a caller contract, not validated during decoding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
    Key {
        code: u32,
        pressed: bool,
    },
    Touch {
        id: u32,
        phase: TouchPhase,
        x: f32,
        y: f32,
    },
}

/// A mouse button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Phase of a touch contact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase {
    Start,
    Move,
    End,
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    #[test]
    fn input_event_roundtrip() {
        let mut events = vec![
            InputEvent::MouseMove { x: 0.25, y: 0.75 },
            InputEvent::Scroll { dx: -1.5, dy: 2.0 },
            InputEvent::Key {
                code: 42,
                pressed: true,
            },
        ];
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Back,
            MouseButton::Forward,
        ] {
            events.push(InputEvent::MouseButton {
                button,
                pressed: false,
            });
        }
        for phase in [
            TouchPhase::Start,
            TouchPhase::Move,
            TouchPhase::End,
            TouchPhase::Cancel,
        ] {
            events.push(InputEvent::Touch {
                id: 7,
                phase,
                x: 0.0,
                y: 1.0,
            });
        }
        for event in events {
            let message = Message::Input(event);
            assert_eq!(
                Message::from_bytes(&message.to_bytes().unwrap()).unwrap(),
                message
            );
        }
    }
}
