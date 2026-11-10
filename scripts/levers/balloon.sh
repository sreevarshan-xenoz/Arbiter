#!/usr/bin/env bash
# LEVER 4a — RAM via virtio-balloon. Tier 0 ONLY (no GPU passthrough): QEMU silently disables ballooning
# once VFIO pins guest memory. Needs the virtio-win balloon driver AND the Blnsvr service inside Windows,
# otherwise setmem appears to succeed and does nothing (plan, verification row 5).
#   virsh setmem <dom> <KiB> --live ;  virsh dommemstat <dom> --period 1 --live
. "$(dirname "$0")/common.sh"
usage() { cat <<U
usage: $0 show | stats-on | set <MiB>
  show        configured max, current balloon target, guest-reported stats
  stats-on    ask the guest for memory stats every second (needed for in-use numbers)
  set 6144    resize the live guest to 6144 MiB (must be <= domain max memory)
U
exit 1; }

case "${1:-}" in
  show)     preflight; v dominfo "$DOMAIN" | grep -i -E 'max memory|used memory'; v dommemstat "$DOMAIN" ;;
  stats-on) preflight; vact dommemstat "$DOMAIN" --period 1 --live ;;
  set)      [ $# -eq 2 ] || usage; valid_uint "$2"; preflight
            if has_hostdev && [ "${FORCE:-0}" != 1 ]; then
              die "domain has <hostdev> (passthrough): balloon is disabled by VFIO. Use virtio-mem.sh (FORCE=1 to try anyway)"
            fi
            max_kib=$(v dominfo "$DOMAIN" | awk -F: '/Max memory/ {gsub(/[^0-9]/,"",$2); print $2}')
            want_kib=$(( $2 * 1024 ))
            [ -z "$max_kib" ] || [ "$want_kib" -le "$max_kib" ] || die "$2 MiB exceeds domain max ($(( max_kib / 1024 )) MiB); balloon cannot grow past boot maximum"
            s=$(now_ms); vact setmem "$DOMAIN" "$want_kib" --live
            echo "setmem issued in $(( $(now_ms) - s )) ms; watch '$0 show' / Task Manager in the guest for the real effect" ;;
  *) usage ;;
esac
