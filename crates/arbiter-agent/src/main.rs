//! Windows guest agent: reports the foreground app, CPU, memory and input idle time to arbiterd
//! over virtio-serial every tick. It never changes any allocation itself and executes nothing it
//! receives. Runs in the logged-in user's session (see `win.rs` for why not as a service).
//!
//! On Linux it can stand in for "QEMU + Windows" with `--dev-listen`, so arbiterd's whole loop can
//! be exercised without a VM.

mod link;
mod port;
mod report;
#[cfg(windows)]
mod win;

use std::time::Duration;

const REOPEN_DELAY: Duration = Duration::from_secs(2);

struct Args {
    port: Option<String>,
    dev_listen: Option<String>,
    tick_ms: u64,
    ticks: Option<u64>,
    fake_foreground: Option<String>,
    fake_cpu: u64,
    fake_mem_mb: u64,
}

const USAGE: &str = "usage: arbiter-agent [--port PATH] [--tick-ms N] [--ticks N]
       arbiter-agent --dev-listen SOCKET [--fake-foreground NAME] [--fake-cpu PCT] [--fake-mem-mb N]  (Linux, no VM)";

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        port: None,
        dev_listen: None,
        tick_ms: 1000,
        ticks: None,
        fake_foreground: None,
        fake_cpu: 0,
        fake_mem_mb: 2048,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        let num = |v: String, name: &str| v.parse::<u64>().map_err(|_| format!("bad {name}"));
        match k.as_str() {
            "--port" => a.port = Some(val("--port")?),
            "--dev-listen" => a.dev_listen = Some(val("--dev-listen")?),
            "--tick-ms" => a.tick_ms = num(val("--tick-ms")?, "--tick-ms")?,
            "--ticks" => a.ticks = Some(num(val("--ticks")?, "--ticks")?),
            "--fake-foreground" => a.fake_foreground = Some(val("--fake-foreground")?),
            "--fake-cpu" => a.fake_cpu = num(val("--fake-cpu")?, "--fake-cpu")?,
            "--fake-mem-mb" => a.fake_mem_mb = num(val("--fake-mem-mb")?, "--fake-mem-mb")?,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    if a.tick_ms == 0 {
        return Err("--tick-ms must be > 0".into());
    }
    Ok(a)
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    eprintln!(
        "arbiter-agent {} protocol v{}",
        env!("CARGO_PKG_VERSION"),
        arbiter_proto::PROTOCOL_VERSION
    );
    let tick = Duration::from_millis(args.tick_ms);

    if let Some(path) = &args.dev_listen {
        #[cfg(unix)]
        return dev_listen(path, &args, tick);
        #[cfg(not(unix))]
        {
            eprintln!("--dev-listen {path}: only available on Linux");
            std::process::exit(2);
        }
    }

    #[cfg(windows)]
    {
        let path = args.port.as_deref().unwrap_or(win::DEFAULT_PORT);
        let mut probe = win::WinProbe;
        loop {
            match win::WinPort::open(path) {
                Ok(mut port) => {
                    eprintln!("[agent] opened {path}");
                    match link::run_session(&mut port, &mut probe, tick, args.ticks) {
                        Ok(()) => return,
                        Err(e) => eprintln!("[agent] session ended: {e}"),
                    }
                }
                Err(e) => eprintln!("[agent] cannot open {path}: {e}"),
            }
            std::thread::sleep(REOPEN_DELAY);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (&args.port, args.ticks, REOPEN_DELAY);
        eprintln!(
            "the real agent runs inside the Windows guest; on Linux use --dev-listen\n{USAGE}"
        );
        std::process::exit(2);
    }
}

/// Play QEMU + a Windows guest: bind `path` (as QEMU does for the domain's channel) and serve
/// every arbiterd connection with a fake probe.
#[cfg(unix)]
fn dev_listen(path: &str, args: &Args, tick: Duration) {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixListener;

    // Only ever remove a stale *socket*, never some other file at that path.
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_socket()) {
        let _ = std::fs::remove_file(path);
    }
    let listener = UnixListener::bind(path).unwrap_or_else(|e| {
        eprintln!("cannot bind {path}: {e}");
        std::process::exit(2)
    });
    eprintln!("[agent] dev mode: waiting for arbiterd on {path}");
    let mut probe = report::FakeProbe::new(
        args.fake_foreground.clone(),
        args.fake_cpu,
        args.fake_mem_mb,
    );
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        eprintln!("[agent] arbiterd connected");
        let mut port = port::UnixPort::new(stream);
        match link::run_session(&mut port, &mut probe, tick, args.ticks) {
            Ok(()) => break,
            Err(e) => eprintln!("[agent] session ended: {e}"),
        }
    }
    let _ = std::fs::remove_file(path);
}
