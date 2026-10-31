use std::{
    error::Error,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
use trinix_core::{pong_rtt_us, Message, MonotonicClock, TcpTransport, Transport, MAX_PAYLOAD_LEN};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const USAGE: &str = "Usage: bench [--frames N] [--size BYTES] [--fps N] [--rtt] [--max-p99-ms MS]\nDefaults: 1000 frames, 65536 data bytes, unpaced; --rtt uses sequential Ping/Pong (size ignored).";

#[derive(Debug)]
struct Args {
    frames: u64,
    size: usize,
    fps: Option<f64>,
    rtt: bool,
    max_p99: Option<f64>,
}
impl Args {
    fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let mut out = Self {
            frames: 1000,
            size: 65536,
            fps: None,
            rtt: false,
            max_p99: None,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                println!("{USAGE}");
                return Ok(None);
            }
            if arg == "--rtt" {
                out.rtt = true;
                continue;
            }
            if !matches!(
                arg.as_str(),
                "--frames" | "--size" | "--fps" | "--max-p99-ms"
            ) {
                return Err(format!("unknown argument: {arg}").into());
            }
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {arg}"))?;
            match arg.as_str() {
                "--frames" => out.frames = value.parse()?,
                "--size" => out.size = value.parse()?,
                "--fps" => out.fps = Some(value.parse()?),
                "--max-p99-ms" => out.max_p99 = Some(value.parse()?),
                _ => unreachable!(),
            }
        }
        if out.frames == 0 || usize::try_from(out.frames).is_err() {
            return Err("frames must be positive and fit usize".into());
        }
        // VideoFrame's fixed-width bincode metadata occupies 29 bytes.
        if !out.rtt && out.size > MAX_PAYLOAD_LEN - 29 {
            return Err("size plus VideoFrame metadata exceeds payload limit".into());
        }
        if let Some(fps) = out.fps {
            if !fps.is_finite() || fps <= 0.0 || Duration::try_from_secs_f64(1.0 / fps).is_err() {
                return Err("fps must be finite, positive, and representable".into());
            }
        }
        if out.max_p99.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err("max-p99-ms must be finite and nonnegative".into());
        }
        Ok(Some(out))
    }
}

async fn send(transport: &mut TcpTransport, message: Message) -> Result<()> {
    let (kind, payload) = message.to_frame_parts()?;
    transport.send(kind, &payload).await?;
    Ok(())
}
async fn recv(transport: &mut TcpTransport) -> Result<Message> {
    let frame = transport.recv().await?;
    Ok(Message::from_frame_parts(frame.kind, &frame.payload)?)
}

async fn run(args: Args) -> Result<()> {
    let clock = MonotonicClock::new();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut sender = TcpTransport::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let mut receiver = TcpTransport::new(stream)?;
    sender.set_read_timeout(Some(Duration::from_secs(10)));
    receiver.set_read_timeout(Some(Duration::from_secs(10)));
    let period = args.fps.map(|fps| Duration::from_secs_f64(1.0 / fps));
    let start = Instant::now();
    let send_clock = clock.clone();
    let send_work = async {
        let mut samples = Vec::new();
        for seq in 0..args.frames {
            if let Some(period) = period {
                // Relative pacing avoids overflow for very long runs; skip first wait.
                if seq > 0 {
                    tokio::time::sleep(period).await;
                }
            }
            if args.rtt {
                let t0 = send_clock.now_us();
                send(
                    &mut sender,
                    Message::Ping {
                        nonce: seq,
                        t_send_us: t0,
                    },
                )
                .await?;
                match recv(&mut sender).await? {
                    Message::Pong {
                        nonce,
                        t_send_us,
                        t_peer_recv_us,
                        t_peer_send_us,
                    } if nonce == seq && t_send_us == t0 => samples.push(pong_rtt_us(
                        t0,
                        t_peer_recv_us,
                        t_peer_send_us,
                        send_clock.now_us(),
                    )),
                    _ => return Err("unexpected Pong".into()),
                }
            } else {
                let data = vec![0x55; args.size];
                send(
                    &mut sender,
                    Message::VideoFrame {
                        seq,
                        timestamp_us: send_clock.now_us(),
                        keyframe: true,
                        data,
                    },
                )
                .await?;
            }
        }
        Ok::<_, Box<dyn Error + Send + Sync>>(samples)
    };
    let receive_work = async {
        let mut samples = Vec::new();
        for expected in 0..args.frames {
            match recv(&mut receiver).await? {
                Message::VideoFrame {
                    seq,
                    timestamp_us,
                    data,
                    ..
                } if !args.rtt && seq == expected && data.len() == args.size => {
                    samples.push(clock.now_us() - timestamp_us)
                }
                Message::Ping { nonce, t_send_us } if args.rtt && nonce == expected => {
                    let t_peer_recv_us = clock.now_us();
                    send(
                        &mut receiver,
                        Message::Pong {
                            nonce,
                            t_send_us,
                            t_peer_recv_us,
                            t_peer_send_us: clock.now_us(),
                        },
                    )
                    .await?
                }
                _ => return Err("unexpected message or sequence".into()),
            }
        }
        Ok::<_, Box<dyn Error + Send + Sync>>(samples)
    };
    let (rtt_samples, frame_samples) = tokio::try_join!(send_work, receive_work)?;
    let elapsed = start.elapsed().as_secs_f64();
    let mut samples = if args.rtt { rtt_samples } else { frame_samples };
    samples.sort_unstable();
    let ms = |p| bench::percentile(&samples, p).unwrap() as f64 / 1000.0;
    let size = if args.rtt { 0 } else { args.size };
    println!("mode={} frames={} size_bytes={} elapsed_s={:.6} throughput_mib_s={:.3} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3}", if args.rtt { "rtt" } else { "one_way" }, args.frames, size, elapsed, args.frames as f64 * size as f64 / elapsed / 1048576.0, ms(50), ms(95), ms(99), ms(100));
    if args.max_p99.is_some_and(|limit| ms(99) > limit) {
        return Err("p99 latency exceeded --max-p99-ms".into());
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    if let Some(args) = Args::parse(std::env::args().skip(1))? {
        run(args).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_arguments() {
        for input in [
            "--frames 0",
            "--fps NaN",
            "--fps 0",
            "--max-p99-ms -1",
            "--size 16777216",
            "--unknown",
            "--frames",
        ] {
            assert!(
                Args::parse(input.split_whitespace().map(str::to_owned)).is_err(),
                "{input}"
            );
        }
    }
}
