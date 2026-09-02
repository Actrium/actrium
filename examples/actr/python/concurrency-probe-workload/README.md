<!-- SPDX-License-Identifier: Apache-2.0 -->

# concurrency-probe-workload (Python)

Generated Python workload that exercises the resident `Store::run_concurrent`
path of the WASM V2 kernel through the **generated** Python codegen +
componentization pipeline (#430). It mirrors the in-flight probe contract of
the hand-built Rust fixture (`crates/actr-hyper/tests/wasm_actor_fixture`), so the
M5 scheduler properties can be asserted against a real generated guest:

* `concurrency.ProbeService.InflightProbe` — bumps a per-instance in-flight
  counter, parks on the host-gated `test/gate` host-import crossing, and
  reports the peak concurrency (`max_seen`) this instance observed.
* `concurrency.ProbeService.BoomAfterAwait` — parks on `test/double_impl`,
  then raises, fault-injecting a guest trap with siblings in flight.

The counters are module-level globals: in a Python wasm component they live
in the linear memory, so a rebuilt instance (post-trap) starts from zero —
the same basis-agnostic semantics as the Rust fixture's per-instance fields.

## Regenerate

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
actr deps install
actr gen -l python --input protos --output generated
```

## Build

```bash
./build.sh
```

## E2E

`crates/actr-hyper/tests/e2e_python_generated_concurrency.rs` builds this component
and drives it through the conflict-key scheduler + interleaved runner:

```bash
ACTR_PYTHON_CONCURRENCY_E2E=1 \
cargo test -p actr-hyper --features wasm-engine,test-utils \
    --test e2e_python_generated_concurrency -- --ignored --test-threads=1
```
