#![cfg(target_os = "linux")]
//! Run only on an isolated X server: verifies actual XTest key/button delivery.
use std::time::{Duration, Instant};
use trinix_core::{InputEvent, MouseButton};
use trinix_host::input::{desktop::DesktopInput, InputLease};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    COPY_FROM_PARENT, CURRENT_TIME,
};
#[test]
#[ignore = "Requires an isolated Xvfb and MM_ALLOW_INPUT_SMOKE=1"]
fn x11_input_smoke() {
    assert_eq!(
        std::env::var("MM_ALLOW_INPUT_SMOKE").as_deref(),
        Ok("1"),
        "Use an isolated Xvfb and explicitly enable this input test"
    );
    let (connection, index) = x11rb::connect(None).unwrap();
    let screen = &connection.setup().roots[index];
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            COPY_FROM_PARENT as u8,
            window,
            screen.root,
            20,
            20,
            320,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(
                EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE,
            ),
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(window).unwrap().check().unwrap();
    connection
        .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    connection.flush().unwrap();
    let mut lease = InputLease(Box::new(
        DesktopInput::new(
            screen.width_in_pixels.into(),
            screen.height_in_pixels.into(),
        )
        .unwrap(),
    ));
    lease
        .0
        .apply(&InputEvent::MouseMove {
            x: 60.0 / (screen.width_in_pixels - 1) as f32,
            y: 80.0 / (screen.height_in_pixels - 1) as f32,
        })
        .unwrap();
    lease
        .0
        .apply(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
    lease
        .0
        .apply(&InputEvent::Key {
            code: 4,
            pressed: true,
        })
        .unwrap();
    drop(lease);
    let mut seen = [false; 4];
    let deadline = Instant::now() + Duration::from_secs(3);
    while !seen.iter().all(|v| *v) && Instant::now() < deadline {
        match connection.poll_for_event().unwrap() {
            Some(Event::KeyPress(_)) => seen[0] = true,
            Some(Event::KeyRelease(_)) => seen[1] = true,
            Some(Event::ButtonPress(_)) => seen[2] = true,
            Some(Event::ButtonRelease(_)) => seen[3] = true,
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    assert_eq!(
        seen, [true; 4],
        "A real key/button press or its lease-drop release was missing"
    );
    let pointer = connection
        .query_pointer(screen.root)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!((pointer.root_x, pointer.root_y), (60, 80));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires an isolated Xvfb, real codecs and MM_ALLOW_INPUT_SMOKE=1"]
async fn x11_multiviewer_control_smoke() {
    use std::io::{BufRead, BufReader};
    use trinix_viewer::net::{Command, ControlAction, State, ViewerSession};
    assert_eq!(std::env::var("MM_ALLOW_INPUT_SMOKE").as_deref(), Ok("1"));
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(
        std::process::Command::new(env!("CARGO_BIN_EXE_trinix-host"))
            .args([
                "--secure",
                "--allow-control",
                "--capture",
                "x11",
                "--bind",
                "127.0.0.1:0",
                "--fps",
                "10",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let link = line
        .strip_prefix("MM_INVITE ")
        .expect("Host did not publish an invitation")
        .trim()
        .to_owned();
    let first = ViewerSession::new(codec_gst::decoder::Decoder::new().unwrap());
    let a = first.output.clone();
    let ca = first.commands.clone();
    let task_a = tokio::spawn(first.run(link.clone()));
    let second = ViewerSession::new(codec_gst::decoder::Decoder::new().unwrap());
    let b = second.output.clone();
    let cb = second.commands.clone();
    let task_b = tokio::spawn(second.run(link));
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let task_a = Abort(task_a);
    let _task_b = Abort(task_b);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if a.lock().unwrap().state == State::Connected
            && b.lock().unwrap().state == State::Connected
            && a.lock().unwrap().picture.is_some()
            && b.lock().unwrap().picture.is_some()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Two viewers did not receive real H.264 simultaneously"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let epoch_a = a.lock().unwrap().control_epoch;
    ca.send(Command {
        generation: epoch_a,
        action: ControlAction::Control(true),
    })
    .await
    .unwrap();
    loop {
        if a.lock().unwrap().controlling {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let epoch_b = b.lock().unwrap().control_epoch;
    cb.send(Command {
        generation: epoch_b,
        action: ControlAction::Control(true),
    })
    .await
    .unwrap();
    loop {
        if b.lock().unwrap().control_response == Some(false) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Second viewer was not denied the occupied control slot"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ca.send(Command {
        generation: epoch_a,
        action: ControlAction::Input(InputEvent::Key {
            code: 225,
            pressed: true,
        }),
    })
    .await
    .unwrap();
    let (connection, _) = x11rb::connect(None).unwrap();
    loop {
        if connection
            .query_keymap()
            .unwrap()
            .reply()
            .unwrap()
            .keys
            .iter()
            .any(|key| *key != 0)
        {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(task_a);
    loop {
        if connection
            .query_keymap()
            .unwrap()
            .reply()
            .unwrap()
            .keys
            .iter()
            .all(|key| *key == 0)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Lost viewer left its remote Shift key held"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    cb.send(Command {
        generation: epoch_b,
        action: ControlAction::Control(true),
    })
    .await
    .unwrap();
    loop {
        if b.lock().unwrap().controlling {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Control ownership did not transfer after disconnect"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
