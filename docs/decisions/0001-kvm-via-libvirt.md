# 0001 — Use KVM, driven only through libvirt
**Status:** accepted
Two unmodified kernels cannot share a machine without a hypervisor layer. Writing one is out of scope; static partitioners (Jailhouse/Bao) can't resize at runtime; Xen is valid but KVM has the best Windows support (virtio-win, VFIO, Looking Glass) and tooling.
arbiterd talks to the hypervisor only via libvirt, keeping a Xen backend possible later. KVM is the reference implementation.
