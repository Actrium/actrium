# Native Runtime Artifacts

Place generated Windows native libraries here during local Windows builds.

Expected output:

```text
windows/native/actro_core.dll
```

The DLL is produced from `crates/actrium-core` and copied by the Windows target in
`scripts/actrium/build-actro-rust-bindings.sh`. Binary artifacts in this directory are
ignored; this README keeps the runtime artifact boundary visible in the repo.
