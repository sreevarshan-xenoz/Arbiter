# 0004 — Levers behind a trait with a mock
**Status:** accepted
Policy/control-loop work must not need KVM. All actuation goes through `arbiterd::levers::Levers`; `MockLevers` records calls for tests; the libvirt backend is behind the `libvirt` cargo feature. Each lever must also be reproducible by hand with `virsh`/`systemctl set-property`.
