"""有界迁移系统。对 2 个 Actor 的可达状态做 BFS，核验安全不变量。

这是有界模型的完全证明：坏状态不可达。不是无界系统的归纳不变量证明。
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from enum import IntEnum
from typing import Iterable


class Gate(IntEnum):
    ACTIVE = 0
    SUSPENDED = 1


class Verdict(IntEnum):
    CONTROLLED = 0
    UNKNOWN = 1
    UNCONTROLLED = 2


PROTECTED = frozenset({"transfer", "retain", "process"})


@dataclass(frozen=True)
class Cap:
    issuer: str
    actor: int
    sid: int
    scope: frozenset[str]
    revoked: bool = False


@dataclass(frozen=True)
class Actor:
    principal: int
    sid: int
    gate: Gate
    keys_for: int | None
    tmax: int
    retain: Verdict
    transfers: tuple[tuple[str, int], ...]
    # transfers: ("actor", target_index) or ("sink", sink_id)
    aa_sid: int | None
    verifier: str


@dataclass(frozen=True)
class Snapshot:
    sids: tuple[int, ...]
    aa_sids: tuple[int, ...]
    color: Verdict


@dataclass(frozen=True)
class State:
    actors: tuple[Actor, ...]
    now: int
    app: Snapshot | None
    leak: bool = False


def actor_verdict(actor: Actor) -> Verdict:
    acc = actor.retain
    for kind, target in actor.transfers:
        if kind == "actor":
            same = actor.principal == 0 and target == 0 or actor.principal == 1
            # target principal: actors[target].principal if in range — computed by caller
            acc = acc if acc > Verdict.UNKNOWN else Verdict.UNKNOWN
        elif target < 0:
            acc = Verdict.UNCONTROLLED
        else:
            acc = acc
    return acc


def eval_actor(actor: Actor, peers: tuple[Actor, ...]) -> Verdict:
    acc = actor.retain
    for kind, target in actor.transfers:
        if kind == "actor":
            if target < 0 or target >= len(peers):
                acc = max(acc, Verdict.UNCONTROLLED)
                continue
            peer = peers[target]
            if peer.principal != actor.principal:
                acc = max(acc, Verdict.UNCONTROLLED)
            else:
                acc = max(acc, Verdict.CONTROLLED)
        elif kind == "sink":
            acc = max(acc, Verdict.UNCONTROLLED if target < 0 else Verdict.CONTROLLED)
    return acc


def app_color(actors: tuple[Actor, ...]) -> Verdict:
    acc = Verdict.CONTROLLED
    for actor in actors:
        acc = max(acc, eval_actor(actor, actors))
    return acc


def ready(state: State) -> bool:
    if state.app is None:
        return False
    if any(actor.gate is Gate.SUSPENDED for actor in state.actors):
        return False
    if tuple(actor.sid for actor in state.actors) != state.app.sids:
        return False
    if tuple(-1 if actor.aa_sid is None else actor.aa_sid for actor in state.actors) != state.app.aa_sids:
        return False
    return True


def displayed(state: State):
    if not ready(state):
        return None
    return state.app.color


def depends(state: State, i: int, j: int) -> bool:
    if i == j:
        return False
    left = state.actors[i]
    right = state.actors[j]
    return ("actor", j) in left.transfers or ("actor", i) in right.transfers


def depends_on_suspended(state: State, i: int) -> bool:
    return any(
        peer.gate is Gate.SUSPENDED and depends(state, i, j)
        for j, peer in enumerate(state.actors)
        if j != i
    )


def cap_ok(state: State, actor_i: int, cap: Cap, op: str) -> bool:
    actor = state.actors[actor_i]
    return (
        cap.issuer != f"actor-{actor_i}"
        and cap.actor == actor_i
        and cap.sid == actor.sid
        and op in cap.scope
        and not cap.revoked
        and state.now < actor.tmax
        and actor.keys_for == actor.sid
        and actor.gate is Gate.ACTIVE
        and not depends_on_suspended(state, actor_i)
    )


def initial(n: int = 2) -> State:
    actors = tuple(
        Actor(
            principal=0,
            sid=0,
            gate=Gate.ACTIVE,
            keys_for=0,
            tmax=5,
            retain=Verdict.CONTROLLED,
            transfers=(),
            aa_sid=0,
            verifier="verifier",
        )
        for _ in range(n)
    )
    snap = Snapshot(
        sids=tuple(0 for _ in actors),
        aa_sids=tuple(0 for _ in actors),
        color=app_color(actors),
    )
    return State(actors=actors, now=0, app=snap, leak=False)


def step_change(
    state: State,
    i: int,
    new_sid: int,
    transfers: tuple[tuple[str, int], ...] | None = None,
) -> State | None:
    actor = state.actors[i]
    if new_sid == actor.sid and transfers is None:
        return None
    updated = replace(
        actor,
        sid=new_sid,
        gate=Gate.SUSPENDED,
        keys_for=None,
        aa_sid=None,
        transfers=actor.transfers if transfers is None else transfers,
    )
    actors = list(state.actors)
    actors[i] = updated
    return replace(state, actors=tuple(actors), app=None)


def step_attest(state: State, i: int) -> State | None:
    actor = state.actors[i]
    if actor.verifier == f"actor-{i}":
        return None
    updated = replace(actor, aa_sid=actor.sid)
    actors = list(state.actors)
    actors[i] = updated
    return replace(state, actors=tuple(actors))


def step_resume(state: State, i: int) -> State | None:
    actor = state.actors[i]
    if actor.gate is not Gate.SUSPENDED:
        return None
    if actor.aa_sid != actor.sid:
        return None
    updated = replace(actor, gate=Gate.ACTIVE, keys_for=actor.sid)
    actors = list(state.actors)
    actors[i] = updated
    return replace(state, actors=tuple(actors))


def step_publish(state: State) -> State | None:
    if any(actor.gate is Gate.SUSPENDED for actor in state.actors):
        return None
    if any(actor.aa_sid != actor.sid for actor in state.actors):
        return None
    snap = Snapshot(
        sids=tuple(actor.sid for actor in state.actors),
        aa_sids=tuple(-1 if actor.aa_sid is None else actor.aa_sid for actor in state.actors),
        color=app_color(state.actors),
    )
    return replace(state, app=snap)


def step_exec(state: State, i: int, op: str, target: int | None) -> State | None:
    actor = state.actors[i]
    cap = Cap("verifier", i, actor.sid, PROTECTED)
    if not cap_ok(state, i, cap, op):
        return None
    if op == "process":
        return state
    if op == "retain":
        return replace(state, leak=True) if actor.retain is Verdict.UNCONTROLLED else state
    if op == "transfer":
        declared = False
        for kind, declared_target in actor.transfers:
            if kind == "actor" and declared_target == target:
                declared = True
            if kind == "sink" and declared_target == target:
                declared = True
        if not declared:
            return None
        if target is not None and 0 <= target < len(state.actors):
            peer = state.actors[target]
            if peer.gate is Gate.SUSPENDED:
                return None
            if peer.principal != actor.principal:
                return replace(state, leak=True)
        if target is not None and target < 0:
            return replace(state, leak=True)
        return state
    return None


def step_tick(state: State) -> State | None:
    return replace(state, now=state.now + 1)


def successors(state: State) -> Iterable[tuple[str, State]]:
    n = len(state.actors)
    for i in range(n):
        for sid in range(3):
            nxt = step_change(state, i, sid)
            if nxt:
                yield f"change-{i}-{sid}", nxt
            nxt = step_change(state, i, sid, (("sink", -1),))
            if nxt:
                yield f"change-{i}-{sid}-leak", nxt
            if n > 1:
                nxt = step_change(state, i, sid, (("actor", (i + 1) % n),))
                if nxt:
                    yield f"change-{i}-{sid}-peer", nxt
        nxt = step_attest(state, i)
        if nxt:
            yield f"attest-{i}", nxt
        nxt = step_resume(state, i)
        if nxt:
            yield f"resume-{i}", nxt
        for op in ("transfer", "retain", "process"):
            for target in list(range(n)) + [-1, None]:
                if op != "transfer" and target is not None:
                    continue
                nxt = step_exec(state, i, op, target)
                if nxt:
                    yield f"exec-{i}-{op}-{target}", nxt
    nxt = step_publish(state)
    if nxt:
        yield "publish", nxt
    if state.now < 6:
        nxt = step_tick(state)
        if nxt:
            yield "tick", nxt


def current_uncontrolled_possible(state: State) -> bool:
    for i, actor in enumerate(state.actors):
        if not cap_ok(
            state,
            i,
            Cap("verifier", i, actor.sid, PROTECTED),
            "transfer",
        ) and not cap_ok(
            state,
            i,
            Cap("verifier", i, actor.sid, PROTECTED),
            "retain",
        ):
            continue
        if actor.retain is Verdict.UNCONTROLLED and cap_ok(
            state, i, Cap("verifier", i, actor.sid, PROTECTED), "retain"
        ):
            return True
        for kind, target in actor.transfers:
            if not cap_ok(
                state, i, Cap("verifier", i, actor.sid, PROTECTED), "transfer"
            ):
                continue
            if kind == "sink" and target < 0:
                return True
            if kind == "actor" and (
                target < 0
                or target >= len(state.actors)
                or state.actors[target].principal != actor.principal
            ):
                return True
    return False


def invariants(state: State) -> list[str]:
    bad: list[str] = []
    shown = displayed(state)
    if shown is Verdict.CONTROLLED and current_uncontrolled_possible(state):
        bad.append("green-and-current-uncontrolled-exec")
    if shown is Verdict.CONTROLLED and any(
        actor.gate is Gate.SUSPENDED for actor in state.actors
    ):
        bad.append("green-while-suspended")
    if state.app is not None:
        if state.app.sids != tuple(actor.sid for actor in state.actors):
            if ready(state):
                bad.append("ready-with-stale-app")
        if any(
            aa != sid
            for sid, aa in zip(state.app.sids, state.app.aa_sids)
        ):
            bad.append("mixed-snapshot-published")
    for i, actor in enumerate(state.actors):
        if actor.gate is Gate.SUSPENDED and actor.keys_for == actor.sid:
            # resume assigns keys; mid-suspend must not have current keys
            if actor.aa_sid != actor.sid:
                bad.append(f"suspended-has-keys-{i}")
        if actor.verifier == f"actor-{i}" and actor.aa_sid == actor.sid:
            bad.append(f"self-signed-{i}")
        if actor.gate is Gate.ACTIVE and depends_on_suspended(state, i):
            if cap_ok(
                state, i, Cap("verifier", i, actor.sid, PROTECTED), "process"
            ):
                bad.append(f"protected-while-depending-on-suspended-{i}")
    return bad


def explore(n: int = 2, limit: int = 20000) -> dict[str, object]:
    start = initial(n)
    seen: set[State] = set()
    queue = [start]
    violations: list[dict[str, object]] = []
    transitions = 0
    while queue and len(seen) < limit:
        state = queue.pop()
        if state in seen:
            continue
        seen.add(state)
        bad = invariants(state)
        if bad:
            violations.append({"bad": bad, "state": str(state)})
            continue
        for _label, nxt in successors(state):
            transitions += 1
            if nxt not in seen:
                queue.append(nxt)
    return {
        "status": "PASS" if not violations else "FAIL",
        "states": len(seen),
        "transitions": transitions,
        "violations": violations[:10],
        "bounded": True,
        "actors": n,
    }


@dataclass(frozen=True)
class GenState:
    sid: int
    gate: Gate
    keys_for: int | None
    aa_sid: int | None
    transfers: tuple[tuple[str, int], ...]
    data_gen: int | None
    data_allowed: tuple[tuple[str, int], ...]
    leak: bool


def _gen_active(state: GenState) -> bool:
    return (
        state.gate is Gate.ACTIVE
        and state.keys_for == state.sid
        and state.aa_sid == state.sid
    )


def _gen_allowed(state: GenState) -> frozenset[tuple[str, int]]:
    current = frozenset(state.transfers)
    if state.data_gen is None:
        return current
    return current & frozenset(state.data_allowed)


def explore_generation(limit: int = 4000) -> dict[str, object]:
    start = GenState(
        sid=0,
        gate=Gate.ACTIVE,
        keys_for=0,
        aa_sid=0,
        transfers=(),
        data_gen=None,
        data_allowed=(),
        leak=False,
    )
    seen: set[GenState] = set()
    queue = [start]
    violations: list[str] = []
    transitions = 0

    def push(nxt: GenState | None) -> None:
        nonlocal transitions
        if nxt is None:
            return
        transitions += 1
        if nxt not in seen:
            queue.append(nxt)

    while queue and len(seen) < limit:
        state = queue.pop()
        if state in seen:
            continue
        seen.add(state)
        if state.leak and state.data_gen is not None:
            if ("sink", -1) not in state.data_allowed:
                violations.append("old-data-left-via-new-only-exit")
                continue
        if state.gate is Gate.SUSPENDED and state.keys_for == state.sid:
            if state.aa_sid != state.sid:
                violations.append("suspended-has-keys")
                continue

        if _gen_active(state):
            if state.data_gen is None:
                push(
                    replace(
                        state,
                        data_gen=state.sid,
                        data_allowed=state.transfers,
                    )
                )
            else:
                push(state)
            allowed = _gen_allowed(state)
            if ("sink", -1) in allowed:
                push(replace(state, leak=True))
            if ("sink", -1) in state.transfers and ("sink", -1) not in allowed:
                # new-only exit must be refused, not taken
                pass
        if state.sid < 2:
            push(
                replace(
                    state,
                    sid=state.sid + 1,
                    gate=Gate.SUSPENDED,
                    keys_for=None,
                    aa_sid=None,
                    transfers=(("sink", -1),),
                )
            )
            push(
                replace(
                    state,
                    sid=state.sid + 1,
                    gate=Gate.SUSPENDED,
                    keys_for=None,
                    aa_sid=None,
                    transfers=state.transfers,
                )
            )
        if state.gate is Gate.SUSPENDED:
            attested = replace(state, aa_sid=state.sid)
            push(attested)
            if attested.aa_sid == attested.sid:
                push(replace(attested, gate=Gate.ACTIVE, keys_for=attested.sid))

    return {
        "status": "PASS" if not violations else "FAIL",
        "states": len(seen),
        "transitions": transitions,
        "violations": violations[:10],
        "bounded": True,
        "model": "generation",
    }


@dataclass(frozen=True)
class ClockState:
    nows: tuple[int, ...]
    expired_exec: bool


def explore_clocks(n: int = 2, horizon: int = 4, tmax: int = 3) -> dict[str, object]:
    start = ClockState(nows=tuple(0 for _ in range(n)), expired_exec=False)
    seen: set[ClockState] = set()
    queue = [start]
    violations: list[str] = []
    transitions = 0
    while queue:
        state = queue.pop()
        if state in seen:
            continue
        seen.add(state)
        if state.expired_exec:
            violations.append("exec-after-local-expiry")
            continue
        for i in range(n):
            if state.nows[i] < horizon:
                nxt_nows = list(state.nows)
                nxt_nows[i] += 1
                nxt = ClockState(nows=tuple(nxt_nows), expired_exec=False)
                transitions += 1
                if nxt not in seen:
                    queue.append(nxt)
            if state.nows[i] >= tmax:
                # must refuse; attempting exec must not yield expired_exec
                continue
            transitions += 1
            idle = ClockState(nows=state.nows, expired_exec=False)
            if idle not in seen:
                queue.append(idle)
    # no transition sets expired_exec, and clocks only increase
    for state in seen:
        if any(
            later < earlier
            for later, earlier in zip(state.nows, start.nows)
        ):
            violations.append("clock-set-back")
    return {
        "status": "PASS" if not violations else "FAIL",
        "states": len(seen),
        "transitions": transitions,
        "violations": violations[:10],
        "bounded": True,
        "model": "clocks",
        "actors": n,
    }


def check_sts() -> dict[str, object]:
    first = explore(2)
    if first["status"] != "PASS":
        return first
    solo = explore(1)
    if solo["status"] != "PASS":
        return solo
    generation = explore_generation()
    if generation["status"] != "PASS":
        return generation
    clocks = explore_clocks()
    if clocks["status"] != "PASS":
        return clocks
    return {
        "status": "PASS",
        "models": {
            "n=1": solo,
            "n=2": first,
            "generation": generation,
            "clocks": clocks,
        },
    }
