#!/usr/bin/env bash
# LEVER 5 — idle/sleep handling: pause, resume, managed save.
# managed save is IMPOSSIBLE with a passed-through device ("cannot migrate a domain with hostdev"),
# so on Tier 1/2 use pause (RAM stays in use) or hibernate from inside Windows.
#   virsh suspend|resume <dom> ; virsh managedsave <dom> ; virsh start <dom>
. "$(dirname "$0")/common.sh"
usage() { cat <<U
usage: $0 state | pause | resume | save | restore
  pause     freeze vCPUs, keep RAM  (Tier 0/1/2)
  resume    thaw a paused VM
  save      managed save to disk, frees RAM (Tier 0 only; refuses with passthrough)
  restore   start the domain again from its managed save
U
exit 1; }

case "${1:-}" in
  state)   v domstate "$DOMAIN"; v dominfo "$DOMAIN" | grep -i 'managed save' || true ;;
  pause)   preflight; s=$(now_ms); vact suspend "$DOMAIN"; echo "paused in $(( $(now_ms) - s )) ms" ;;
  resume)  s=$(now_ms); vact resume "$DOMAIN"; echo "resumed in $(( $(now_ms) - s )) ms" ;;
  save)    preflight
           has_hostdev && die "domain has <hostdev>: libvirt cannot managed-save it. Use 'pause' or hibernate Windows from inside"
           s=$(now_ms); vact managedsave "$DOMAIN"; echo "saved in $(( $(now_ms) - s )) ms" ;;
  restore) s=$(now_ms); vact start "$DOMAIN"; echo "restored in $(( $(now_ms) - s )) ms (measure 'click to window' separately)" ;;
  *) usage ;;
esac
