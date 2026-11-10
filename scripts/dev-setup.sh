#!/bin/bash
# One-shot developer environment for Debian/Ubuntu-family hosts.
# Fedora: sudo dnf install libvirt-devel mingw64-gcc pkgconf-pkg-config
set -euo pipefail
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libvirt-dev gcc-mingw-w64-x86-64 pciutils
rustup target add x86_64-pc-windows-gnu
cargo build --workspace && cargo test --workspace
echo "Build env OK. For the real VM stack (Phase 0) also run: scripts/iommu-check.sh and see docs/PHASE0.md"
