"""Bounded transition systems for the main model (n <= 2), data
generations, adversarial local clocks, and rollback resurrection.

All four models use bounded exhaustive search. They establish that bad states
are unreachable within the stated bounds; they are not unbounded inductive
proofs.

Structural rules prevent a vacuous "the guard is the model" check:
- executable guards come from semantics.py, shared with the Z3 layer;
- invariants() independently recomputes safety properties instead of reusing
  guard return values;
- adversarial transitions always attempt bad behavior and let the guard decide
  whether it proceeds. Replacing a guard with a bad definition in mutants.py
  must therefore trigger the corresponding invariant.

The main model is deliberately conservative, as documented alongside it:
- ActorStateDigest is represented as (revision, cfg, transfers). Revision is
  monotonic, and capability declarations are part of state identity (h_cap),
  so rolling content back still produces a new sid;
- Change does not globally erase published snapshots because no such atomic
  operation exists in a distributed system. Old snapshots remain in state and
  ready() rejects them by checking the current pointer (Thm 12 / A10);
- Retain and cross-Principal judgments are checked in theory.py and
  check_formal_model.py, so the main model does not carry constant dimensions.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass, replace
from enum import IntEnum
from typing import Iterable, Iterator

try:
    from . import semantics
except ImportError:  # pragma: no cover - direct script execution
    import semantics


class Gate(IntEnum):
    ACTIVE = 0
    SUSPENDED = 1


class Verdict(IntEnum):
    CONTROLLED = 0
    UNKNOWN = 1
    UNCONTROLLED = 2


# Capability declaration menu: empty, uncontrolled sink, or peer Actor.
TRANSFER_MENU: tuple[tuple[tuple[str, object], ...], ...] = (
    (),
    (("sink", -1),),
    (("actor", "peer"),),
)

MAX_REVISION = 2


@dataclass(frozen=True)
class Actor:
    principal: int
    revision: int
    cfg: int
    transfers: tuple[tuple[str, object], ...]
    gate: Gate
    keys_current: bool
    aa_current: bool
    verifier: str


def sid_of(actor: Actor) -> tuple:
    """Bounded ActorStateDigest stand-in with monotonic revision and h_cap."""
    return (actor.revision, actor.cfg, actor.transfers)


@dataclass(frozen=True)
class Snapshot:
    sids: tuple[tuple, ...]
    color: Verdict


@dataclass(frozen=True)
class State:
    actors: tuple[Actor, ...]
    app: Snapshot | None


def _peer_index(state: State, i: int) -> int | None:
    n = len(state.actors)
    if n < 2:
        return None
    return (i + 1) % n


def eval_actor(state: State, i: int) -> Verdict:
    """Classify same-Principal Actor targets and nonnegative sinks as C.

    All other targets are X. The complete E-Acc / E-C-Sink judgment relation
    lives in check_formal_model.judge_transfer.
    """
    actor = state.actors[i]
    acc = Verdict.CONTROLLED
    for kind, target in actor.transfers:
        if kind == "actor":
            peer = _peer_index(state, i)
            if peer is None or state.actors[peer].principal != actor.principal:
                acc = max(acc, Verdict.UNCONTROLLED)
            else:
                acc = max(acc, Verdict.CONTROLLED)
        elif kind == "sink":
            acc = max(
                acc,
                Verdict.UNCONTROLLED if target < 0 else Verdict.CONTROLLED,
            )
    return acc


def app_color(state: State) -> Verdict:
    return max(
        (eval_actor(state, i) for i in range(len(state.actors))),
        default=Verdict.CONTROLLED,
    )


def ready(state: State) -> bool:
    """A12: current pointer, all ACTIVE, and verdicts bound to current state."""
    if state.app is None:
        return False
    pointer_current = tuple(sid_of(a) for a in state.actors) == state.app.sids
    none_suspended = all(a.gate is Gate.ACTIVE for a in state.actors)
    aa_all_current = all(a.aa_current for a in state.actors)
    return bool(
        semantics.ready_ok(
            has_app=True,
            pointer_current=pointer_current,
            none_suspended=none_suspended,
        )
        and aa_all_current
    )


def displayed(state: State) -> Verdict | None:
    if not ready(state):
        return None
    return state.app.color


def depends(state: State, i: int, j: int) -> bool:
    if i == j:
        return False
    peer_i = _peer_index(state, i)
    peer_j = _peer_index(state, j)
    left = ("actor", "peer") in state.actors[i].transfers and peer_i == j
    right = ("actor", "peer") in state.actors[j].transfers and peer_j == i
    return left or right


def depends_on_suspended(state: State, i: int) -> bool:
    return any(
        peer.gate is Gate.SUSPENDED and depends(state, i, j)
        for j, peer in enumerate(state.actors)
        if j != i
    )


def exec_guard(state: State, i: int) -> bool:
    """Implementation guard for protected operations, replaceable by mutants.

    The Actor Grant is issued for the current state, so sid_match is true.
    explore_rollback models an attacker holding an old credential. Time
    semantics live in explore_clocks and the shared guard.
    """
    actor = state.actors[i]
    return bool(
        semantics.exec_gate_ok(
            gate_active=actor.gate is Gate.ACTIVE,
            keys_current=actor.keys_current,
            sid_match=semantics.cap_sid_match(sid_of(actor), sid_of(actor)),
            time_ok=True,
        )
        and not depends_on_suspended(state, i)
    )


def uncontrolled_exec_possible(state: State) -> bool:
    """Independently recompute whether an executable uncontrolled exit exists."""
    for i, actor in enumerate(state.actors):
        if not exec_guard(state, i):
            continue
        for kind, target in actor.transfers:
            if kind == "sink" and target < 0:
                return True
            if kind == "actor":
                peer = _peer_index(state, i)
                if peer is None or (
                    state.actors[peer].principal != actor.principal
                ):
                    return True
    return False


def _set_actor(state: State, i: int, actor: Actor, **state_kw) -> State:
    actors = list(state.actors)
    actors[i] = actor
    return replace(state, actors=tuple(actors), **state_kw)


def step_change(
    state: State, i: int, cfg: int, transfers: tuple[tuple[str, object], ...]
) -> State | None:
    actor = state.actors[i]
    if actor.revision >= MAX_REVISION:
        return None
    if cfg == actor.cfg and transfers == actor.transfers:
        return None
    updated = replace(
        actor,
        revision=actor.revision + 1,
        cfg=cfg,
        transfers=transfers,
        gate=Gate.SUSPENDED,
        keys_current=False,
        aa_current=False,
    )
    # Do not erase state.app. A distributed system cannot atomically delete all
    # old snapshot copies; ready() must reject them using the current pointer.
    return _set_actor(state, i, updated)


def step_attest(state: State, i: int) -> State | None:
    actor = state.actors[i]
    if actor.verifier == f"actor-{i}":
        return None
    if actor.aa_current:
        return None
    return _set_actor(state, i, replace(actor, aa_current=True))


def step_resume(state: State, i: int) -> State | None:
    actor = state.actors[i]
    if actor.gate is not Gate.SUSPENDED:
        return None
    if not actor.aa_current:
        return None
    return _set_actor(
        state, i, replace(actor, gate=Gate.ACTIVE, keys_current=True)
    )


def step_publish(state: State) -> State | None:
    leaves_ok = all(
        semantics.publish_leaf_ok(
            leaf_current=True,  # The leaf is derived from the current pointer.
            aa_matches_leaf=actor.aa_current,
            gate_active=actor.gate is Gate.ACTIVE,
        )
        for actor in state.actors
    )
    if not leaves_ok:
        return None
    snap = Snapshot(
        sids=tuple(sid_of(a) for a in state.actors),
        color=app_color(state),
    )
    if state.app == snap:
        return None
    return replace(state, app=snap)


def successors(state: State) -> Iterator[tuple[str, State]]:
    for i in range(len(state.actors)):
        for cfg in (0, 1):
            for menu_index, transfers in enumerate(TRANSFER_MENU):
                nxt = step_change(state, i, cfg, transfers)
                if nxt:
                    yield f"change-{i}-{cfg}-{menu_index}", nxt
        nxt = step_attest(state, i)
        if nxt:
            yield f"attest-{i}", nxt
        nxt = step_resume(state, i)
        if nxt:
            yield f"resume-{i}", nxt
    nxt = step_publish(state)
    if nxt:
        yield "publish", nxt


def invariants(state: State) -> list[str]:
    """Recompute safety invariants without reusing guard return values."""
    bad: list[str] = []
    shown = displayed(state)
    if shown is Verdict.CONTROLLED:
        if uncontrolled_exec_possible(state):
            bad.append("green-and-current-uncontrolled-exec")
        if any(a.gate is Gate.SUSPENDED for a in state.actors):
            bad.append("green-while-suspended")
        if state.app.color != app_color(state):
            bad.append("displayed-color-stale")
    for i, actor in enumerate(state.actors):
        if actor.gate is Gate.SUSPENDED and actor.keys_current:
            bad.append(f"suspended-has-current-keys-{i}")
        if actor.verifier == f"actor-{i}" and actor.aa_current:
            bad.append(f"self-signed-{i}")
        if depends_on_suspended(state, i) and exec_guard(state, i):
            bad.append(f"protected-while-depending-on-suspended-{i}")
    return bad


def initial(n: int = 2) -> State:
    actors = tuple(
        Actor(
            principal=0,
            revision=0,
            cfg=0,
            transfers=(),
            gate=Gate.ACTIVE,
            keys_current=True,
            aa_current=True,
            verifier="verifier",
        )
        for _ in range(n)
    )
    state = State(actors=actors, app=None)
    return replace(
        state,
        app=Snapshot(
            sids=tuple(sid_of(a) for a in actors), color=app_color(state)
        ),
    )


def _bfs(
    start,
    successor_fn,
    invariant_fn,
    limit: int,
    tag: dict[str, object],
) -> dict[str, object]:
    seen = set()
    queue = deque([start])
    violations: list[dict[str, object]] = []
    transitions = 0
    truncated = False
    while queue:
        if len(seen) >= limit:
            truncated = True
            break
        state = queue.popleft()
        if state in seen:
            continue
        seen.add(state)
        bad = invariant_fn(state)
        if bad:
            violations.append({"bad": bad, "state": str(state)})
            continue
        for _label, nxt in successor_fn(state):
            transitions += 1
            if nxt not in seen:
                queue.append(nxt)
    status = "PASS"
    if violations:
        status = "FAIL"
    elif truncated:
        # Silent truncation is unverified; reaching the limit must fail.
        status = "FAIL"
        violations.append({"bad": ["state-limit-truncated"], "state": ""})
    return {
        "status": status,
        "states": len(seen),
        "transitions": transitions,
        "violations": violations[:10],
        "truncated": truncated,
        "bounded": True,
        **tag,
    }


def explore(n: int = 2, limit: int = 400_000) -> dict[str, object]:
    return _bfs(
        initial(n),
        successors,
        invariants,
        limit,
        {"model": "main", "actors": n},
    )


# --- Data-generation model ---


@dataclass(frozen=True)
class GenState:
    sid: int
    gate: Gate
    keys_current: bool
    aa_current: bool
    transfers: tuple[tuple[str, int], ...]
    data_gen: int | None
    data_allowed: tuple[tuple[str, int], ...]
    leak: bool


def _gen_active(state: GenState) -> bool:
    return state.gate is Gate.ACTIVE and state.keys_current and state.aa_current


def _gen_allowed(state: GenState) -> frozenset[tuple[str, int]]:
    current = frozenset(state.transfers)
    if state.data_gen is None:
        return current
    acquired = frozenset(state.data_allowed)
    return frozenset(
        effect
        for effect in current | acquired
        if semantics.allowed_effect(effect in current, effect in acquired)
    )


def _gen_successors(state: GenState) -> Iterator[tuple[str, GenState]]:
    if _gen_active(state):
        if state.data_gen is None:
            yield "acquire", replace(
                state, data_gen=state.sid, data_allowed=state.transfers
            )
        if ("sink", -1) in _gen_allowed(state):
            yield "exec-sink", replace(state, leak=True)
    if state.sid < 2:
        for label, transfers in (
            ("leaky", (("sink", -1),)),
            ("same", state.transfers),
        ):
            yield f"change-{label}", replace(
                state,
                sid=state.sid + 1,
                gate=Gate.SUSPENDED,
                keys_current=False,
                aa_current=False,
                transfers=transfers,
            )
    if not state.aa_current:
        yield "attest", replace(state, aa_current=True)
    if state.gate is Gate.SUSPENDED and state.aa_current:
        yield "resume", replace(state, gate=Gate.ACTIVE, keys_current=True)


def _gen_invariants(state: GenState) -> list[str]:
    bad: list[str] = []
    if state.leak and state.data_gen is not None:
        if ("sink", -1) not in state.data_allowed:
            bad.append("old-data-left-via-new-only-exit")
    if state.gate is Gate.SUSPENDED and state.keys_current:
        bad.append("suspended-has-current-keys")
    return bad


def explore_generation(limit: int = 20_000) -> dict[str, object]:
    start = GenState(
        sid=0,
        gate=Gate.ACTIVE,
        keys_current=True,
        aa_current=True,
        transfers=(),
        data_gen=None,
        data_allowed=(),
        leak=False,
    )
    return _bfs(
        start, _gen_successors, _gen_invariants, limit, {"model": "generation"}
    )


# --- Adversarial local-clock model ---


@dataclass(frozen=True)
class ClockState:
    nows: tuple[int, ...]
    highest: tuple[int, ...]
    expired_exec: bool


def _clock_successors(
    state: ClockState, *, horizon: int, tmax: int
) -> Iterator[tuple[str, ClockState]]:
    for i, now in enumerate(state.nows):
        if now < horizon:
            nows = list(state.nows)
            nows[i] = now + 1
            highest = list(state.highest)
            highest[i] = max(highest[i], now + 1)
            yield f"tick-{i}", replace(
                state, nows=tuple(nows), highest=tuple(highest)
            )
        # Attempt an adversarial rollback; the monotonic guard decides.
        candidate = now - 1
        if candidate >= 0 and semantics.tick_ok(candidate, now):
            nows = list(state.nows)
            nows[i] = candidate
            yield f"rollback-{i}", replace(state, nows=tuple(nows))
        # Attempt adversarial execution; the shared guard accepts or rejects it.
        if semantics.exec_time_ok(now, tmax):
            if now >= tmax:
                yield f"exec-expired-{i}", replace(state, expired_exec=True)
            # Valid execution before tmax does not change graph state.


def _clock_invariants(state: ClockState) -> list[str]:
    bad: list[str] = []
    if state.expired_exec:
        bad.append("exec-after-local-expiry")
    if any(now < high for now, high in zip(state.nows, state.highest)):
        bad.append("clock-set-back")
    return bad


def explore_clocks(
    n: int = 2, horizon: int = 4, tmax: int = 3, limit: int = 20_000
) -> dict[str, object]:
    start = ClockState(
        nows=tuple(0 for _ in range(n)),
        highest=tuple(0 for _ in range(n)),
        expired_exec=False,
    )
    return _bfs(
        start,
        lambda s: _clock_successors(s, horizon=horizon, tmax=tmax),
        _clock_invariants,
        limit,
        {"model": "clocks", "actors": n},
    )


# --- Rollback-resurrection model with a stale-Actor-Grant attacker ---


@dataclass(frozen=True)
class RollState:
    revision: int
    cfg: int
    gate: Gate
    keys_current: bool
    aa_current: bool
    resurrected: bool


HELD_CAP_SID = (0, 0)  # Actor Grant issued for initial (revision=0, cfg=0).


def roll_sid(state: RollState) -> tuple[int, int]:
    return (state.revision, state.cfg)


def _roll_successors(
    state: RollState, *, max_rev: int
) -> Iterator[tuple[str, RollState]]:
    if state.revision < max_rev:
        yield "change", replace(
            state,
            revision=state.revision + 1,
            cfg=1 - state.cfg,  # Toggle config; rev 2 restores initial content.
            gate=Gate.SUSPENDED,
            keys_current=False,
            aa_current=False,
        )
    if not state.aa_current:
        yield "attest", replace(state, aa_current=True)
    if state.gate is Gate.SUSPENDED and state.aa_current:
        yield "resume", replace(state, gate=Gate.ACTIVE, keys_current=True)
    # Attempt execution with the Actor Grant issued for the initial state.
    usable = semantics.exec_gate_ok(
        gate_active=state.gate is Gate.ACTIVE,
        keys_current=state.keys_current,
        sid_match=semantics.cap_sid_match(roll_sid(state), HELD_CAP_SID),
        time_ok=True,
    )
    if usable and state.revision > 0:
        yield "exec-held-cap", replace(state, resurrected=True)


def _roll_invariants(state: RollState) -> list[str]:
    if state.resurrected:
        return ["old-actor-grant-resurrected-after-rollback"]
    return []


def explore_rollback(max_rev: int = 3, limit: int = 5_000) -> dict[str, object]:
    start = RollState(
        revision=0,
        cfg=0,
        gate=Gate.ACTIVE,
        keys_current=True,
        aa_current=True,
        resurrected=False,
    )
    return _bfs(
        start,
        lambda s: _roll_successors(s, max_rev=max_rev),
        _roll_invariants,
        limit,
        {"model": "rollback"},
    )


def check_sts() -> dict[str, object]:
    models = {
        "n=1": explore(1),
        "n=2": explore(2),
        "generation": explore_generation(),
        "clocks": explore_clocks(),
        "rollback": explore_rollback(),
    }
    failed = [name for name, body in models.items() if body["status"] != "PASS"]
    if failed:
        first = models[failed[0]]
        return {**first, "status": "FAIL", "failed_model": failed[0]}
    return {"status": "PASS", "models": models}
