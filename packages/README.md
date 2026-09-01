# Packages

All reusable non-Rust implementation units live directly under this directory.
Use a component prefix to show ownership: `actr-*`, `actrium-*`, or `keel-*`.
Platform applications and Loran deployment composition stay at their component
roots. Keel is intentionally kept as the single `packages/keel` package until
real publishable boundaries justify splitting it.
