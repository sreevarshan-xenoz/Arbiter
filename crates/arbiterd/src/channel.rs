//! Host end of the agent channel. QEMU exposes the guest's virtio-serial port as a unix socket
//! (domain XML: `<channel type='unix'><source mode='bind' path='…'/><target type='virtio'
//! name='org.arbiter.agent.0'/></channel>`); arbiterd connects to it as a client.
//!
//! Windows is less trusted (docs/SECURITY-MODEL.md): every frame is size-capped by `arbiter_proto`,
//! nothing is accepted before a matching `Hello`, every report is range-validated, and a flood
//! drops the connection. Reports that arrive before `Hello` (arbiterd joined mid-stream) are ignored
//! unacked; the agent notices the silence and greets again, and a repeated `Hello` re-handshakes. Any error ends the session; the data then goes stale and the controller
//! holds. Nothing received here is ever executed — it only becomes `SideSignals`.

use crate::collector::signals_from_report;
use arbiter_policy::SideSignals;
use arbiter_proto::{
    read_frame, write_frame, AgentMessage, HostMessage, ProtoError, PROTOCOL_VERSION,
};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// More messages than this within one second ends the session (the agent sends one every 1-2 s).
pub const MAX_MSGS_PER_SEC: u32 = 10;
const RECONNECT_MIN: Duration = Duration::from_millis(500);
const RECONNECT_MAX: Duration = Duration::from_secs(10);

/// The latest valid guest report and when it arrived, shared with the control loop.
pub type Latest = Arc<Mutex<Option<(SideSignals, Instant)>>>;

#[derive(Debug, PartialEq)]
pub enum SessionError {
    VersionMismatch { agent: u32 },
    Invalid(String),
    RateLimited,
}

/// Protocol rules for one connection, independent of any socket.
pub struct Session {
    greeted: bool,
    window_start: Option<Instant>,
    window_count: u32,
}

impl Session {
    pub fn new() -> Self {
        Self {
            greeted: false,
            window_start: None,
            window_count: 0,
        }
    }

    /// Check one inbound message. Returns the reply to send (if any) and the signals to publish (if any).
    pub fn handle(
        &mut self,
        msg: AgentMessage,
        now: Instant,
    ) -> Result<(Option<HostMessage>, Option<SideSignals>), SessionError> {
        match self.window_start {
            Some(t) if now.duration_since(t) < Duration::from_secs(1) => {
                self.window_count += 1;
                if self.window_count > MAX_MSGS_PER_SEC {
                    return Err(SessionError::RateLimited);
                }
            }
            _ => {
                self.window_start = Some(now);
                self.window_count = 1;
            }
        }
        match msg {
            AgentMessage::Hello { protocol, .. } => {
                if protocol != PROTOCOL_VERSION {
                    return Err(SessionError::VersionMismatch { agent: protocol });
                }
                self.greeted = true;
                Ok((
                    Some(HostMessage::Welcome {
                        protocol: PROTOCOL_VERSION,
                    }),
                    None,
                ))
            }
            AgentMessage::Report(r) => {
                if !self.greeted {
                    return Ok((None, None)); // not greeted yet: ignore, the agent will say Hello
                }
                r.validate()
                    .map_err(|e| SessionError::Invalid(e.to_string()))?;
                Ok((Some(HostMessage::Ack), Some(signals_from_report(&r))))
            }
        }
    }
}

/// Why a connection ended.
#[derive(Debug)]
pub enum EndReason {
    Proto(ProtoError),
    Session(SessionError),
}

impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndReason::Proto(e) => write!(f, "{e}"),
            EndReason::Session(e) => write!(f, "protocol violation: {e:?}"),
        }
    }
}

/// Serve one connection until it fails or closes, publishing every accepted report to `latest`.
pub fn serve<S: Read + Write>(stream: &mut S, latest: &Latest) -> EndReason {
    let mut session = Session::new();
    loop {
        let msg: AgentMessage = match read_frame(stream) {
            Ok(m) => m,
            Err(e) => return EndReason::Proto(e),
        };
        match session.handle(msg, Instant::now()) {
            Ok((reply, signals)) => {
                if let Some(s) = signals {
                    *latest.lock().unwrap_or_else(|p| p.into_inner()) = Some((s, Instant::now()));
                }
                if let Some(r) = reply {
                    if let Err(e) = write_frame(stream, &r) {
                        return EndReason::Proto(e);
                    }
                }
            }
            Err(e) => return EndReason::Session(e),
        }
    }
}

/// Connect to the channel socket and keep serving it, reconnecting with backoff, forever.
/// Runs on its own thread; the control loop only ever reads `latest`.
pub fn spawn_client(path: PathBuf, latest: Latest) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut backoff = RECONNECT_MIN;
        loop {
            match UnixStream::connect(&path) {
                Ok(mut s) => {
                    eprintln!("[channel] connected to {}", path.display());
                    let started = Instant::now();
                    let why = serve(&mut s, &latest);
                    eprintln!("[channel] session ended: {why}");
                    if started.elapsed() > RECONNECT_MAX {
                        backoff = RECONNECT_MIN; // it was healthy for a while
                    }
                }
                Err(e) => eprintln!("[channel] cannot connect to {}: {e}", path.display()),
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(RECONNECT_MAX);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbiter_proto::AgentReport;

    fn hello(v: u32) -> AgentMessage {
        AgentMessage::Hello {
            protocol: v,
            agent_version: "test".into(),
        }
    }
    fn report(cpu: f32) -> AgentMessage {
        AgentMessage::Report(AgentReport {
            foreground: Some("devenv.exe".into()),
            cpu_percent: cpu,
            mem_used_mb: 6000,
            gpu_heavy: false,
            idle_secs: 0,
            processes: vec![],
        })
    }

    #[test]
    fn handshake_then_reports() {
        let (mut s, t) = (Session::new(), Instant::now());
        assert_eq!(
            s.handle(hello(PROTOCOL_VERSION), t).unwrap().0,
            Some(HostMessage::Welcome {
                protocol: PROTOCOL_VERSION
            })
        );
        let (reply, sig) = s.handle(report(80.0), t).unwrap();
        assert_eq!(reply, Some(HostMessage::Ack));
        let sig = sig.unwrap();
        assert_eq!((sig.cpu_percent, sig.mem_used_mb), (80.0, 6000));
        assert_eq!(sig.foreground.as_deref(), Some("devenv.exe"));
    }

    #[test]
    fn report_before_hello_is_ignored_not_published() {
        assert_eq!(
            Session::new().handle(report(10.0), Instant::now()),
            Ok((None, None))
        );
    }

    #[test]
    fn wrong_version_is_refused_and_a_second_hello_re_handshakes() {
        let t = Instant::now();
        assert_eq!(
            Session::new().handle(hello(PROTOCOL_VERSION + 1), t),
            Err(SessionError::VersionMismatch {
                agent: PROTOCOL_VERSION + 1
            })
        );
        let mut s = Session::new();
        s.handle(hello(PROTOCOL_VERSION), t).unwrap();
        assert_eq!(
            s.handle(hello(PROTOCOL_VERSION), t).unwrap().0,
            Some(HostMessage::Welcome {
                protocol: PROTOCOL_VERSION
            })
        );
    }

    #[test]
    fn out_of_range_report_is_refused() {
        let (mut s, t) = (Session::new(), Instant::now());
        s.handle(hello(PROTOCOL_VERSION), t).unwrap();
        assert!(matches!(
            s.handle(report(250.0), t),
            Err(SessionError::Invalid(_))
        ));
    }

    #[test]
    fn flood_is_cut_off_but_normal_rate_is_fine() {
        let (mut s, t0) = (Session::new(), Instant::now());
        s.handle(hello(PROTOCOL_VERSION), t0).unwrap();
        // one report per second for a minute
        for i in 1..60 {
            s.handle(report(10.0), t0 + Duration::from_secs(i)).unwrap();
        }
        let t = t0 + Duration::from_secs(100);
        for _ in 0..MAX_MSGS_PER_SEC {
            s.handle(report(10.0), t).unwrap();
        }
        assert_eq!(s.handle(report(10.0), t), Err(SessionError::RateLimited));
    }

    #[test]
    fn serves_a_real_socket_and_publishes_reports() {
        let (mut agent, mut host) = UnixStream::pair().unwrap();
        let latest: Latest = Arc::default();
        let l2 = latest.clone();
        let server = std::thread::spawn(move || serve(&mut host, &l2));

        write_frame(&mut agent, &hello(PROTOCOL_VERSION)).unwrap();
        let w: HostMessage = read_frame(&mut agent).unwrap();
        assert!(matches!(w, HostMessage::Welcome { .. }));
        write_frame(&mut agent, &report(42.0)).unwrap();
        let a: HostMessage = read_frame(&mut agent).unwrap();
        assert_eq!(a, HostMessage::Ack);
        assert_eq!(
            latest.lock().unwrap().as_ref().map(|(s, _)| s.cpu_percent),
            Some(42.0)
        );

        drop(agent); // guest goes away: session ends, last report stays (and will go stale)
        assert!(matches!(server.join().unwrap(), EndReason::Proto(_)));
        assert!(latest.lock().unwrap().is_some());
    }

    #[test]
    fn garbage_from_the_guest_ends_the_session_without_publishing() {
        let (mut agent, mut host) = UnixStream::pair().unwrap();
        let latest: Latest = Arc::default();
        let l2 = latest.clone();
        let server = std::thread::spawn(move || serve(&mut host, &l2));
        agent.write_all(&u32::MAX.to_be_bytes()).unwrap(); // claims a 4 GiB frame
        assert!(matches!(
            server.join().unwrap(),
            EndReason::Proto(ProtoError::FrameTooLarge(_))
        ));
        assert!(latest.lock().unwrap().is_none());
    }

    #[test]
    fn client_reconnects_after_the_socket_appears() {
        let dir = std::env::temp_dir().join(format!("arbiter-chan-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.sock");
        let _ = std::fs::remove_file(&path);
        let latest: Latest = Arc::default();
        spawn_client(path.clone(), latest.clone()); // nothing listening yet: it retries

        std::thread::sleep(Duration::from_millis(100));
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let (mut agent, _) = listener.accept().unwrap();
        write_frame(&mut agent, &hello(PROTOCOL_VERSION)).unwrap();
        let _: HostMessage = read_frame(&mut agent).unwrap();
        write_frame(&mut agent, &report(55.0)).unwrap();
        let _: HostMessage = read_frame(&mut agent).unwrap();
        assert_eq!(
            latest.lock().unwrap().as_ref().map(|(s, _)| s.cpu_percent),
            Some(55.0)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
