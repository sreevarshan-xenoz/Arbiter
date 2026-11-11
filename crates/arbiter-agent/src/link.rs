//! The agent's side of the conversation: when to say `Hello`, and the per-tick session loop.
//!
//! The agent re-sends `Hello` until it is welcomed, and again whenever the host goes quiet for
//! `HOST_SILENT` (arbiterd restarted or reconnected). The host acks every report, so silence means
//! nobody is listening. Replies are read with a timeout, never a blocking read, so a missing host
//! can't wedge the agent.

use crate::port::Port;
use crate::report::{Probe, ReportBuilder};
use arbiter_proto::{write_frame, AgentMessage, FrameBuf, HostMessage, PROTOCOL_VERSION};
use std::io;
use std::time::{Duration, Instant};

pub const HELLO_RETRY: Duration = Duration::from_secs(3);
pub const HOST_SILENT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct Link {
    welcomed: bool,
    last_hello: Option<Instant>,
    last_heard: Option<Instant>,
}

impl Link {
    pub fn should_hello(&mut self, now: Instant) -> bool {
        if self.welcomed
            && self
                .last_heard
                .is_some_and(|t| now.duration_since(t) >= HOST_SILENT)
        {
            self.welcomed = false; // host went away; greet whoever is there now
        }
        !self.welcomed
            && self
                .last_hello
                .is_none_or(|t| now.duration_since(t) >= HELLO_RETRY)
    }

    pub fn hello_sent(&mut self, now: Instant) {
        self.last_hello = Some(now);
    }

    pub fn heard(&mut self, msg: &HostMessage, now: Instant) -> Result<(), String> {
        if let HostMessage::Welcome { protocol } = msg {
            if *protocol != PROTOCOL_VERSION {
                return Err(format!(
                    "host speaks protocol v{protocol}, this agent v{PROTOCOL_VERSION}"
                ));
            }
            self.welcomed = true;
        }
        self.last_heard = Some(now);
        Ok(())
    }

    pub fn welcomed(&self) -> bool {
        self.welcomed
    }
}

fn send(port: &mut dyn Port, msg: &AgentMessage) -> io::Result<()> {
    let mut frame = Vec::new();
    write_frame(&mut frame, msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    port.send(&frame)
}

/// Report once per `tick` until the port fails (`Err`) or `max_ticks` is reached (`Ok`).
pub fn run_session(
    port: &mut dyn Port,
    probe: &mut dyn Probe,
    tick: Duration,
    max_ticks: Option<u64>,
) -> io::Result<()> {
    let (mut link, mut reports, mut frames) = (
        Link::default(),
        ReportBuilder::default(),
        FrameBuf::default(),
    );
    let mut buf = [0u8; 4096];
    let mut n = 0u64;
    loop {
        let start = Instant::now();
        if link.should_hello(start) {
            send(
                port,
                &AgentMessage::Hello {
                    protocol: PROTOCOL_VERSION,
                    agent_version: env!("CARGO_PKG_VERSION").into(),
                },
            )?;
            link.hello_sent(start);
        }
        send(port, &AgentMessage::Report(reports.build(probe.sample())))?;

        // Wait out the rest of the tick reading replies.
        let deadline = start + tick;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let k = port.recv(&mut buf, deadline - now)?;
            frames.push(&buf[..k]);
            while let Some(m) = frames
                .pop::<HostMessage>()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            {
                let was = link.welcomed();
                link.heard(&m, Instant::now())
                    .map_err(|e| io::Error::new(io::ErrorKind::Unsupported, e))?;
                if !was && link.welcomed() {
                    eprintln!("[agent] welcomed by arbiterd");
                }
            }
        }
        n += 1;
        if max_ticks.is_some_and(|m| n >= m) {
            return Ok(());
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::port::UnixPort;
    use crate::report::FakeProbe;
    use arbiter_proto::read_frame;
    use std::os::unix::net::UnixStream;

    #[test]
    fn hello_until_welcomed_then_only_when_host_goes_quiet() {
        let (mut l, t) = (Link::default(), Instant::now());
        assert!(l.should_hello(t));
        l.hello_sent(t);
        assert!(!l.should_hello(t + Duration::from_secs(1)));
        assert!(l.should_hello(t + HELLO_RETRY)); // no answer: try again
        l.hello_sent(t + HELLO_RETRY);

        let w = t + Duration::from_secs(4);
        l.heard(
            &HostMessage::Welcome {
                protocol: PROTOCOL_VERSION,
            },
            w,
        )
        .unwrap();
        for s in 1..60 {
            l.heard(&HostMessage::Ack, w + Duration::from_secs(s))
                .unwrap();
            assert!(!l.should_hello(w + Duration::from_secs(s)));
        }
        // host stops acking (arbiterd restarted)
        assert!(l.should_hello(w + Duration::from_secs(59) + HOST_SILENT));
        assert!(!l.welcomed());
    }

    #[test]
    fn wrong_protocol_version_is_an_error() {
        let mut l = Link::default();
        assert!(l
            .heard(
                &HostMessage::Welcome {
                    protocol: PROTOCOL_VERSION + 1
                },
                Instant::now()
            )
            .is_err());
    }

    /// Minimal stand-in for arbiterd: answers Hello with Welcome and reports with Ack.
    /// Hands the socket back so it stays open until the agent's session is over.
    fn host(mut s: UnixStream, msgs: usize) -> (Vec<AgentMessage>, UnixStream) {
        let mut got = Vec::new();
        for _ in 0..msgs {
            let m: AgentMessage = read_frame(&mut s).unwrap();
            let reply = match &m {
                AgentMessage::Hello { .. } => HostMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                },
                AgentMessage::Report(_) => HostMessage::Ack,
            };
            write_frame(&mut s, &reply).unwrap();
            got.push(m);
        }
        (got, s)
    }

    #[test]
    fn session_says_hello_once_then_reports_every_tick() {
        let (a, h) = UnixStream::pair().unwrap();
        let host = std::thread::spawn(move || host(h, 4));
        let mut port = UnixPort::new(a);
        let mut probe = FakeProbe::new(Some("devenv.exe".into()), 90, 4000);
        run_session(&mut port, &mut probe, Duration::from_millis(50), Some(3)).unwrap();
        let (got, _open) = host.join().unwrap();
        assert!(matches!(
            got[0],
            AgentMessage::Hello {
                protocol: PROTOCOL_VERSION,
                ..
            }
        ));
        assert!(got[1..]
            .iter()
            .all(|m| matches!(m, AgentMessage::Report(r) if r.validate().is_ok())));
        let AgentMessage::Report(last) = &got[3] else {
            panic!()
        };
        assert_eq!(
            (last.cpu_percent, last.foreground.as_deref()),
            (90.0, Some("devenv.exe"))
        );
    }

    #[test]
    fn host_disappearing_ends_the_session() {
        let (a, h) = UnixStream::pair().unwrap();
        drop(h);
        let mut port = UnixPort::new(a);
        let mut probe = FakeProbe::new(None, 5, 1000);
        assert!(run_session(&mut port, &mut probe, Duration::from_millis(20), None).is_err());
    }
}
