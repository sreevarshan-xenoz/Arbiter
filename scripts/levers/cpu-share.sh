#!/usr/bin/env bash
# LEVER 2 — how much CPU *time* the VM gets (cgroup v2 cpu.weight / cpu.max via libvirt + systemd).
#   virsh schedinfo <dom> --live --set cpu_shares=N      (-> cpu.weight)
#   virsh schedinfo <dom> --live --set vcpu_period=.. --set vcpu_quota=..   (-> per-vCPU cpu.max)
#   systemctl set-property --runtime user.slice CPUWeight=N                  (host side weight)
. "$(dirname "$0")/common.sh"
usage() { cat <<U
usage: $0 show | weight <n> | cap <percent-per-vcpu> | uncap | host-weight <n> | host-weight-reset
  weight 800         VM cpu_shares (relative; default is 1024-ish under libvirt)
  cap 60             limit each vCPU thread to 60% of a core (period 100000us)
  uncap              remove the cap (quota -1)
  host-weight 200    CPUWeight for user.slice (default 100); higher = host wins contention
U
exit 1; }

case "${1:-}" in
  show)
    preflight
    v schedinfo "$DOMAIN" --live
    "$SYSTEMCTL" show user.slice machine.slice -p Id -p CPUWeight -p CPUQuotaPerSecUSec || true ;;
  weight)  [ $# -eq 2 ] || usage; valid_uint "$2"; preflight; vact schedinfo "$DOMAIN" --live --set "cpu_shares=$2" ;;
  cap)     [ $# -eq 2 ] || usage; valid_uint "$2"; [ "$2" -ge 1 ] && [ "$2" -le 100 ] || die "percent must be 1-100"
           preflight; vact schedinfo "$DOMAIN" --live --set vcpu_period=100000 --set "vcpu_quota=$(( $2 * 1000 ))" ;;
  uncap)   preflight; vact schedinfo "$DOMAIN" --live --set vcpu_quota=-1 ;;
  host-weight)       [ $# -eq 2 ] || usage; valid_uint "$2"; sact set-property --runtime user.slice "CPUWeight=$2" ;;
  host-weight-reset) sact set-property --runtime user.slice CPUWeight= ;;
  *) usage ;;
esac
