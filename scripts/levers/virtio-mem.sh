#!/usr/bin/env bash
# LEVER 4b — RAM via virtio-mem (works alongside VFIO via coordinated discards). Windows driver is young:
# open MSI-X bug (virtio-win #1679, fix PR 1710). Grow likely works; shrink partial. THIS IS THE PHASE 0 TEST.
# Domain needs the config in virtio-mem-device.xml first (maxMemory, NUMA, shared memory backing, device).
#   virsh update-memory-device <dom> --alias <ua-name> --requested-size <KiB> --live
. "$(dirname "$0")/common.sh"
ALIAS="${VMEM_ALIAS:-ua-vmem0}"
usage() { cat <<U
usage: $0 show | resize <MiB> | wait <MiB> [timeout_s]
  show           requested vs plugged ("current") size of the virtio-mem device
  resize 4096    request 4096 MiB of hot-plugged RAM (not total RAM: just the virtio-mem part)
  wait 4096 30   poll until plugged == requested; prints elapsed = RAM reaction time (target < 10 s)
env: VMEM_ALIAS (default $ALIAS)
U
exit 1; }

sizes() { # -> "<requested_kib> <current_kib>"
  v dumpxml "$DOMAIN" | python3 -c '
import sys, xml.etree.ElementTree as ET
alias = sys.argv[1]
U = {"b":1/1024,"bytes":1/1024,"KB":1000/1024,"k":1,"KiB":1,"MB":1e6/1024,"M":1024,"MiB":1024,"GB":1e9/1024,"G":1048576,"GiB":1048576}
for m in ET.fromstring(sys.stdin.read()).iter("memory"):
    if m.get("model") != "virtio-mem": continue
    a = m.find("alias")
    if alias and (a is None or a.get("name") != alias): continue
    t = m.find("target")
    def kib(tag):
        e = t.find(tag)
        return int(int(e.text) * U[e.get("unit", "KiB")]) if e is not None else -1
    print(kib("requested"), kib("current")); break
else:
    sys.exit(3)' "$ALIAS" || die "no virtio-mem device with alias '$ALIAS' (see virtio-mem-device.xml)"
}

case "${1:-}" in
  show)   preflight; read -r req cur < <(sizes); echo "requested=$(( req / 1024 )) MiB  plugged=$(( cur / 1024 )) MiB" ;;
  resize) [ $# -eq 2 ] || usage; valid_uint "$2"; preflight
          vact update-memory-device "$DOMAIN" --alias "$ALIAS" --requested-size "$(( $2 * 1024 ))" --live ;;
  wait)   [ $# -ge 2 ] || usage; valid_uint "$2"; preflight; to="${3:-30}"; s=$(now_ms)
          while :; do
            read -r req cur < <(sizes); el=$(( $(now_ms) - s ))
            if [ "$cur" -eq "$(( $2 * 1024 ))" ]; then echo "converged: plugged=$(( cur / 1024 )) MiB after ${el} ms"; exit 0; fi
            [ "$el" -lt $(( to * 1000 )) ] || { echo "TIMEOUT after ${el} ms: requested=$(( req / 1024 )) plugged=$(( cur / 1024 )) MiB (MSI-X bug? try MSI-X off)" >&2; exit 2; }
            sleep 0.5
          done ;;
  *) usage ;;
esac
