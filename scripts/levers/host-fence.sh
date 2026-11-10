#!/usr/bin/env bash
# LEVER 3 — keep host tasks OFF the cores reserved for Windows (what vfio-isolate does once at VM start;
# we do it at runtime). Needs cgroup v2 + systemd >= 244. --runtime means it vanishes on reboot.
#   systemctl set-property --runtime user.slice AllowedCPUs=<host cores>
. "$(dirname "$0")/common.sh"
UNITS=(user.slice system.slice init.scope)
usage() { cat <<U
usage: $0 show | fence <host-cpulist> | unfence
  fence 0-3    confine user.slice, system.slice, init.scope to cores 0-3 (leave 4-7 to the VM)
  unfence      remove the restriction
(machine.slice is NOT fenced, so the VM may still use every core; pin its vCPUs with vcpu-pin.sh)
U
exit 1; }

[ "$(stat -fc %T /sys/fs/cgroup 2>/dev/null)" = "cgroup2fs" ] || echo "warning: /sys/fs/cgroup is not cgroup v2; AllowedCPUs will not work" >&2
case "${1:-}" in
  show)    for u in "${UNITS[@]}"; do "$SYSTEMCTL" show "$u" -p Id -p AllowedCPUs -p EffectiveCPUs; done ;;
  fence)   [ $# -eq 2 ] || usage; valid_cpulist "$2"
           s=$(now_ms); for u in "${UNITS[@]}"; do sact set-property --runtime "$u" "AllowedCPUs=$2"; done
           echo "fenced host to $2 in $(( $(now_ms) - s )) ms" ;;
  unfence) for u in "${UNITS[@]}"; do sact set-property --runtime "$u" AllowedCPUs=; done ;;
  *) usage ;;
esac
