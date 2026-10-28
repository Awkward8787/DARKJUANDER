# Benchmarks

Section 7.9 of the PRD lists targets. None of them have been measured on a VM.

The mock-engine scenes in `dj demo` check security properties: one resident
record, zero cross-domain cache hits, a blocked exfil, a PSI-driven model
switch, and a private span kept off the cloud engine. They do not measure
resident RSS, time to first token, or router latency on a booted guest.

An in-process unit test checks that `umbra::decide` returns in under 5 ms on
the CI machine. That is not the guest p95 target.

Do not record a number in this directory until a run on the UTM or QEMU guest
produces it.
