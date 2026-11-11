mod collector;
mod controller;
mod levers;

use arbiter_policy::{Allocation, Governor, Limits, Profile};
use collector::{Collector, ProcCollector};
use controller::Controller;
use std::time::{Duration, Instant};

const DEFAULT_PROFILE: &str = include_str!("../../../profiles/balanced.toml");

struct Args {
    profile: Option<String>,
    tick_ms: u64,
    ticks: Option<u64>,
    guest_max_ram_mb: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        profile: None,
        tick_ms: 1000,
        ticks: None,
        guest_max_ram_mb: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match k.as_str() {
            "--profile" => a.profile = Some(val("--profile")?),
            "--tick-ms" => a.tick_ms = val("--tick-ms")?.parse().map_err(|_| "bad --tick-ms")?,
            "--ticks" => a.ticks = Some(val("--ticks")?.parse().map_err(|_| "bad --ticks")?),
            "--guest-max-ram-mb" => {
                a.guest_max_ram_mb = Some(
                    val("--guest-max-ram-mb")?
                        .parse()
                        .map_err(|_| "bad --guest-max-ram-mb")?,
                )
            }
            "--check-libvirt" => {}
            "-h" | "--help" => return Err(
                "usage: arbiterd [--profile FILE] [--tick-ms N] [--ticks N] [--guest-max-ram-mb N]"
                    .into(),
            ),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    if a.tick_ms == 0 {
        return Err("--tick-ms must be > 0".into());
    }
    Ok(a)
}

fn total_ram_mb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| {
            m.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map_or(0, |kb| kb / 1024)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    #[cfg(feature = "libvirt")]
    if std::env::args().any(|a| a == "--check-libvirt") {
        match levers::libvirt_backend::list_domains() {
            Ok(d) => println!("libvirt ok, domains: {d:?}"),
            Err(e) => eprintln!("libvirt error: {e}"),
        }
        return;
    }

    let text = match &args.profile {
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("cannot read profile {p}: {e}");
            std::process::exit(2)
        }),
        None => DEFAULT_PROFILE.to_string(),
    };
    let profile = Profile::from_toml(&text).unwrap_or_else(|e| {
        eprintln!("invalid profile: {e}");
        std::process::exit(2)
    });

    let cores = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
    let ram = total_ram_mb();
    let guest_max = args.guest_max_ram_mb.unwrap_or(ram / 2).max(2048);
    let limits = Limits::new(cores, ram, guest_max);
    // Start from an even split; the libvirt backend will read the real state from the domain.
    let g = (cores / 2).max(2);
    let initial = arbiter_policy::clamp(
        Allocation {
            guest_cores: g,
            host_cores: cores - g,
            guest_ram_mb: guest_max / 2,
        },
        &limits,
        0,
    );

    eprintln!("arbiterd {} profile={} cores={cores} ram={ram}MiB guest_max={guest_max}MiB tick={}ms (dry-run levers)", env!("CARGO_PKG_VERSION"), profile.name, args.tick_ms);
    let mut collector = ProcCollector::new(profile.host_workloads.clone());
    let mut controller = Controller::new(profile, limits, Governor::default(), initial);
    let mut levers = levers::LogLevers;

    let start = Instant::now();
    let mut n = 0u64;
    loop {
        let snap = collector.collect();
        let decision = controller.tick(start.elapsed().as_millis() as u64, &snap, &mut levers);
        println!("{}", decision.to_json_line());
        n += 1;
        if args.ticks.is_some_and(|max| n >= max) {
            break;
        }
        std::thread::sleep(Duration::from_millis(args.tick_ms));
    }
}
