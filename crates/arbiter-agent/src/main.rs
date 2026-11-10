//! Windows guest agent. Real Win32 collection (foreground window, process list,
//! PDH GPU counters) lands in Phase 1 behind `cfg(windows)`; the crate compiles
//! on Linux so CI and the protocol work stay unblocked.

fn main() {
    println!(
        "arbiter-agent {} protocol v{}",
        env!("CARGO_PKG_VERSION"),
        arbiter_proto::PROTOCOL_VERSION
    );
}
