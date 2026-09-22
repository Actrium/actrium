# loran-packages

`loran-packages` is Loran's reusable implementation of the
`actr-packages/1` protocol role. It maps opaque Actr URI Targets to immutable
package release metadata and serves the corresponding `.actr` artifacts.

The component is independent of authority discovery and publisher:

- `/.well-known/actr` advertises its endpoint with `rel=packages`;
- the Publisher Registry manages publishers, keys, signing authority, and publication audit;
- `loran-packages` resolves Targets and distributes package bytes.

The default Loran daemon mounts the component at `/actr/packages`. A deployment
can run the component without a local authority document, or advertise another
compatible package service from its authority document.

Local-file Targets are configured under `[[services.packages.targets]]` with
an absolute `.actr` artifact path. The Target is matched exactly and never
translated into a filesystem path. Resolve responses bind the artifact size
and SHA-256 digest; download URLs carry that digest and are rejected if the
configured bytes change.

See the [Actr Package Resolution specification](../../actr/protocols/package-resolution.md).
