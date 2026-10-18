//! Portal-authorized Wayland capture. Native dependencies are feature gated.
use std::time::Instant;

use crate::CapturedFrame;

#[derive(Clone, Copy, Debug)]
enum PixelFormat {
    Bgrx,
    Rgbx,
}

/// Validate the mapped chunk before accessing rows; discard padding and X alpha.
fn convert(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: i32,
    format: PixelFormat,
    start: Instant,
) -> Option<CapturedFrame> {
    let row = usize::try_from(width).ok()?.checked_mul(4)?;
    let rows = usize::try_from(height).ok()?;
    let stride = usize::try_from(stride).ok()?;
    if row == 0 || rows == 0 || stride < row {
        return None;
    }
    let needed = stride.checked_mul(rows - 1)?.checked_add(row)?;
    if bytes.len() < needed {
        return None;
    }
    let mut data = Vec::with_capacity(row.checked_mul(rows)?);
    for y in 0..rows {
        for p in bytes[y * stride..y * stride + row].chunks_exact(4) {
            let (b, r) = match format {
                PixelFormat::Bgrx => (p[0], p[2]),
                PixelFormat::Rgbx => (p[2], p[0]),
            };
            data.extend_from_slice(&[b, p[1], r, 255]);
        }
    }
    Some(CapturedFrame {
        width,
        height,
        timestamp_us: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        data,
    })
}

#[cfg(feature = "pipewire")]
mod native {
    use super::*;
    use ashpd::desktop::{
        screencast::{CursorMode, Screencast, SourceType},
        PersistMode,
    };
    use futures_util::StreamExt;
    use mightymonitor_core::{Bytes, Capture};
    use pipewire::{self as pw, properties::properties, spa};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread::JoinHandle,
        time::Duration,
    };

    /// Setup errors include portal cancellation, unavailable services and negotiation errors.
    #[derive(Debug, thiserror::Error)]
    #[error("PipeWire capture: {0}")]
    pub struct PipeWireError(String);

    /// Owns a portal session and a worker. Drop stops capture and closes the session.
    pub struct PipeWireCapture {
        frames: mpsc::Receiver<CapturedFrame>,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl PipeWireCapture {
        /// Blocks for the portal consent dialog. Call outside UI/async executor threads.
        pub fn open() -> Result<Self, PipeWireError> {
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let (frames_tx, frames) = mpsc::sync_channel(1);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = stop.clone();
            let worker = std::thread::spawn(move || {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async {
                        let portal = Screencast::new().await?;
                        let session = portal.create_session().await?;
                        let result = async {
                            let closed = session.receive_closed().await?;
                            let mut closed = std::pin::pin!(closed);
                            portal
                                .select_sources(
                                    &session,
                                    CursorMode::Hidden,
                                    SourceType::Monitor.into(),
                                    false,
                                    None,
                                    PersistMode::DoNot,
                                )
                                .await?
                                .response()?;
                            let response = portal.start(&session, None).await?.response()?;
                            let node = response
                                .streams()
                                .first()
                                .ok_or("portal returned no streams")?
                                .pipe_wire_node_id();
                            let fd = portal.open_pipe_wire_remote(&session).await?;
                            tokio::select! {
                                result = receive(fd, node, frames_tx, &ready_tx, &worker_stop) => result,
                                _ = closed.next() => Err("portal session closed".into()),
                            }
                        }
                        .await;
                        let _ = session.close().await;
                        result
                    })
                })();
                if let Err(error) = result {
                    let _ = ready_tx.try_send(Err(PipeWireError(error.to_string())));
                }
            });
            match ready_rx.recv() {
                Ok(Ok(())) => Ok(Self {
                    frames,
                    stop,
                    worker: Some(worker),
                }),
                Ok(Err(error)) => {
                    let _ = worker.join();
                    Err(error)
                }
                Err(error) => {
                    stop.store(true, Ordering::Relaxed);
                    let _ = worker.join();
                    Err(PipeWireError(error.to_string()))
                }
            }
        }

        /// Wait at most five seconds. Timestamps measure monotonic elapsed time
        /// since worker setup, recorded after conversion, before queue delivery.
        pub fn next_captured(&mut self) -> Option<CapturedFrame> {
            self.frames.recv_timeout(Duration::from_secs(5)).ok()
        }
    }

    impl Capture for PipeWireCapture {
        fn next_frame(&mut self) -> Option<Bytes> {
            self.next_captured().map(|frame| frame.data)
        }
    }

    impl Drop for PipeWireCapture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    async fn receive(
        fd: std::os::fd::OwnedFd,
        node: u32,
        frames: mpsc::SyncSender<CapturedFrame>,
        ready: &mpsc::SyncSender<Result<(), PipeWireError>>,
        stop: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mainloop = pw::main_loop::MainLoop::new(None)?;
        let context = pw::context::Context::new(&mainloop)?;
        let core = context.connect_fd(fd, None)?;
        let stream = pw::stream::Stream::new(
            &core,
            "MightyMonitor",
            properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )?;
        let failed = std::rc::Rc::new(std::cell::RefCell::new(None));
        let core_failure = failed.clone();
        let _core_listener = core
            .add_listener_local()
            .error(move |_, _, _, message| {
                *core_failure.borrow_mut() = Some(message.to_string());
            })
            .register();
        let failure = failed.clone();
        let start = Instant::now();
        let _listener = stream
            .add_local_listener_with_user_data(spa::param::video::VideoInfoRaw::default())
            .state_changed(move |_, _, _, state| {
                if let pw::stream::StreamState::Error(error) = state {
                    *failure.borrow_mut() = Some(error.to_string());
                }
            })
            .param_changed(|_, format, id, param| {
                if id == spa::param::ParamType::Format.as_raw() {
                    *format = Default::default();
                    if let Some(param) = param {
                        let _ = format.parse(param);
                    }
                }
            })
            .process(move |stream, info| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let format = match info.format() {
                    spa::param::video::VideoFormat::BGRx => PixelFormat::Bgrx,
                    spa::param::video::VideoFormat::RGBx => PixelFormat::Rgbx,
                    _ => return,
                };
                let offset = data.chunk().offset() as usize;
                let size = data.chunk().size() as usize;
                let stride = data.chunk().stride();
                let Some(bytes) = data.data() else { return };
                let Some(end) = offset.checked_add(size) else {
                    return;
                };
                let Some(bytes) = bytes.get(offset..end) else {
                    return;
                };
                if let Some(frame) = convert(
                    bytes,
                    info.size().width,
                    info.size().height,
                    stride,
                    format,
                    start,
                ) {
                    // Bounded queue: never block the PipeWire callback on a slow caller.
                    let _ = frames.try_send(frame);
                }
            })
            .register()?;
        let object = spa::pod::object!(
            spa::utils::SpaTypes::ObjectParamFormat,
            spa::param::ParamType::EnumFormat,
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaType,
                Id,
                spa::param::format::MediaType::Video
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaSubtype,
                Id,
                spa::param::format::MediaSubtype::Raw
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::RGBx
            )
        );
        let bytes = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(object),
        )?
        .0
        .into_inner();
        let mut params = [spa::pod::Pod::from_bytes(&bytes).ok_or("invalid format pod")?];
        stream.connect(
            spa::utils::Direction::Input,
            Some(node),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )?;
        let _ = ready.send(Ok(()));
        while !stop.load(Ordering::Relaxed) {
            mainloop.loop_().iterate(Duration::ZERO);
            if let Some(error) = failed.borrow_mut().take() {
                return Err(error.into());
            }
            // Also drive portal D-Bus tasks, including session revocation.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok(())
    }
}

#[cfg(feature = "pipewire")]
pub use native::{PipeWireCapture, PipeWireError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_backend_conversion_stride_and_timestamp() {
        let start = Instant::now();
        let bytes = [1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0];
        let first = convert(&bytes, 1, 2, 8, PixelFormat::Bgrx, start).unwrap();
        assert_eq!((first.width, first.height), (1, 2));
        assert_eq!(first.data, [1, 2, 3, 255, 4, 5, 6, 255]);
        let second = convert(&bytes, 1, 2, 8, PixelFormat::Rgbx, start).unwrap();
        assert_eq!(second.data, [3, 2, 1, 255, 6, 5, 4, 255]);
        assert!(second.timestamp_us >= first.timestamp_us);
        assert!(second.timestamp_us <= start.elapsed().as_micros() as u64);
        for (width, height, stride) in [
            (0, 2, 8),
            (1, 0, 8),
            (1, 2, 3),
            (1, 2, -8),
            (2, 2, 8),
            (u32::MAX, u32::MAX, i32::MAX),
        ] {
            assert!(convert(&bytes, width, height, stride, PixelFormat::Bgrx, start).is_none());
        }
    }
}
