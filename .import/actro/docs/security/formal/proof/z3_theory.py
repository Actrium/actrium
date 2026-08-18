"""用 Z3 证明规范代数与执行前置。

量词由 SMT 处理。失败则抛出反例，不静默跳过。
"""

from __future__ import annotations

from z3 import And, ForAll, If, Implies, Ints, IntVal, Not, Or, Solver, sat, unsat


C, U, X = 0, 1, 2
ACTIVE, SUSPENDED = 0, 1


def join(left, right):
    return If(left > right, left, right)


def verdicts(*xs):
    return And(*[And(item >= C, item <= X) for item in xs])


def prove(name: str, formula) -> str:
    solver = Solver()
    solver.add(Not(formula))
    result = solver.check()
    if result != unsat:
        model = solver.model() if result == sat else None
        raise AssertionError(f"{name} 未证: {result} {model}")
    return name


def check_z3() -> dict[str, object]:
    a, b, c = Ints("a b c")
    proved: list[str] = []

    proved.append(
        prove(
            "join_comm",
            ForAll([a, b], Implies(verdicts(a, b), join(a, b) == join(b, a))),
        )
    )
    proved.append(
        prove(
            "join_assoc",
            ForAll(
                [a, b, c],
                Implies(verdicts(a, b, c), join(join(a, b), c) == join(a, join(b, c))),
            ),
        )
    )
    proved.append(
        prove("join_idem", ForAll([a], Implies(verdicts(a), join(a, a) == a)))
    )
    proved.append(
        prove(
            "thm3_x_dominates",
            ForAll(
                [a, b],
                Implies(And(verdicts(a, b), Or(a == X, b == X)), join(a, b) == X),
            ),
        )
    )
    proved.append(
        prove(
            "thm3_u_if_no_x",
            ForAll(
                [a, b],
                Implies(
                    And(verdicts(a, b), a != X, b != X, Or(a == U, b == U)),
                    join(a, b) == U,
                ),
            ),
        )
    )
    proved.append(
        prove("thm4_monotone", ForAll([a, b], Implies(verdicts(a, b), join(a, b) >= a)))
    )

    color, waiver, graph_ok, chan_ok, ready = Ints(
        "color waiver graph_ok chan_ok ready"
    )
    displayed = If(And(graph_ok == 1, chan_ok == 1, ready == 1), color, IntVal(-1))
    run_allowed = And(displayed != IntVal(-1), Or(displayed == C, waiver == 1))
    proved.append(
        prove(
            "invalid_not_waivable",
            ForAll(
                [color, waiver, graph_ok, chan_ok, ready],
                Implies(
                    And(verdicts(color), Or(graph_ok != 1, chan_ok != 1, ready != 1)),
                    Not(run_allowed),
                ),
            ),
        )
    )

    transfers, retain, out_graph, a2, current, rdy = Ints(
        "transfers retain out_graph a2 current rdy"
    )
    actor = join(join(transfers, retain), If(out_graph == 1, IntVal(X), IntVal(C)))
    shown = If(And(current == 1, rdy == 1), actor, IntVal(-1))
    leak = Or(transfers == X, retain == X, out_graph == 1, a2 != 1)
    proved.append(
        prove(
            "thm7_green_relative",
            ForAll(
                [transfers, retain, out_graph, a2, current, rdy],
                Implies(
                    And(
                        verdicts(transfers, retain),
                        out_graph >= 0,
                        out_graph <= 1,
                        a2 >= 0,
                        a2 <= 1,
                        shown == C,
                        current == 1,
                        rdy == 1,
                        a2 == 1,
                    ),
                    Not(leak),
                ),
            ),
        )
    )

    gate, sid, cap_sid, keys_for, now, tmax = Ints(
        "gate sid cap_sid keys_for now tmax"
    )
    exec_ok = And(gate == ACTIVE, sid == cap_sid, keys_for == sid, now < tmax)
    proved.append(
        prove(
            "thm10_old_cap_rejected",
            ForAll(
                [gate, sid, cap_sid, keys_for, now, tmax],
                Implies(sid != cap_sid, Not(exec_ok)),
            ),
        )
    )
    proved.append(
        prove(
            "thm13_suspended_blocked",
            ForAll(
                [sid, cap_sid, keys_for, now, tmax, gate],
                Implies(gate == SUSPENDED, Not(exec_ok)),
            ),
        )
    )
    proved.append(
        prove(
            "thm16_after_tmax",
            ForAll(
                [gate, sid, cap_sid, keys_for, now, tmax],
                Implies(now >= tmax, Not(exec_ok)),
            ),
        )
    )

    issuer, subject = Ints("issuer subject")
    accepted = issuer != subject
    proved.append(
        prove(
            "a11_no_self_sign",
            ForAll([issuer, subject], Implies(issuer == subject, Not(accepted))),
        )
    )

    leaf_sid, current_sid, aa_sid, any_suspended = Ints(
        "leaf_sid current_sid aa_sid any_suspended"
    )
    publish_ok = And(leaf_sid == current_sid, aa_sid == leaf_sid, any_suspended == 0)
    proved.append(
        prove(
            "thm15_mixed_or_suspended_unpublishable",
            ForAll(
                [leaf_sid, current_sid, aa_sid, any_suspended],
                Implies(
                    Or(leaf_sid != current_sid, aa_sid != leaf_sid, any_suspended != 0),
                    Not(publish_ok),
                ),
            ),
        )
    )

    a_sid, b_sid, a_now, b_now, mac_a, mac_b = Ints(
        "a_sid b_sid a_now b_now mac_a mac_b"
    )
    accept = And(mac_a == 1, mac_b == 1, a_sid == a_now, b_sid == b_now)
    proved.append(
        prove(
            "f_chan_01_replay_rejected",
            ForAll(
                [a_sid, b_sid, a_now, b_now, mac_a, mac_b],
                Implies(Or(a_sid != a_now, b_sid != b_now), Not(accept)),
            ),
        )
    )

    old_has, new_has = Ints("old_has new_has")
    allowed = And(old_has == 1, new_has == 1)
    proved.append(
        prove(
            "thm18_old_data_not_new_exit",
            ForAll(
                [old_has, new_has],
                Implies(
                    And(old_has == 0, new_has == 1),
                    Not(allowed),
                ),
            ),
        )
    )
    proved.append(
        prove(
            "thm14_allowed_intersection",
            ForAll(
                [old_has, new_has],
                Implies(
                    And(old_has >= 0, old_has <= 1, new_has >= 0, new_has <= 1),
                    allowed == And(old_has == 1, new_has == 1),
                ),
            ),
        )
    )

    left, right = Ints("aa_left aa_right")
    published = join(left, right)
    proved.append(
        prove(
            "a13_cannot_shop_stronger",
            ForAll(
                [left, right],
                Implies(verdicts(left, right), And(published >= left, published >= right)),
            ),
        )
    )
    has_aa, has_effects = Ints("has_aa has_effects")
    used = If(has_aa == 1, C, If(has_effects == 1, U, C))
    proved.append(
        prove(
            "a13_empty_accaa_not_green_if_effects",
            ForAll(
                [has_aa, has_effects],
                Implies(And(has_aa == 0, has_effects == 1), used == U),
            ),
        )
    )
    proved.append(
        prove(
            "a13_green_only_if_all_c",
            ForAll(
                [left, right],
                Implies(
                    And(verdicts(left, right), published == C),
                    And(left == C, right == C),
                ),
            ),
        )
    )

    depends, peer_sus = Ints("depends peer_sus")
    protected_ok = Not(And(depends == 1, peer_sus == 1))
    proved.append(
        prove(
            "thm20_depends_blocks",
            ForAll(
                [depends, peer_sus],
                Implies(And(depends == 1, peer_sus == 1), Not(protected_ok)),
            ),
        )
    )
    any_suspended, display_green = Ints("any_suspended display_green")
    ready_display = And(any_suspended == 0, display_green == 1)
    proved.append(
        prove(
            "thm20_continue_not_app_green",
            ForAll(
                [any_suspended, display_green],
                Implies(any_suspended != 0, Not(ready_display)),
            ),
        )
    )

    now_a, now_b, tmax = Ints("now_a now_b tmax")
    exec_a = now_a < tmax
    proved.append(
        prove(
            "thm21_local_tmax_ignores_peer",
            ForAll(
                [now_a, now_b, tmax],
                Implies(now_a >= tmax, Not(exec_a)),
            ),
        )
    )
    now_before, now_after = Ints("now_before now_after")
    tick_ok = now_after > now_before
    proved.append(
        prove(
            "thm21_clock_not_set_back",
            ForAll(
                [now_before, now_after],
                Implies(now_after < now_before, Not(tick_ok)),
            ),
        )
    )

    return {
        "status": "PASS",
        "solver": "z3",
        "proved": proved,
        "count": len(proved),
    }
