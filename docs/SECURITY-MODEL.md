# Security and threat model
Principle: **Windows is less trusted than the Linux host; nothing is reachable from outside the machine.**

| Surface | Risk | Mitigation |
|---|---|---|
| Agent channel (virtio-serial) | compromised Windows sends fake/malformed data | strict schema, size + rate limits, arbiterd never executes anything from Windows, safety rails cap any allocation Windows can trigger |
| `arbiter run --windows` | command path into Windows from Linux | socket owned by user, mode 0600; runs in logged-in user session, never SYSTEM; every command logged; disable-able in config |
| RDP / RemoteApp | exposed to LAN/internet | isolated libvirt network, host-only; NLA with random per-install password in Linux keyring; Windows firewall blocks other addresses |
| Shared folder | malware reads/encrypts Linux files | one dedicated folder (e.g. `~/Shared`), never `$HOME`; read-only option per share |
| Clipboard | secrets leak across OSes | per-direction toggle in dashboard; optional pause-sync hotkey |
| GPU / drivers | VFIO/SR-IOV bugs affect host | accepted + documented; keep kernel updated; Tier 0 smallest surface |
| Windows itself | malware | keep Defender/updates on; snapshot before risky installs |

## Security test list (run before each review)
- [ ] malformed / oversized / out-of-range agent messages do not crash or confuse arbiterd (unit tests in `arbiter-proto` and `arbiterd::channel` cover oversize, malformed, out-of-range, report-before-hello, wrong version and floods; still to do on a real VM)
- [ ] RDP port unreachable from another machine on the LAN
- [ ] shared folder scope is only the dedicated folder
- [ ] `arbiter run` socket is 0600 and commands are logged
