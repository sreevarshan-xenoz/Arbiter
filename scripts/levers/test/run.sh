#!/usr/bin/env bash
# Dry-run tests: verify the exact virsh/systemctl commands each lever script issues, using fakes.
# Does NOT prove the commands work on a real VM — that is what Phase 0 hardware testing is for.
cd "$(dirname "$0")/.."
export VIRSH="$PWD/test/fake-virsh" SYSTEMCTL="$PWD/test/fake-systemctl" DRY_RUN=1 FAKE_LOG; FAKE_LOG=$(mktemp)
pass=0; fail=0
expect() { # <desc> <substring in log> -- <cmd...>
  local desc=$1 want=$2; shift 3; : > "$FAKE_LOG"
  out=$("$@" 2>&1); rc=$?
  if [ $rc -eq 0 ] && { [ -z "$want" ] || echo "$out" | grep -qF -- "$want"; }; then pass=$((pass+1)); echo "ok   $desc"
  else fail=$((fail+1)); echo "FAIL $desc (rc=$rc, wanted: $want)"; echo "$out" | sed 's/^/     /'; fi
}
rejects() { local desc=$1; shift 2; out=$("$@" 2>&1) && { fail=$((fail+1)); echo "FAIL $desc (should have failed)"; } || { pass=$((pass+1)); echo "ok   $desc"; }; }

expect "pin all 4 vCPUs"          "vcpupin arbiter-win 3 2-5 --live" -- ./vcpu-pin.sh pin 2-5
expect "pin emulator threads"     "emulatorpin arbiter-win 0-1 --live" -- env EMULATOR_CPUS=0-1 ./vcpu-pin.sh pin 2-5
rejects "bad cpulist"             -- ./vcpu-pin.sh pin "2;rm -rf /"
expect "cpu weight"               "schedinfo arbiter-win --live --set cpu_shares=800" -- ./cpu-share.sh weight 800
expect "cpu cap 60%"              "vcpu_quota=60000" -- ./cpu-share.sh cap 60
rejects "cap > 100"               -- ./cpu-share.sh cap 150
expect "uncap"                    "vcpu_quota=-1" -- ./cpu-share.sh uncap
expect "host weight"              "set-property --runtime user.slice CPUWeight=200" -- ./cpu-share.sh host-weight 200
expect "fence host"               "set-property --runtime system.slice AllowedCPUs=0-3" -- ./host-fence.sh fence 0-3
expect "unfence"                  "init.scope AllowedCPUs=" -- ./host-fence.sh unfence
expect "balloon set 6144"         "setmem arbiter-win 6291456 --live" -- ./balloon.sh set 6144
rejects "balloon above max"       -- ./balloon.sh set 20000
rejects "balloon with hostdev"    -- env FAKE_HOSTDEV=1 ./balloon.sh set 4096
expect "balloon stats"            "dommemstat arbiter-win --period 1 --live" -- ./balloon.sh stats-on
expect "virtio-mem show"          "requested=2048 MiB  plugged=2048 MiB" -- ./virtio-mem.sh show
expect "virtio-mem resize"        "update-memory-device arbiter-win --alias ua-vmem0 --requested-size 4194304 --live" -- ./virtio-mem.sh resize 4096
expect "virtio-mem wait converged" "converged" -- env FAKE_CUR_MIB=4096 ./virtio-mem.sh wait 4096 2
rejects "virtio-mem wait timeout" -- ./virtio-mem.sh wait 4096 1
rejects "virtio-mem bad alias"    -- env VMEM_ALIAS=nope ./virtio-mem.sh show
expect "pause"                    "suspend arbiter-win" -- ./vm-power.sh pause
expect "managed save (no hostdev)" "managedsave arbiter-win" -- ./vm-power.sh save
rejects "managed save w/ hostdev" -- env FAKE_HOSTDEV=1 ./vm-power.sh save
expect "status runs"              "## virtio-mem" -- ./status.sh
echo; echo "$pass passed, $fail failed"; rm -f "$FAKE_LOG"; [ $fail -eq 0 ]
