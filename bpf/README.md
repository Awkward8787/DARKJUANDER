# BPF

This directory is reserved for later UMBRA probes.

No BPF program is built or loaded here. UM-12 (page-cache tracing) and UM-16
(`scx_umbra`) are not in this tree. The router reads PSI from `/proc/pressure`,
which does not need BPF. Unprivileged BPF is turned off in
`image/sysctl.d/90-darkjuander.conf`.
