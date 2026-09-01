# Loran

Loran is Actrium's default and reference shore-side service suite. It provides
the service composition around Actr peers: signaling, STUN/TURN, signing,
discovery, control, administration, and observability.

`config/` and `deploy/` define how the suite is assembled and operated. The
main daemon is under `../crates/loran-server/`; reusable services live in the
other `../crates/loran-*` directories. Their published Rust package and binary
names still use Actrix compatibility names until the separate semantic rename
is completed.
