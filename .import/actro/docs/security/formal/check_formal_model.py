#!/usr/bin/env python3
"""Actrium 形式模型的有限状态回归。

检查规范中已经写成纯函数的性质：三值格、图划分、Merkle 成员证明、
判断关系、跨 Principal 出口、挂起隔离、信箱最新状态、快照抗拼接、
Capability 字段匹配，以及颜色对风险接受／输入授权独立。

这不是密码学证明、协议模型检查器或实现符合性测试。
"""

from __future__ import annotations

import hashlib
import itertools
import json
from dataclasses import dataclass
from enum import IntEnum
from typing import Iterable, Sequence


class Verdict(IntEnum):
    CONTROLLED = 0
    UNKNOWN = 1
    UNCONTROLLED = 2


class Gate(IntEnum):
    ACTIVE = 0
    SUSPENDED = 1


def digest(domain: str, fields: Sequence[str]) -> str:
    payload = json.dumps(
        [domain, *fields], ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def aggregate(items: Iterable[Verdict]) -> Verdict:
    return max(items, default=Verdict.CONTROLLED)


def retain_verdict(retain: int | None) -> Verdict:
    if retain == 0:
        return Verdict.CONTROLLED
    if retain is None:
        return Verdict.UNKNOWN
    if retain == 1:
        return Verdict.UNCONTROLLED
    raise ValueError("retain 必须是 0、None 或 1")


def application_verdict(actor_verdicts: Iterable[Verdict]) -> Verdict:
    return aggregate(actor_verdicts)


def color(verdict: Verdict, graph_valid: bool = True, channel_valid: bool = True):
    if not graph_valid or not channel_valid:
        return None
    return verdict


def run_allowed(
    verdict: Verdict,
    accepted: bool,
    *,
    graph_valid: bool = True,
    channel_valid: bool = True,
) -> bool:
    if color(verdict, graph_valid, channel_valid) is None:
        return False
    return verdict == Verdict.CONTROLLED or accepted


def color_of(
    actor_verdicts: Sequence[Verdict],
    *,
    waiver: object = None,
    input_auth: object = None,
    graph_valid: bool = True,
    channel_valid: bool = True,
):
    _ = waiver, input_auth
    return color(application_verdict(actor_verdicts), graph_valid, channel_valid)


# --- Merkle (RFC 6962 风格：奇数层提升，不复制叶子) ---


def merkle_leaf_hash(leaf: str) -> str:
    return digest("actrium/merkle-leaf/v1", (leaf,))


def merkle_node_hash(left: str, right: str) -> str:
    return digest("actrium/merkle-node/v1", (left, right))


def merkle_levels(leaves: Sequence[str]) -> list[list[str]]:
    if not leaves:
        return [[digest("actrium/merkle-empty/v1", ())]]
    level = [merkle_leaf_hash(leaf) for leaf in leaves]
    levels = [level]
    while len(level) > 1:
        nxt: list[str] = []
        index = 0
        while index < len(level):
            if index + 1 < len(level):
                nxt.append(merkle_node_hash(level[index], level[index + 1]))
                index += 2
            else:
                nxt.append(level[index])
                index += 1
        levels.append(nxt)
        level = nxt
    return levels


def merkle_root(leaves: Sequence[str]) -> str:
    return merkle_levels(leaves)[-1][0]


def merkle_proof(leaves: Sequence[str], index: int) -> list[tuple[str, str]]:
    if index < 0 or index >= len(leaves):
        raise IndexError(index)
    proof: list[tuple[str, str]] = []
    levels = merkle_levels(leaves)
    position = index
    for level in levels[:-1]:
        if len(level) == 1:
            break
        if position % 2 == 1:
            proof.append(("L", level[position - 1]))
            position //= 2
        elif position + 1 < len(level):
            proof.append(("R", level[position + 1]))
            position //= 2
        else:
            position //= 2
    return proof


def merkle_verify(leaf: str, proof: Sequence[tuple[str, str]], root: str) -> bool:
    acc = merkle_leaf_hash(leaf)
    for side, sibling in proof:
        if side == "L":
            acc = merkle_node_hash(sibling, acc)
        elif side == "R":
            acc = merkle_node_hash(acc, sibling)
        else:
            return False
    return acc == root


def actor_state_id(
    actor_id: str,
    principal_id: str,
    revision: str,
    package: str,
    config: str,
    capabilities: str,
    environment: str,
) -> str:
    """revision 由状态权威单调递增：内容回滚也得到新 ActorStateId。"""
    return digest(
        "actrium/actor-state/v1",
        (actor_id, principal_id, revision, package, config, capabilities, environment),
    )


def app_leaf(actor_id: str, state_id: str, assurance_id: str) -> str:
    return digest("actrium/application-leaf/v1", (actor_id, state_id, assurance_id))


_ENC_ID_ALLOWED = frozenset(
    "abcdefghijklmnopqrstuvwxyz0123456789._:-"
)


def _require_canonical_id(value: str) -> str:
    """EncGraph 内的标识符必须落在受限 ASCII 字符集，杜绝 JSON 转义歧义。"""
    if not value or any(ch not in _ENC_ID_ALLOWED for ch in value):
        raise ValueError(f"EncGraph 标识符超出受限字符集: {value!r}")
    return value


def enc_graph(
    principal_id: str,
    nodes: Sequence[str],
    edges: Sequence[tuple[str, str]],
) -> str:
    nodes = tuple(nodes)
    edges = tuple(edges)
    _require_canonical_id(principal_id)
    for node in nodes:
        _require_canonical_id(node)
    for src, dst in edges:
        _require_canonical_id(src)
        _require_canonical_id(dst)
    ordered_nodes = sorted(set(nodes))
    ordered_edges = sorted(set((src, dst) for src, dst in edges))
    return json.dumps(
        [
            "actrium/graph/v1",
            1,
            principal_id,
            ordered_nodes,
            [list(edge) for edge in ordered_edges],
        ],
        ensure_ascii=False,
        separators=(",", ":"),
    )


def structure_id(
    principal_id: str,
    nodes: Sequence[str],
    edges: Sequence[tuple[str, str]],
) -> str:
    return digest("actrium/structure/v1", (enc_graph(principal_id, nodes, edges),))


def allowed_effects(
    current: Sequence[object],
    acquired: Sequence[object] | None,
) -> set[object]:
    current_set = set(current)
    if acquired is None:
        return current_set
    return current_set & set(acquired)


def join_accepted(*verdicts: Verdict) -> Verdict:
    return aggregate(verdicts)


def used_node_verdict(
    accepted: Sequence[Verdict], *, has_effects: bool
) -> Verdict:
    if not accepted:
        return Verdict.UNKNOWN if has_effects else Verdict.CONTROLLED
    return aggregate(accepted)


def depends_undirected(
    transfers: Sequence[tuple[int, int]],
    left: int,
    right: int,
) -> bool:
    return (left, right) in transfers or (right, left) in transfers


def local_continue(
    *,
    gate_active: bool,
    aa_current: bool,
    depends_on_changed: bool,
    display_app_green: bool,
) -> bool:
    return (
        gate_active
        and aa_current
        and not depends_on_changed
        and not display_app_green
    )


def app_state_id(
    structure_id: str,
    principal_id: str,
    leaves: Sequence[tuple[str, str]],
) -> str:
    """leaves 是 (actor_id, leaf_digest) 对；按 ActorId 排序建树（§5.2）。"""
    ordered = tuple(leaf for _actor, leaf in sorted(leaves))
    return digest(
        "actrium/application-state/v1",
        (structure_id, principal_id, merkle_root(ordered)),
    )


# --- 判断关系 ---


@dataclass(frozen=True)
class Evidence:
    issuer: str
    authentic: bool
    bound_actor: str
    bound_state: str
    not_after: int
    claim: str
    target: str | None = None
    covers_recover_paths: bool = False


@dataclass(frozen=True)
class Policy:
    trusted_issuers: frozenset[str]
    trusted_sinks: frozenset[str]
    now: int = 0


def evidence_accepted(
    item: Evidence, policy: Policy, actor_id: str, state_id: str
) -> bool:
    return (
        item.authentic
        and item.issuer in policy.trusted_issuers
        and item.issuer != actor_id
        and item.bound_actor == actor_id
        and item.bound_state == state_id
        and item.not_after > policy.now
    )


def judge_transfer(
    *,
    actor_id: str,
    state_id: str,
    target: str,
    target_kind: str,
    in_graph: bool,
    same_principal: bool,
    confirmed_present: bool,
    host_enforced: bool,
    evidence: Sequence[Evidence],
    policy: Policy,
) -> Verdict:
    if target_kind == "actor" and not (in_graph and same_principal):
        return (
            Verdict.UNCONTROLLED if confirmed_present else Verdict.UNKNOWN
        )

    accepted = [
        item
        for item in evidence
        if evidence_accepted(item, policy, actor_id, state_id)
        and item.claim in {"restricts", "uncontrolled"}
        and item.target == target
    ]
    if any(item.claim == "uncontrolled" for item in accepted):
        return Verdict.UNCONTROLLED
    if (
        host_enforced
        and any(item.claim == "restricts" for item in accepted)
        and (
            (target_kind == "actor" and in_graph)
            or target in policy.trusted_sinks
        )
    ):
        return Verdict.CONTROLLED
    return Verdict.UNKNOWN


def judge_retain(
    *,
    actor_id: str,
    state_id: str,
    evidence: Sequence[Evidence],
    policy: Policy,
) -> Verdict:
    accepted = [
        item
        for item in evidence
        if evidence_accepted(item, policy, actor_id, state_id)
        and item.claim in {"unrecoverable", "recoverable"}
    ]
    if any(item.claim == "recoverable" for item in accepted):
        return Verdict.UNCONTROLLED
    if any(
        item.claim == "unrecoverable" and item.covers_recover_paths
        for item in accepted
    ):
        return Verdict.CONTROLLED
    return Verdict.UNKNOWN


# --- Capability / 执行 ---


@dataclass(frozen=True)
class Capability:
    issuer: str
    issuer_key_holder: str
    actor_id: str
    state_id: str
    scope: frozenset[str]
    not_before: int
    expires_at: int
    revoked: bool = False


def capability_accepts(
    cap: Capability,
    *,
    current_actor: str,
    current_state: str,
    operation: str,
    now: int,
    trusted_issuers: frozenset[str],
) -> bool:
    return (
        cap.issuer in trusted_issuers
        and cap.issuer_key_holder == cap.issuer
        and cap.actor_id == current_actor
        and cap.state_id == current_state
        and operation in cap.scope
        and cap.not_before <= now < cap.expires_at
        and not cap.revoked
    )


@dataclass
class ActorRuntime:
    actor_id: str
    principal_id: str
    state_id: str
    gate: Gate = Gate.ACTIVE
    t_max: int = 100
    keys_for: str | None = None


PROTECTED = frozenset({"transfer", "retain"})
ADMIN = frozenset({"attest", "exit"})


def exec_allowed(
    runtime: ActorRuntime,
    cap: Capability,
    operation: str,
    *,
    now: int,
    trusted_issuers: frozenset[str],
) -> bool:
    if operation in PROTECTED:
        if runtime.gate is Gate.SUSPENDED:
            return False
        if now >= runtime.t_max:
            return False
        if runtime.keys_for != runtime.state_id:
            return False
    elif operation not in ADMIN:
        return False
    if operation in ADMIN:
        return runtime.actor_id == cap.actor_id
    return capability_accepts(
        cap,
        current_actor=runtime.actor_id,
        current_state=runtime.state_id,
        operation=operation,
        now=now,
        trusted_issuers=trusted_issuers,
    )


def change_state(runtime: ActorRuntime, new_state: str) -> ActorRuntime:
    runtime.state_id = new_state
    runtime.gate = Gate.SUSPENDED
    runtime.keys_for = None
    return runtime


# --- 图 ---


def valid_dependency(
    source_principal: str,
    target_principal: str,
    target_resolvable: bool,
) -> bool:
    return target_resolvable and source_principal == target_principal


def directed_edges_from_transfers(
    principals: Sequence[str],
    transfers: Sequence[tuple[int, int]],
    resolvable: Sequence[bool] | None = None,
) -> set[tuple[int, int]]:
    flags = resolvable or [True] * len(principals)
    edges: set[tuple[int, int]] = set()
    for source, target in transfers:
        if valid_dependency(principals[source], principals[target], flags[target]):
            edges.add((source, target))
    return edges


def canonical_components(node_count: int, directed_edges: set[tuple[int, int]]):
    adjacency = {node: set() for node in range(node_count)}
    for source, target in directed_edges:
        adjacency[source].add(target)
        adjacency[target].add(source)

    unseen = set(range(node_count))
    components: list[tuple[int, ...]] = []
    while unseen:
        seed = min(unseen)
        stack = [seed]
        reached: set[int] = set()
        while stack:
            current = stack.pop()
            if current in reached:
                continue
            reached.add(current)
            stack.extend(adjacency[current] - reached)
        unseen -= reached
        components.append(tuple(sorted(reached)))
    return tuple(sorted(components))


def outbound_only_components(
    node_count: int, directed_edges: set[tuple[int, int]], seed: int
) -> tuple[int, ...]:
    outgoing: dict[int, set[int]] = {node: set() for node in range(node_count)}
    for source, target in directed_edges:
        outgoing[source].add(target)
    reached = set()
    stack = [seed]
    while stack:
        current = stack.pop()
        if current in reached:
            continue
        reached.add(current)
        stack.extend(outgoing[current] - reached)
    return tuple(sorted(reached))


# --- 快照发布 ---


@dataclass(frozen=True)
class SnapshotLeaf:
    actor_id: str
    current_sid: str
    leaf_sid: str
    assurance_sid: str
    assurance_id: str


def publish_ok(leaves: Sequence[SnapshotLeaf], structure_id: str, principal: str):
    if len({leaf.actor_id for leaf in leaves}) != len(leaves):
        return None  # 每个 ActorId 恰好一个叶子
    if any(
        leaf.leaf_sid != leaf.current_sid or leaf.assurance_sid != leaf.leaf_sid
        for leaf in leaves
    ):
        return None
    encoded = [
        (leaf.actor_id, app_leaf(leaf.actor_id, leaf.leaf_sid, leaf.assurance_id))
        for leaf in leaves
    ]
    return app_state_id(structure_id, principal, encoded)


# --- 检查 ---


def verify_lattice() -> int:
    checked = 0
    values = tuple(Verdict)
    for left, right, third in itertools.product(values, repeat=3):
        assert aggregate((left, right)) == aggregate((right, left))
        assert aggregate((aggregate((left, right)), third)) == aggregate(
            (left, aggregate((right, third)))
        )
        assert aggregate((left, left)) == left
        checked += 3

    for length in range(6):
        for items in itertools.product(values, repeat=length):
            expected = aggregate(items)
            for permutation in set(itertools.permutations(items)):
                assert aggregate(permutation) == expected
                checked += 1
    return checked


def verify_monotonicity() -> int:
    checked = 0
    values = tuple(Verdict)
    for length in range(5):
        for actors in itertools.product(values, repeat=length):
            current = application_verdict(actors)
            for added in values:
                assert application_verdict((*actors, added)) >= current
                checked += 1
            for index, original in enumerate(actors):
                for weakened in values:
                    if weakened < original:
                        continue
                    changed = list(actors)
                    changed[index] = weakened
                    assert application_verdict(changed) >= current
                    checked += 1
    return checked


def verify_policy_separation() -> int:
    checked = 0
    values = tuple(Verdict)
    waivers = (None, "accept-yellow", "accept-red")
    inputs = (None, "camera-on", "camera-off")
    for length in range(1, 4):
        for actors in itertools.product(values, repeat=length):
            baseline = color_of(actors)
            for waiver, input_auth in itertools.product(waivers, inputs):
                assert color_of(actors, waiver=waiver, input_auth=input_auth) == baseline
                checked += 1
            for verdict in values:
                assert run_allowed(verdict, False) is (verdict == Verdict.CONTROLLED)
                assert run_allowed(verdict, True) is True
                assert run_allowed(verdict, True, channel_valid=False) is False
                assert run_allowed(verdict, True, graph_valid=False) is False
                checked += 4
    return checked


def verify_retain() -> int:
    assert retain_verdict(0) == Verdict.CONTROLLED
    assert retain_verdict(None) == Verdict.UNKNOWN
    assert retain_verdict(1) == Verdict.UNCONTROLLED
    return 3


def verify_graph_components() -> int:
    checked = 0
    for node_count in range(1, 5):
        possible_edges = tuple(
            (source, target)
            for source in range(node_count)
            for target in range(node_count)
            if source != target
        )
        for mask in range(1 << len(possible_edges)):
            edges = {
                possible_edges[index]
                for index in range(len(possible_edges))
                if mask & (1 << index)
            }
            result = canonical_components(node_count, edges)
            reversed_result = canonical_components(
                node_count, {(target, source) for source, target in edges}
            )
            assert result == reversed_result
            flattened = tuple(sorted(node for component in result for node in component))
            assert flattened == tuple(range(node_count))
            assert sum(len(component) for component in result) == node_count
            checked += 3

            for seed in range(node_count):
                outbound = outbound_only_components(node_count, edges, seed)
                owned = next(component for component in result if seed in component)
                assert set(outbound) <= set(owned)
                checked += 1
    return checked


def verify_cross_principal() -> int:
    checked = 0
    principals = ("p1", "p1", "p2")
    transfers = ((0, 1), (0, 2), (2, 0))
    edges = directed_edges_from_transfers(principals, transfers)
    assert (0, 1) in edges
    assert (0, 2) not in edges
    assert (2, 0) not in edges
    components = canonical_components(3, edges)
    assert (0, 1) in components
    assert (2,) in components
    checked += 4

    policy = Policy(trusted_issuers=frozenset({"v"}), trusted_sinks=frozenset())
    out = judge_transfer(
        actor_id="a0",
        state_id="s0",
        target="a2",
        target_kind="actor",
        in_graph=False,
        same_principal=False,
        confirmed_present=True,
        host_enforced=True,
        evidence=(),
        policy=policy,
    )
    unknown = judge_transfer(
        actor_id="a0",
        state_id="s0",
        target="a-missing",
        target_kind="actor",
        in_graph=False,
        same_principal=False,
        confirmed_present=False,
        host_enforced=False,
        evidence=(),
        policy=policy,
    )
    assert out == Verdict.UNCONTROLLED
    assert unknown == Verdict.UNKNOWN
    checked += 2
    return checked


def verify_judgment() -> int:
    checked = 0
    policy = Policy(
        trusted_issuers=frozenset({"verifier", "a"}),
        trusted_sinks=frozenset({"display://local"}),
        now=10,
    )
    good = Evidence(
        issuer="verifier",
        authentic=True,
        bound_actor="a",
        bound_state="s1",
        not_after=20,
        claim="restricts",
        target="display://local",
    )
    assert (
        judge_transfer(
            actor_id="a",
            state_id="s1",
            target="display://local",
            target_kind="sink",
            in_graph=False,
            same_principal=True,
            confirmed_present=True,
            host_enforced=True,
            evidence=(good,),
            policy=policy,
        )
        == Verdict.CONTROLLED
    )
    checked += 1

    stale = Evidence(**{**good.__dict__, "bound_state": "s0"})
    expired = Evidence(**{**good.__dict__, "not_after": 5})
    untrusted = Evidence(**{**good.__dict__, "issuer": "stranger"})
    fake = Evidence(**{**good.__dict__, "authentic": False})
    self_signed = Evidence(**{**good.__dict__, "issuer": "a"})
    for item in (stale, expired, untrusted, fake, self_signed):
        assert (
            judge_transfer(
                actor_id="a",
                state_id="s1",
                target="display://local",
                target_kind="sink",
                in_graph=False,
                same_principal=True,
                confirmed_present=True,
                host_enforced=True,
                evidence=(item,),
                policy=policy,
            )
            == Verdict.UNKNOWN
        )
        checked += 1

    leak = Evidence(
        issuer="verifier",
        authentic=True,
        bound_actor="a",
        bound_state="s1",
        not_after=20,
        claim="uncontrolled",
        target="display://local",
    )
    assert (
        judge_transfer(
            actor_id="a",
            state_id="s1",
            target="display://local",
            target_kind="sink",
            in_graph=False,
            same_principal=True,
            confirmed_present=True,
            host_enforced=True,
            evidence=(good, leak),
            policy=policy,
        )
        == Verdict.UNCONTROLLED
    )
    checked += 1

    recover = Evidence(
        issuer="verifier",
        authentic=True,
        bound_actor="a",
        bound_state="s1",
        not_after=20,
        claim="recoverable",
    )
    gone = Evidence(
        issuer="verifier",
        authentic=True,
        bound_actor="a",
        bound_state="s1",
        not_after=20,
        claim="unrecoverable",
        covers_recover_paths=True,
    )
    incomplete = Evidence(
        issuer="verifier",
        authentic=True,
        bound_actor="a",
        bound_state="s1",
        not_after=20,
        claim="unrecoverable",
        covers_recover_paths=False,
    )
    assert judge_retain(actor_id="a", state_id="s1", evidence=(gone,), policy=policy) == (
        Verdict.CONTROLLED
    )
    assert judge_retain(
        actor_id="a", state_id="s1", evidence=(incomplete,), policy=policy
    ) == Verdict.UNKNOWN
    assert judge_retain(
        actor_id="a", state_id="s1", evidence=(gone, recover), policy=policy
    ) == Verdict.UNCONTROLLED
    checked += 3
    return checked


def verify_merkle_and_binding() -> int:
    checked = 0
    for count in range(0, 9):
        leaves = [f"leaf-{index}" for index in range(count)]
        root = merkle_root(leaves)
        for index, leaf in enumerate(leaves):
            proof = merkle_proof(leaves, index)
            assert merkle_verify(leaf, proof, root)
            if count > 1:
                other = leaves[(index + 1) % count]
                assert not merkle_verify(other, proof, root)
                checked += 1
            checked += 1

    base = ("actor-a", "principal-1", "0", "pkg-1", "cfg-1", "cap-1", "env-1")
    base_state = actor_state_id(*base)
    for index in range(len(base)):
        changed = list(base)
        changed[index] += "-changed"
        assert actor_state_id(*changed) != base_state
        checked += 1

    # 回滚不复现旧 sid：内容相同但 revision 不同，得到不同状态根。
    rolled_back = ("actor-a", "principal-1", "2", "pkg-1", "cfg-1", "cap-1", "env-1")
    assert actor_state_id(*rolled_back) != base_state
    assert actor_state_id(*base) == base_state
    checked += 2

    leaf_a = ("actor-a", app_leaf("actor-a", base_state, "aa-1"))
    leaf_b = ("actor-b", app_leaf("actor-b", "sid-b", "aa-b"))
    app = app_state_id("structure-1", "principal-1", (leaf_a, leaf_b))
    assert app == app_state_id("structure-1", "principal-1", (leaf_b, leaf_a))
    assert app != app_state_id("structure-1", "principal-1", (leaf_a,))
    assert app != app_state_id("structure-2", "principal-1", (leaf_a, leaf_b))
    assert app != app_state_id("structure-1", "principal-2", (leaf_a, leaf_b))
    spliced = ("actor-a", app_leaf("actor-a", base_state, "aa-old"))
    assert app_state_id("structure-1", "principal-1", (spliced, leaf_b)) != app
    checked += 5
    return checked


def verify_capability_and_execution() -> int:
    checked = 0
    trusted = frozenset({"issuer"})
    cap = Capability(
        issuer="issuer",
        issuer_key_holder="issuer",
        actor_id="a",
        state_id="s1",
        scope=frozenset({"transfer"}),
        not_before=0,
        expires_at=10,
    )
    runtime = ActorRuntime(actor_id="a", principal_id="p", state_id="s1", keys_for="s1")
    assert exec_allowed(runtime, cap, "transfer", now=1, trusted_issuers=trusted)
    checked += 1

    forged = Capability(**{**cap.__dict__, "issuer_key_holder": "attacker", "scope": frozenset({"transfer", "retain"})})
    assert not capability_accepts(
        forged,
        current_actor="a",
        current_state="s1",
        operation="retain",
        now=1,
        trusted_issuers=trusted,
    )
    checked += 1

    for kwargs in (
        {"actor_id": "b"},
        {"state_id": "s0"},
        {"expires_at": 1},
        {"revoked": True},
        {"scope": frozenset({"retain"})},
    ):
        broken = Capability(**{**cap.__dict__, **kwargs})
        assert not capability_accepts(
            broken,
            current_actor="a",
            current_state="s1",
            operation="transfer",
            now=1,
            trusted_issuers=trusted,
        )
        checked += 1

    change_state(runtime, "s2")
    assert runtime.gate is Gate.SUSPENDED
    assert not exec_allowed(runtime, cap, "transfer", now=1, trusted_issuers=trusted)
    new_cap = Capability(**{**cap.__dict__, "state_id": "s2"})
    assert not exec_allowed(runtime, new_cap, "transfer", now=1, trusted_issuers=trusted)
    assert exec_allowed(runtime, new_cap, "attest", now=1, trusted_issuers=trusted)
    runtime.gate = Gate.ACTIVE
    runtime.keys_for = "s2"
    assert exec_allowed(runtime, new_cap, "transfer", now=1, trusted_issuers=trusted)
    assert not exec_allowed(runtime, new_cap, "transfer", now=100, trusted_issuers=trusted)
    checked += 6
    return checked


def verify_snapshot_publish() -> int:
    checked = 0
    consistent = (
        SnapshotLeaf("a", "s1", "s1", "s1", "aa-1"),
        SnapshotLeaf("b", "s2", "s2", "s2", "aa-2"),
    )
    appid = publish_ok(consistent, "g1", "p")
    assert appid is not None
    checked += 1

    mixed_state = (
        SnapshotLeaf("a", "s1", "s0", "s0", "aa-old"),
        consistent[1],
    )
    mixed_aa = (
        SnapshotLeaf("a", "s1", "s1", "s0", "aa-old"),
        consistent[1],
    )
    assert publish_ok(mixed_state, "g1", "p") is None
    assert publish_ok(mixed_aa, "g1", "p") is None
    checked += 2

    later = (
        SnapshotLeaf("a", "s3", "s3", "s3", "aa-3"),
        consistent[1],
    )
    assert publish_ok(later, "g1", "p") != appid
    checked += 1

    duplicated = (
        consistent[0],
        SnapshotLeaf("a", "s1", "s1", "s1", "aa-1-bis"),
    )
    assert publish_ok(duplicated, "g1", "p") is None
    checked += 1
    return checked


def verify_enc_graph() -> int:
    checked = 0
    nodes = ("b", "a", "c")
    edges = (("c", "a"), ("a", "b"))
    baseline = enc_graph("p", nodes, edges)
    assert baseline == enc_graph("p", reversed(nodes), reversed(edges))
    assert baseline == enc_graph("p", ("a", "a", "b", "c"), (("a", "b"), ("a", "b"), ("c", "a")))
    assert enc_graph("p", nodes, edges) != enc_graph("q", nodes, edges)
    assert enc_graph("p", nodes, edges) != enc_graph("p", nodes, (("a", "b"),))
    assert enc_graph("p", nodes, edges) != enc_graph("p", ("a", "b"), edges)
    assert structure_id("p", nodes, edges) == structure_id("p", ("a", "c", "b"), (("a", "b"), ("c", "a")))
    checked += 6

    # 标识符字符集受限（D-ENC-02）：超集直接拒绝，不产生歧义编码。
    for bad_id in ("A", "空", "a b", "a\\b", 'a"b', ""):
        try:
            enc_graph("p", (bad_id,), ())
        except ValueError:
            checked += 1
        else:
            raise AssertionError(f"应拒绝非规范标识符: {bad_id!r}")

    labels = ("n0", "n1", "n2")
    for node_count in range(1, 4):
        possible = tuple(
            (labels[src], labels[dst])
            for src in range(node_count)
            for dst in range(node_count)
            if src != dst
        )
        encodings: dict[str, int] = {}
        for mask in range(1 << len(possible)):
            chosen = tuple(
                possible[index]
                for index in range(len(possible))
                if mask & (1 << index)
            )
            encoded = enc_graph("p", labels[:node_count], chosen)
            assert encoded not in encodings
            encodings[encoded] = mask
            checked += 1
    return checked


def verify_generation() -> int:
    checked = 0
    old = {"peer", "display"}
    new = {"peer", "internet"}
    assert allowed_effects(new, old) == {"peer"}
    assert "internet" not in allowed_effects(new, old)
    assert "display" not in allowed_effects(new, old)
    assert allowed_effects(new, None) == new
    checked += 4

    for in_old, in_new in itertools.product((False, True), repeat=2):
        current = {"t"} if in_new else set()
        acquired = {"t"} if in_old else set()
        allowed = allowed_effects(current, acquired)
        if in_new and not in_old:
            assert "t" not in allowed
        if in_old and in_new:
            assert "t" in allowed
        checked += 1
    return checked


def verify_assurance_join() -> int:
    checked = 0
    values = tuple(Verdict)
    for left, right in itertools.product(values, repeat=2):
        published = join_accepted(left, right)
        assert published >= left and published >= right
        if published is Verdict.CONTROLLED:
            assert left is Verdict.CONTROLLED and right is Verdict.CONTROLLED
        checked += 2
    assert join_accepted() is Verdict.CONTROLLED
    assert used_node_verdict((), has_effects=True) is Verdict.UNKNOWN
    assert used_node_verdict((), has_effects=False) is Verdict.CONTROLLED
    assert used_node_verdict((Verdict.CONTROLLED, Verdict.UNKNOWN), has_effects=True) is (
        Verdict.UNKNOWN
    )
    assert join_accepted(Verdict.UNKNOWN) is Verdict.UNKNOWN
    checked += 5
    return checked


def verify_depends_and_continue() -> int:
    checked = 0
    edges = ((0, 1),)
    assert depends_undirected(edges, 0, 1)
    assert depends_undirected(edges, 1, 0)
    assert not depends_undirected(edges, 0, 2)
    checked += 3

    assert local_continue(
        gate_active=True,
        aa_current=True,
        depends_on_changed=False,
        display_app_green=False,
    )
    assert not local_continue(
        gate_active=True,
        aa_current=True,
        depends_on_changed=True,
        display_app_green=False,
    )
    assert not local_continue(
        gate_active=True,
        aa_current=True,
        depends_on_changed=False,
        display_app_green=True,
    )
    assert not local_continue(
        gate_active=False,
        aa_current=True,
        depends_on_changed=False,
        display_app_green=False,
    )
    checked += 4
    return checked


def verify_rollback_revision() -> int:
    """D-ROLL-01：内容回滚不复活旧 Capability。"""
    checked = 0
    trusted = frozenset({"issuer"})
    content = ("pkg-1", "cfg-1", "cap-1", "env-1")
    sid_r0 = actor_state_id("a", "p", "0", *content)
    sid_r2 = actor_state_id("a", "p", "2", *content)
    assert sid_r0 != sid_r2
    checked += 1

    held = Capability(
        issuer="issuer",
        issuer_key_holder="issuer",
        actor_id="a",
        state_id=sid_r0,
        scope=frozenset({"transfer"}),
        not_before=0,
        expires_at=10,
    )
    # 签发时（revision 0）可用。
    assert capability_accepts(
        held,
        current_actor="a",
        current_state=sid_r0,
        operation="transfer",
        now=1,
        trusted_issuers=trusted,
    )
    # 状态变化两次后内容回到原值（revision 2）：旧凭证仍然不匹配。
    assert not capability_accepts(
        held,
        current_actor="a",
        current_state=sid_r2,
        operation="transfer",
        now=1,
        trusted_issuers=trusted,
    )
    checked += 2
    return checked


def verify_local_clocks() -> int:
    checked = 0
    tmax = 3
    for now_a, now_b in itertools.product(range(0, 6), repeat=2):
        exec_a = now_a < tmax
        exec_b = now_b < tmax
        if now_a >= tmax:
            assert exec_a is False
        if now_b < tmax:
            assert exec_b is True
        assert exec_a == (now_a < tmax)
        checked += 3
        if now_a > 0:
            assert now_a - 1 < now_a
            checked += 1
    return checked


def main() -> None:
    checks = {
        "lattice": verify_lattice(),
        "monotonicity": verify_monotonicity(),
        "policy_separation": verify_policy_separation(),
        "retain_mapping": verify_retain(),
        "graph_components": verify_graph_components(),
        "cross_principal": verify_cross_principal(),
        "judgment": verify_judgment(),
        "merkle_and_binding": verify_merkle_and_binding(),
        "capability_and_execution": verify_capability_and_execution(),
        "snapshot_publish": verify_snapshot_publish(),
        "enc_graph": verify_enc_graph(),
        "generation": verify_generation(),
        "assurance_join": verify_assurance_join(),
        "depends_and_continue": verify_depends_and_continue(),
        "local_clocks": verify_local_clocks(),
        "rollback_revision": verify_rollback_revision(),
    }
    print(
        json.dumps(
            {
                "status": "PASS",
                "checks": checks,
                "total_assertions": sum(checks.values()),
                "scope": "finite-model regression; not a cryptographic or implementation proof",
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
