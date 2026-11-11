# 0007 — The Windows agent runs in the user's session, with overlapped I/O
**Status:** accepted (to be validated on a real VM)
- Not a Windows service: services run in session 0, where `GetForegroundWindow` and `GetLastInputInfo` don't see the user's
  desktop, and those are the signals the rules use. The agent starts at logon (Task Scheduler, "run only when user is logged on",
  normal privileges), which also matches the security model's "never SYSTEM". Cost: no data before logon or on the lock screen,
  which the controller already treats as "no data → hold".
- The virtio-serial port is opened with `FILE_FLAG_OVERLAPPED`. On a synchronous handle Windows serialises I/O per file object, so
  a read waiting for the host would block writes; reads use a timeout and a timed-out write ends the session.
- Re-handshake instead of strict "one `Hello`": the agent greets again after host silence, the host ignores un-greeted reports.
  A host restart then recovers in about 5 s without restarting the agent.
- Win32 via `windows-sys` (no wrapper crate). GPU (PDH counters) and the per-process list come later; arbiterd doesn't use them yet.
- Install (until `arbiter setup` exists):
  `schtasks /create /tn "Arbiter Agent" /sc onlogon /rl limited /tr "C:\Program Files\Arbiter\arbiter-agent.exe"`.
  It runs as a console program for now; a hidden window plus a log file comes with packaging.
