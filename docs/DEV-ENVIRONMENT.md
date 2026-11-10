# Development environment

Two layers, deliberately separate:

## Layer 1 — code (works anywhere, including CI and cloud sandboxes)
Rust stable, `libvirt-dev`, `pkg-config`, MinGW (to cross-compile the Windows agent).
`scripts/dev-setup.sh` installs all of it on Debian/Ubuntu. Then:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p arbiterd --features libvirt          # real libvirt backend
cargo build -p arbiter-agent --target x86_64-pc-windows-gnu
```
All lever code goes behind the `Levers` trait with a `MockLevers`, so the policy/control loop is testable with no hypervisor.

## Layer 2 — real VM stack (needs real hardware, Phase 0)
Needs a Linux host (plan says Fedora 41+ or Arch, one distro for everyone) with VT-x/AMD-V (+ VT-d/AMD-Vi for GPU tiers):
`qemu-kvm libvirt virt-manager swtpm edk2-ovmf virtio-win` + a Windows 10/11 Pro or evaluation ISO.
Cloud containers/most VMs lack `/dev/kvm`, so this layer cannot run there.
Start with `scripts/iommu-check.sh` and `cargo run -p arbiter-probe`.

## Status of this repo's environment (as set up on 10 Nov 2026)
- Builds clean, 16 unit tests pass, clippy/fmt clean, libvirt feature and Windows target both build.
- The cloud sandbox used for the initial scaffold has **no KVM, no VT-x flag**: probe correctly reports `Unsupported` there.
