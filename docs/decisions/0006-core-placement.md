# 0006 — Guest cores are whole physical cores, counted as logical CPUs
**Status:** accepted (unit still open, see below)
- Guest vCPUs go to whole physical cores (all SMT siblings together), highest core first; CPU 0 stays with the host longest.
  A plain "highest CPU numbers" split shares every physical core on Intel laptops, where siblings are numbered (n, n+N/2).
- Online CPUs and siblings come from sysfs, not `available_parallelism()`, which shrinks under our own fencing.
- Guest cores are capped at the domain's vCPU count (fixed at boot): pinning onto more cores helps nobody and starves Linux.
- **Open:** rails and profiles still count *logical* CPUs, so "host ≥ 2" can mean one physical core on an SMT machine.
  Decide whether rules/rails/report use physical cores before benchmarks. Hybrid P/E-core CPUs are not handled yet
  (the highest-numbered cores there are usually E-cores).
