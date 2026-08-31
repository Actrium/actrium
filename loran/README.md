# Loran

Loran is Actrium's default and reference shore-side service suite. It provides
the service composition around Actr peers: signaling, STUN/TURN, signing,
discovery, control, administration, and observability.

`config/` and `deploy/` define how the suite is assembled and operated. The
daemon and reusable service implementations belong under `../crates/loran-*`,
so they can be tested and consumed without importing a deployment.
