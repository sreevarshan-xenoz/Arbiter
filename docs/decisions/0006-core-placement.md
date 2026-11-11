# 0006 — Allocations count whole physical cores
**Status:** accepted
- The allocation unit is a **physical core** with all its SMT siblings. Profiles (`guest_cores`), rails and the report count
  physical cores; the levers receive the matching logical CPU lists.
- Guest cores are taken highest core first; CPU 0 stays with the host longest. A plain "highest CPU numbers" split shares
  every physical core on Intel laptops, where siblings are numbered (n, n+N/2).
- Rails: Linux keeps **≥ 2 physical cores**; Windows keeps enough cores for **≥ 2 logical CPUs** (1 core with SMT, 2 without).
  "≥ 2 + ≥ 2 physical" would freeze a 4-core laptop at a fixed 2/2 split; this way it moves between Windows 1–2 / Linux 2–3.
- Guest cores are capped at the cores needed to hold the domain's vCPUs (fixed at boot): more helps nobody and starves Linux.
- Online CPUs and siblings come from sysfs, not `available_parallelism()`, which shrinks under our own fencing.
- Profile values are absolute and get clamped per machine, so one profile behaves differently on 4- and 8-core hosts.
  Revisit (e.g. fractions of the machine) if that gets confusing in the user study.
- Not handled yet: hybrid P/E-core CPUs (the highest-numbered cores there are usually E-cores, which Windows would get first).
