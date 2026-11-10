mod levers;

fn main() {
    println!(
        "arbiterd {} (skeleton: control loop lands in Phase 1)",
        env!("CARGO_PKG_VERSION")
    );
    #[cfg(feature = "libvirt")]
    if std::env::args().any(|a| a == "--check-libvirt") {
        match levers::libvirt_backend::list_domains() {
            Ok(d) => println!("libvirt ok, domains: {d:?}"),
            Err(e) => eprintln!("libvirt error: {e}"),
        }
    }
}
