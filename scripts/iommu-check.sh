#!/bin/bash
# Phase 0 hardware check (from the project plan). Run on Linux, ~5 minutes per machine.
# arbiter-probe automates these same checks.
shopt -s nullglob
echo "== CPU virtualization (must be > 0) =="
grep -E -c '(vmx|svm)' /proc/cpuinfo
echo "== IOMMU in kernel log =="
sudo dmesg | grep -i -e DMAR -e IOMMU | head
echo "== IOMMU groups =="
for g in /sys/kernel/iommu_groups/*/; do
  echo "IOMMU Group ${g##*/}:"
  for d in $g/devices/*; do
    echo -e "\t$(lspci -nns ${d##*/})"
  done
done
echo "Pass: the dGPU (VGA) + its audio function are alone in their group; the iGPU is a separate VGA controller."
