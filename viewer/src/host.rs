//! Lifecycle for the optional sibling host executable. Never invokes a shell.
use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    net::SocketAddr,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default, Clone)]
pub struct PairStatus {
    pub state: Option<trinix_security::managed::Status>,
    pub offer: Option<String>,
}
pub struct HostProcess {
    pairing: Arc<Mutex<PairStatus>>,
    child: Child,
    log: File,
    log_path: PathBuf,
    invitation: Arc<Mutex<Option<trinix_security::Invitation>>>,
    invitation_worker: Option<std::thread::JoinHandle<()>>,
}
impl HostProcess {
    pub fn start(address: &str, fps: u32, bitrate: u32) -> io::Result<Self> {
        Self::start_with_control(address, fps, bitrate, false)
    }
    pub fn start_with_control(
        address: &str,
        fps: u32,
        bitrate: u32,
        allow_control: bool,
    ) -> io::Result<Self> {
        Self::start_with_options(address, fps, bitrate, allow_control, false)
    }
    pub fn start_with_options(
        address: &str,
        fps: u32,
        bitrate: u32,
        allow_control: bool,
        extend: bool,
    ) -> io::Result<Self> {
        if !cfg!(any(target_os = "linux", target_os = "windows")) {
            return Err(io::Error::other(
                "Screen sharing currently requires Linux or Windows.",
            ));
        }
        validate(address, fps, bitrate)?;
        let binary = std::env::current_exe()?.with_file_name(if cfg!(target_os = "windows") {
            "trinix-host.exe"
        } else {
            "trinix-host"
        });
        if !binary.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Sharing component missing. Reinstall Trinix with its screen-sharing component.",
            ));
        }
        let mut command = Command::new(binary);
        command.args([
            "--secure",
            "--managed",
            "--bind",
            address,
            "--fps",
            &fps.to_string(),
            "--bitrate",
            &bitrate.to_string(),
        ]);
        if allow_control {
            command.arg("--allow-control");
        }
        if extend {
            command.arg("--extend");
        }
        Self::spawn(command)
    }
    fn spawn(mut command: Command) -> io::Result<Self> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            // The CLI remains usable in a terminal; GUI-owned sharing has no console.
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let log_path =
            std::env::temp_dir().join(format!("trinix-{}-{stamp}.log", std::process::id()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let log = options.open(&log_path)?;
        let spawned = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(OpenOptions::new().append(true).open(&log_path)?)
            .spawn();
        match spawned {
            Ok(mut child) => {
                let invitation = Arc::new(Mutex::new(None));
                let shared = invitation.clone();
                let pairing = Arc::new(Mutex::new(PairStatus::default()));
                let shared_pairing = pairing.clone();
                let Some(stdout) = child.stdout.take() else {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&log_path);
                    return Err(io::Error::other("Sharing invitation channel unavailable"));
                };
                let worker = std::thread::Builder::new()
                    .name("mm-invitation".into())
                    .spawn(move || {
                        let mut input = BufReader::new(stdout);
                        while let Ok(Some(line)) =
                            trinix_security::managed::read_line(&mut input)
                        {
                            if let Some(link) = line.strip_prefix("MM_INVITE ") {
                                if let Ok(value) = link.parse() {
                                    if let Ok(mut shared) = shared.lock() {
                                        *shared = Some(value);
                                    }
                                }
                            } else if let Some(status) =
                                trinix_security::managed::Status::parse(&line)
                            {
                                if let Ok(mut pairing) = shared_pairing.lock() {
                                    if matches!(
                                        status,
                                        trinix_security::managed::Status::Expired
                                            | trinix_security::managed::Status::Locked
                                    ) {
                                        pairing.offer = None;
                                    }
                                    pairing.state = Some(status);
                                }
                            } else if let Some(value) = line.strip_prefix("MM_PAIR_OFFER ") {
                                if trinix_security::pairing::PairingOffer::parse(
                                    value,
                                    &trinix_security::pairing::SystemClock,
                                )
                                .is_ok()
                                {
                                    if let Ok(mut pairing) = shared_pairing.lock() {
                                        pairing.offer = Some(value.into());
                                    }
                                }
                            }
                        }
                    });
                match worker {
                    Ok(worker) => Ok(Self {
                        child,
                        pairing,
                        log,
                        log_path,
                        invitation,
                        invitation_worker: Some(worker),
                    }),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(log);
                        let _ = std::fs::remove_file(log_path);
                        Err(error)
                    }
                }
            }
            Err(error) => {
                drop(log);
                let _ = std::fs::remove_file(log_path);
                Err(error)
            }
        }
    }
    pub fn pairing(&self) -> PairStatus {
        self.pairing
            .lock()
            .map_or_else(|_| PairStatus::default(), |state| state.clone())
    }
    pub fn command(&mut self, command: trinix_security::managed::Command) -> io::Result<()> {
        let line = command.line();
        if trinix_security::managed::Command::parse(&line).is_none() {
            return Err(io::Error::other("Invalid sharing command"));
        }
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("Sharing stopped"))?;
        writeln!(input, "{line}")?;
        input.flush()
    }
    pub fn invitation(&self) -> Option<trinix_security::Invitation> {
        self.invitation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    pub fn diagnostics(&mut self) -> io::Result<String> {
        // Bound UI work even if the host produced many diagnostics.
        self.log.seek(SeekFrom::End(0))?;
        let length = self.log.stream_position()?;
        self.log
            .seek(SeekFrom::Start(length.saturating_sub(4096)))?;
        let mut bytes = Vec::new();
        (&mut self.log).take(4096).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
impl Drop for HostProcess {
    fn drop(&mut self) {
        if let Some(mut input) = self.child.stdin.take() {
            let _ = input.write_all(b"STOP\n");
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while self.child.try_wait().ok().flatten().is_none() && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(worker) = self.invitation_worker.take() {
            let _ = worker.join();
        }
        *self.invitation.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = std::fs::remove_file(&self.log_path);
    }
}
fn validate(address: &str, fps: u32, bitrate: u32) -> io::Result<()> {
    let parsed: SocketAddr = address
        .parse()
        .map_err(|_| io::Error::other("Enter a local IP and port, such as 192.168.1.10:5000."))?;
    if parsed.ip().is_unspecified() {
        return Err(io::Error::other(
            "Choose this computer's Wi-Fi IP for encrypted sharing",
        ));
    }
    if parsed.port() == 0 || !(1..=240).contains(&fps) || !(1..=1_000_000).contains(&bitrate) {
        return Err(io::Error::other(
            "Use port 1–65535, 1–240 fps, and bitrate 1–1000000 kbit/s.",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::validate;
    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_owned_host_terminates_and_reaps_it() {
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        let host = super::HostProcess::spawn(command).unwrap();
        let pid = host.child.id();
        let log = host.log_path.clone();
        drop(host);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(!log.exists());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn failed_host_exposes_diagnostics_and_cleans_log() {
        let mut command = std::process::Command::new("/bin/ls");
        command.arg("/trinix-test-nonexistent-path");
        let mut host = super::HostProcess::spawn(command).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !host.exited().unwrap() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(host
            .diagnostics()
            .unwrap()
            .contains("trinix-test-nonexistent-path"));
        let log = host.log_path.clone();
        drop(host);
        assert!(!log.exists());
    }
    #[test]
    fn host_configuration_rejects_invalid_endpoint_and_limits() {
        for address in ["", "example.org:5000", "127.0.0.1:0", "127.0.0.1:99999"] {
            assert!(validate(address, 30, 4000).is_err());
        }
        assert!(validate("127.0.0.1:5000", 0, 4000).is_err());
        assert!(validate("127.0.0.1:5000", 241, 4000).is_err());
        assert!(validate("127.0.0.1:5000", 30, 0).is_err());
        assert!(validate("127.0.0.1:5000", 30, 1_000_001).is_err());
        assert!(validate("127.0.0.1:5000", 30, 4000).is_ok());
        assert!(validate("[::1]:5000", 30, 4000).is_ok());
    }
}
