#!/usr/bin/env bash
# Shared helpers for the manual lever scripts (Phase 0).
# Rule of thumb from the plan: everything arbiterd will do must first be done by hand with these.
#
# Env:  ARBITER_DOMAIN (default arbiter-win)   ARBITER_URI (default qemu:///system)
#       DRY_RUN=1   print mutating commands instead of running them
#       VIRSH / SYSTEMCTL  override binaries (used by the dry-run tests)
set -euo pipefail

DOMAIN="${ARBITER_DOMAIN:-arbiter-win}"
CONNECT="${ARBITER_URI:-qemu:///system}"
DRY_RUN="${DRY_RUN:-0}"
VIRSH="${VIRSH:-virsh}"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
if [ "$(id -u)" -eq 0 ]; then SUDO=(); else SUDO=(sudo); fi

die()  { echo "error: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' not found in PATH"; }
now_ms() { date +%s%3N; }

# read-only virsh query (always runs, even in DRY_RUN)
v() { "$VIRSH" -c "$CONNECT" "$@"; }

# mutating command: echo it, run it unless DRY_RUN=1
act() {
  echo "+ $*" >&2
  [ "$DRY_RUN" = 1 ] && return 0
  "$@"
}
vact() { act "$VIRSH" -c "$CONNECT" "$@"; }
sact() { act "${SUDO[@]}" "$SYSTEMCTL" "$@"; }

valid_cpulist() {
  [[ "$1" =~ ^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$ ]] || die "bad cpu list '$1' (examples: 2-5 or 0,1,6,7)"
  local part a b
  local -a parts
  IFS=',' read -r -a parts <<< "$1"
  for part in "${parts[@]}"; do
    if [[ "$part" == *-* ]]; then
      a=${part%-*}; b=${part#*-}
      [ $((10#$a)) -le $((10#$b)) ] || die "descending cpu range '$part'"
    fi
  done
}
# No leading zeros: bash treats 010 as octal 8 and 08 as an arithmetic error.
valid_uint()    { [[ "$1" =~ ^(0|[1-9][0-9]*)$ ]] || die "expected a non-negative integer, got '$1'"; }
# Returns 0 = has hostdev, 1 = none, 2 = could not tell (callers MUST fail closed on 2).
# `grep -q` in a pipeline would SIGPIPE a large dumpxml; with pipefail that looks like "no hostdev".
has_hostdev() {
  local xml
  if ! xml=$(v dumpxml "$DOMAIN"); then
    echo "error: cannot read '$DOMAIN' XML; refusing to assume no passthrough" >&2
    return 2
  fi
  [[ "$xml" == *"<hostdev"* ]]
}
domain_running(){ [ "$(v domstate "$DOMAIN" | tr -d '[:space:]')" = "running" ]; }

preflight() {
  [ "$DRY_RUN" = 1 ] || need "$VIRSH"
  [ "$DRY_RUN" = 1 ] || domain_running || die "domain '$DOMAIN' is not running (set ARBITER_DOMAIN=...)"
}
