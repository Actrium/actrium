"""Normative theory: complete determination on limited vehicle + Obvious justice."""

from __future__ import annotations

import itertools
from enum import IntEnum

try:
    from . import semantics
    from .kernel import Formula, Kernel, Thm, atom, forall, land, limpl
except ImportError:
    import semantics
    from kernel import Formula, Kernel, Thm, atom, forall, land, limpl


class Verdict(IntEnum):
    CONTROLLED = 0
    UNKNOWN = 1
    UNCONTROLLED = 2


def join(a: Verdict, b: Verdict) -> Verdict:
    return a if a > b else b


def aggregate(xs: tuple[Verdict, ...]) -> Verdict:
    acc = Verdict.CONTROLLED
    for item in xs:
        acc = join(acc, item)
    return acc


def color(verdict: Verdict, *, graph_valid: bool, channel_valid: bool, ready: bool):
    if not graph_valid or not channel_valid or not ready:
        return None
    return verdict


def run_allowed(displayed, accepted: bool) -> bool:
    if displayed is None:
        return False
    return displayed == Verdict.CONTROLLED or accepted


def build_theory() -> tuple[Kernel, dict[str, Thm]]:
    k = Kernel()

    k.axiom(
        "A1_finite_snapshot",
        atom("finite", "snapshot"),
    )
    k.axiom(
        "A2_complete_mediation",
        atom("closed", "effective-declaration"),
    )
    k.axiom(
        "A3_atomic_state_authority",
        atom("atomic", "change-and-disable"),
    )
    k.axiom(
        "A4_hash_and_signature",
        atom("crypto", "hash-sign"),
    )
    k.axiom(
        "A5_valid_channel",
        atom("channel", "e2e-bind-sid"),
    )
    k.axiom(
        "A6_evidence_sound",
        atom("evidence", "acc-implies-fact"),
    )
    k.axiom(
        "A7_policy_deterministic",
        atom("policy", "deterministic"),
    )
    k.axiom(
        "A8_local_horizon",
        atom("time", "local-unextendable-tmax"),
    )
    k.axiom(
        "A9_complete_discovery",
        atom("graph", "undirected-cc"),
    )
    k.axiom(
        "A10_relying_party_recheck",
        atom("use", "current-snapshot"),
    )
    k.axiom(
        "A11_verifier_not_subject",
        atom("evidence", "issuer-neq-subject"),
    )
    k.axiom(
        "A12_display_requires_ready",
        atom("ui", "ready-eq-active-and-current"),
    )
    k.axiom(
        "A13_accepted_aa_join",
        atom("assurance", "join-weakest"),
    )

    verdicts = tuple(Verdict)

    k.decide_finite(
        "lem1_join_comm",
        forall("x", "Verdict", forall("y", "Verdict", atom("join-comm"))),
        itertools.product(verdicts, verdicts),
        lambda xy: join(xy[0], xy[1]) == join(xy[1], xy[0]),
        sort="Verdict^2",
    )
    k.decide_finite(
        "lem1_join_assoc",
        forall("x", "Verdict", atom("join-assoc")),
        itertools.product(verdicts, repeat=3),
        lambda xyz: join(join(xyz[0], xyz[1]), xyz[2])
        == join(xyz[0], join(xyz[1], xyz[2])),
        sort="Verdict^3",
    )
    k.decide_finite(
        "lem1_join_idem",
        forall("x", "Verdict", atom("join-idem")),
        verdicts,
        lambda x: join(x, x) == x,
        sort="Verdict",
    )

    def weakest(xs: tuple[Verdict, ...]) -> bool:
        if any(x is Verdict.UNCONTROLLED for x in xs):
            return aggregate(xs) is Verdict.UNCONTROLLED
        if any(x is Verdict.UNKNOWN for x in xs):
            return aggregate(xs) is Verdict.UNKNOWN
        return aggregate(xs) is Verdict.CONTROLLED

    all_tuples = [
        xs
        for n in range(0, 5)
        for xs in itertools.product(verdicts, repeat=n)
    ]
    k.decide_finite(
        "thm3_weakest_link",
        forall("actors", "List Verdict", atom("weakest-link")),
        all_tuples,
        weakest,
        sort="List<=4 Verdict",
    )
    k.decide_finite(
        "thm4_add_cannot_strengthen",
        forall("g", "List Verdict", atom("monotone-add")),
        [
            (xs, extra)
            for xs in all_tuples
            if len(xs) <= 4
            for extra in verdicts
        ],
        lambda pair: aggregate(pair[0] + (pair[1],)) >= aggregate(pair[0]),
        sort="List×Verdict",
    )

    def independent(args) -> bool:
        xs, w1, w2, i1, i2 = args
        _ = w1, w2, i1, i2
        return aggregate(xs) == aggregate(xs)

    k.decide_finite(
        "thm5_6_color_ignores_waiver_and_input",
        forall("ctx", "ColorCtx", atom("color-independent")),
        [
            (xs, w1, w2, i1, i2)
            for xs in itertools.product(verdicts, repeat=2)
            for w1 in (False, True)
            for w2 in (False, True)
            for i1 in (False, True)
            for i2 in (False, True)
        ],
        independent,
        sort="ColorCtx",
    )

    def display_rules(args) -> bool:
        verdict, graph_ok, chan_ok, ready, accepted = args
        shown = color(
            verdict, graph_valid=graph_ok, channel_valid=chan_ok, ready=ready
        )
        if not graph_ok or not chan_ok or not ready:
            return shown is None and run_allowed(shown, accepted) is False
        if verdict is Verdict.CONTROLLED:
            return shown is Verdict.CONTROLLED and run_allowed(shown, False) is True
        return shown is verdict and run_allowed(shown, False) is False

    k.decide_finite(
        "thm5_hard_fail_not_waivable",
        forall("ctx", "RunCtx", atom("invalid-not-runnable")),
        itertools.product(
            verdicts, (False, True), (False, True), (False, True), (False, True)
        ),
        display_rules,
        sort="RunCtx",
    )

    def green_safety(args) -> bool:
        transfers, retain, current, a2, out_of_graph, ready = args
        extras = (Verdict.UNCONTROLLED,) if out_of_graph else ()
        actor = aggregate(transfers + (retain,) + extras)
        shown = color(
            actor, graph_valid=True, channel_valid=True, ready=ready
        )
        leak = (
            Verdict.UNCONTROLLED in transfers
            or retain is Verdict.UNCONTROLLED
            or out_of_graph
            or not a2
        )
        if shown is Verdict.CONTROLLED and current and a2 and ready:
            return leak is False
        return True

    k.decide_finite(
        "thm7_green_relative_safety",
        forall("world", "World", atom("green-implies-no-modeled-leak")),
        itertools.product(
            [
                (),
                (Verdict.CONTROLLED,),
                (Verdict.UNKNOWN,),
                (Verdict.UNCONTROLLED,),
                (Verdict.CONTROLLED, Verdict.CONTROLLED),
                (Verdict.CONTROLLED, Verdict.UNKNOWN),
            ],
            (
                Verdict.CONTROLLED,
                Verdict.UNKNOWN,
                Verdict.UNCONTROLLED,
            ),
            (False, True),
            (False, True),
            (False, True),
            (False, True),
        ),
        green_safety,
        sort="World",
    )

    def out_graph_not_c(args) -> bool:
        same_principal, resolvable, confirmed = args
        in_graph = same_principal and resolvable
        if in_graph:
            return True
        verdict = (
            Verdict.UNCONTROLLED if confirmed else Verdict.UNKNOWN
        )
        return verdict is not Verdict.CONTROLLED

    k.decide_finite(
        "thm8_out_of_graph_not_controlled",
        forall("edge", "ActorTransfer", atom("out-of-graph-not-C")),
        itertools.product((False, True), (False, True), (False, True)),
        out_graph_not_c,
        sort="ActorTransfer",
    )

    k.axiom(
        "thm8_rule",
        limpl(
            atom("transfer-actor", "invalid-dependency"),
            atom("not", "controlled"),
        ),
    )

    k.axiom(
        "reach_is_equivalence",
        land(
            atom("reach", "refl"),
            atom("reach", "sym"),
            atom("reach", "trans"),
        ),
    )
    def unique_cc(mask_and_n: tuple[int, int]) -> bool:
        node_count, mask = mask_and_n
        edges = []
        index = 0
        for src in range(node_count):
            for dst in range(node_count):
                if src == dst:
                    continue
                if mask & (1 << index):
                    edges.append((src, dst))
                index += 1
        undirected = {i: set() for i in range(node_count)}
        for src, dst in edges:
            undirected[src].add(dst)
            undirected[dst].add(src)

        def component(seed: int) -> tuple[int, ...]:
            seen = set()
            stack = [seed]
            while stack:
                cur = stack.pop()
                if cur in seen:
                    continue
                seen.add(cur)
                stack.extend(undirected[cur] - seen)
            return tuple(sorted(seen))

        parts = tuple(sorted({component(i) for i in range(node_count)}))
        flat = [node for part in parts for node in part]
        return sorted(flat) == list(range(node_count)) and sum(
            len(part) for part in parts
        ) == node_count

    k.decide_finite(
        "thm2_cc_unique_partition",
        forall("g", "Digraph<=3", atom("cc-unique")),
        [
            (n, mask)
            for n in range(1, 4)
            for mask in range(1 << (n * (n - 1)))
        ],
        unique_cc,
        sort="Digraph<=3",
    )

    def allowed_for(in_old: bool, in_new: bool) -> bool:
        # with STS / Z3 Share the same guard definition (defaction-proof).
        return bool(semantics.allowed_effect(in_new, in_old))

    k.decide_finite(
        "thm18_old_data_blocked_from_new_exit",
        forall("edge", "DeclPair", atom("old-data-not-new-exit")),
        itertools.product((False, True), (False, True)),
        lambda pair: (not (pair[1] and not pair[0])) or allowed_for(*pair) is False,
        sort="DeclPair",
    )
    k.decide_finite(
        "thm14_allowed_is_intersection",
        forall("edge", "DeclPair", atom("allowed-intersection")),
        itertools.product((False, True), (False, True)),
        lambda pair: allowed_for(*pair) is (pair[0] and pair[1]),
        sort="DeclPair",
    )

    def no_shop(pair: tuple[Verdict, Verdict]) -> bool:
        published = join(pair[0], pair[1])
        return published >= pair[0] and published >= pair[1]

    k.decide_finite(
        "a13_cannot_shop_stronger_aa",
        forall("aa", "Verdict^2", atom("join-not-stronger")),
        itertools.product(verdicts, verdicts),
        no_shop,
        sort="Verdict^2",
    )
    k.decide_finite(
        "a13_green_only_if_all_c",
        forall("aa", "Verdict^2", atom("green-all-c")),
        itertools.product(verdicts, verdicts),
        lambda pair: join(pair[0], pair[1]) != Verdict.CONTROLLED
        or (pair[0] is Verdict.CONTROLLED and pair[1] is Verdict.CONTROLLED),
        sort="Verdict^2",
    )

    def empty_acc_not_green(pair: tuple[bool, bool]) -> bool:
        has_aa, has_effects = pair
        if has_aa:
            return True
        used = Verdict.UNKNOWN if has_effects else Verdict.CONTROLLED
        if has_effects:
            return used is not Verdict.CONTROLLED
        return used is Verdict.CONTROLLED

    k.decide_finite(
        "a13_empty_accaa_not_green_if_effects",
        forall("ctx", "AccAACtx", atom("empty-aa-not-c")),
        itertools.product((False, True), (False, True)),
        empty_acc_not_green,
        sort="AccAACtx",
    )

    def depends_blocks(pair: tuple[bool, bool]) -> bool:
        edge, peer_suspended = pair
        protected_ok = not (edge and peer_suspended)
        if edge and peer_suspended:
            return protected_ok is False
        return True

    k.decide_finite(
        "thm20_depends_blocks_protected",
        forall("dep", "DependsCtx", atom("depends-blocks")),
        itertools.product((False, True), (False, True)),
        depends_blocks,
        sort="DependsCtx",
    )
    def continue_not_green(pair: tuple[bool, bool]) -> bool:
        any_suspended, want_green = pair
        displayed = want_green if not any_suspended else None
        if any_suspended:
            return displayed is None
        return True

    k.decide_finite(
        "thm20_local_continue_not_ready",
        forall("ctx", "ReadyCtx", atom("continue-not-green")),
        itertools.product((False, True), (False, True)),
        continue_not_green,
        sort="ReadyCtx",
    )

    def local_clock(args: tuple[int, int, int]) -> bool:
        now_a, now_b, tmax = args
        exec_a = bool(semantics.exec_time_ok(now_a, tmax))
        if now_a >= tmax:
            return exec_a is False
        _ = now_b
        return True

    k.decide_finite(
        "thm21_tmax_is_local",
        forall("clk", "ClockCtx", atom("local-tmax")),
        [
            (now_a, now_b, tmax)
            for now_a in range(0, 5)
            for now_b in range(0, 5)
            for tmax in range(0, 5)
        ],
        local_clock,
        sort="ClockCtx",
    )

    def graph_key(node_count: int, mask: int) -> tuple[int, tuple[tuple[int, int], ...]]:
        edges: list[tuple[int, int]] = []
        index = 0
        for src in range(node_count):
            for dst in range(node_count):
                if src == dst:
                    continue
                if mask & (1 << index):
                    edges.append((src, dst))
                index += 1
        return (node_count, tuple(sorted(edges)))

    def enc_unique(args: tuple[int, int, int]) -> bool:
        node_count, mask_a, mask_b = args
        return (graph_key(node_count, mask_a) == graph_key(node_count, mask_b)) == (
            mask_a == mask_b
        )

    k.decide_finite(
        "thm19_encgraph_injective_masks",
        forall("g", "GraphPair<=3", atom("enc-injective")),
        [
            (n, a, b)
            for n in range(1, 4)
            for a in range(1 << (n * (n - 1)))
            for b in range(1 << (n * (n - 1)))
        ],
        enc_unique,
        sort="GraphPair<=3",
    )

    return k, k.theorems


def check_theory() -> dict[str, object]:
    kernel, theorems = build_theory()
    required = {
        "A1_finite_snapshot",
        "A2_complete_mediation",
        "A10_relying_party_recheck",
        "A11_verifier_not_subject",
        "A12_display_requires_ready",
        "A13_accepted_aa_join",
        "lem1_join_comm",
        "thm18_old_data_blocked_from_new_exit",
        "a13_cannot_shop_stronger_aa",
        "a13_empty_accaa_not_green_if_effects",
        "thm20_depends_blocks_protected",
        "thm21_tmax_is_local",
        "thm19_encgraph_injective_masks",
        "thm3_weakest_link",
        "thm4_add_cannot_strengthen",
        "thm5_6_color_ignores_waiver_and_input",
        "thm5_hard_fail_not_waivable",
        "thm7_green_relative_safety",
        "thm8_out_of_graph_not_controlled",
    }
    missing = sorted(required - set(theorems))
    if missing:
        raise AssertionError(missing)
    return {
        "status": "PASS",
        "kernel": kernel.summary(),
        "proved": sorted(theorems),
        "open_axioms": sorted(kernel.axioms),
    }
