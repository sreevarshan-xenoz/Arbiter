#!/usr/bin/env bash
# LEVER 1 — which physical cores the Windows vCPUs run on (live, no reboot).
# Windows' vCPU *count* is fixed at boot (plan, verification row 1); we only move them.
#   virsh vcpupin <dom> <vcpu> <cpulist> --live
. "$(dirname "$0")/common.sh"
usage() { cat <<U
usage: $0 show | pin <cpulist> | reset
  show            current vCPU and emulator-thread pinning
  pin 2-5         pin ALL vCPUs to host cores 2-5 (EMULATOR_CPUS=0-1 also pins QEMU's emulator threads)
  reset           un-pin: allow all host cores again
U
exit 1; }

case "${1:-}" in
  show)
    preflight
    v vcpupin "$DOMAIN" --live
    v emulatorpin "$DOMAIN" --live || true ;;
  pin)
    [ $# -eq 2 ] || usage; valid_cpulist "$2"; preflight
    n=$(v vcpuinfo "$DOMAIN" | grep -c '^VCPU:') ; [ "$n" -gt 0 ] || die "could not read vCPU count"
    s=$(now_ms)
    for ((i = 0; i < n; i++)); do vact vcpupin "$DOMAIN" "$i" "$2" --live; done
    [ -n "${EMULATOR_CPUS:-}" ] && { valid_cpulist "$EMULATOR_CPUS"; vact emulatorpin "$DOMAIN" "$EMULATOR_CPUS" --live; }
    echo "pinned $n vCPU(s) to $2 in $(( $(now_ms) - s )) ms" ;;
  reset)
    preflight
    # Use the kernel's actual online CPU list (it can be sparse); re-pin vCPUs to all of them AND
    # widen the emulator pin, otherwise a previous EMULATOR_CPUS=... pin survives "reset".
    online=$(tr -d '\n' < /sys/devices/system/cpu/online 2>/dev/null || true)
    [ -n "$online" ] || die "cannot read /sys/devices/system/cpu/online"
    EMULATOR_CPUS="$online" exec "$0" pin "$online" ;;
  *) usage ;;
esac
