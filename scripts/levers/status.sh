#!/usr/bin/env bash
# One-shot snapshot of every lever's current state. Paste the output into docs/weekly/ when testing.
. "$(dirname "$0")/common.sh"
d=$(dirname "$0")
echo "### $(date -Is)  domain=$DOMAIN uri=$CONNECT"
echo "## host:  $(uname -r), cores=$(nproc --all), libvirt=$(v version 2>/dev/null | awk '/libvirt/ {print $NF; exit}')"
echo "## passthrough hostdev present: $(has_hostdev && echo yes || echo no)"
for s in vcpu-pin cpu-share host-fence balloon virtio-mem vm-power; do
  echo; echo "## $s"; case $s in vm-power) "$d/$s.sh" state ;; *) "$d/$s.sh" show ;; esac 2>&1 || echo "(failed: $?)"
done
