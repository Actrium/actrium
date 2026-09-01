"""用 Z3 证明规范代数与执行前置。

守卫公式不再各自内联重写：凡有共享定义的，都从 semantics.py 构造
（与有界 STS 同一定义源），Z3 证明因此锁定的是可执行模型实际使用的
那份守卫，而不是一份平行手写副本。纯代数性质（三值格、聚合、A13）
仍在本文件内定义并证明。

失败即抛出反例，不静默跳过。
"""

from __future__ import annotations

from z3 import (
    And,
    Bool,
    ForAll,
    If,
    Implies,
    Ints,
    IntVal,
    Not,
    Or,
    Solver,
    sat,
    unsat,
)

try:
    from . import semantics
except ImportError:  # pragma: no cover - 直接脚本运行
    import semantics


C, U, X = 0, 1, 2


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

    # --- 三值格（纯代数，本文件定义） ---
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

    # --- A12：展示前提（semantics.ready_ok） ---
    has_app = Bool("has_app")
    pointer_current = Bool("pointer_current")
    none_suspended = Bool("none_suspended")
    ready = semantics.ready_ok(
        has_app=has_app,
        pointer_current=pointer_current,
        none_suspended=none_suspended,
    )
    proved.append(
        prove(
            "a12_ready_requires_all",
            ForAll(
                [has_app, pointer_current, none_suspended],
                Implies(
                    Or(Not(has_app), Not(pointer_current), Not(none_suspended)),
                    Not(ready),
                ),
            ),
        )
    )

    color, waiver, graph_ok, chan_ok, rdy_flag = Ints(
        "color waiver graph_ok chan_ok rdy_flag"
    )
    displayed_val = If(
        And(graph_ok == 1, chan_ok == 1, rdy_flag == 1), color, IntVal(-1)
    )
    run_allowed = And(displayed_val != IntVal(-1), Or(displayed_val == C, waiver == 1))
    proved.append(
        prove(
            "invalid_not_waivable",
            ForAll(
                [color, waiver, graph_ok, chan_ok, rdy_flag],
                Implies(
                    And(verdicts(color), Or(graph_ok != 1, chan_ok != 1, rdy_flag != 1)),
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

    # --- Exec 守卫（semantics.exec_gate_ok / exec_time_ok） ---
    gate_active = Bool("gate_active")
    keys_current = Bool("keys_current")
    sid, cap_sid, now, tmax = Ints("sid cap_sid now tmax")
    exec_ok = semantics.exec_gate_ok(
        gate_active=gate_active,
        keys_current=keys_current,
        sid_match=semantics.cap_sid_match(sid, cap_sid),
        time_ok=semantics.exec_time_ok(now, tmax),
    )
    proved.append(
        prove(
            "thm10_old_cap_rejected",
            ForAll(
                [gate_active, keys_current, sid, cap_sid, now, tmax],
                Implies(sid != cap_sid, Not(exec_ok)),
            ),
        )
    )
    proved.append(
        prove(
            "thm13_suspended_blocked",
            ForAll(
                [gate_active, keys_current, sid, cap_sid, now, tmax],
                Implies(Not(gate_active), Not(exec_ok)),
            ),
        )
    )
    proved.append(
        prove(
            "thm16_after_tmax",
            ForAll(
                [gate_active, keys_current, sid, cap_sid, now, tmax],
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

    # --- Publish 前置（semantics.publish_leaf_ok） ---
    leaf_current = Bool("leaf_current")
    aa_matches = Bool("aa_matches")
    leaf_gate_active = Bool("leaf_gate_active")
    publish_ok = semantics.publish_leaf_ok(
        leaf_current=leaf_current,
        aa_matches_leaf=aa_matches,
        gate_active=leaf_gate_active,
    )
    proved.append(
        prove(
            "thm15_mixed_or_suspended_unpublishable",
            ForAll(
                [leaf_current, aa_matches, leaf_gate_active],
                Implies(
                    Or(Not(leaf_current), Not(aa_matches), Not(leaf_gate_active)),
                    Not(publish_ok),
                ),
            ),
        )
    )

    a_sid, b_sid, a_now_v, b_now_v, mac_a, mac_b = Ints(
        "a_sid b_sid a_now b_now mac_a mac_b"
    )
    accept = And(mac_a == 1, mac_b == 1, a_sid == a_now_v, b_sid == b_now_v)
    proved.append(
        prove(
            "f_chan_01_replay_rejected",
            ForAll(
                [a_sid, b_sid, a_now_v, b_now_v, mac_a, mac_b],
                Implies(Or(a_sid != a_now_v, b_sid != b_now_v), Not(accept)),
            ),
        )
    )

    # --- 数据世代（semantics.allowed_effect） ---
    in_current = Bool("in_current")
    in_acquired = Bool("in_acquired")
    allowed = semantics.allowed_effect(in_current, in_acquired)
    proved.append(
        prove(
            "thm18_old_data_not_new_exit",
            ForAll(
                [in_current, in_acquired],
                Implies(And(in_current, Not(in_acquired)), Not(allowed)),
            ),
        )
    )
    proved.append(
        prove(
            "thm14_allowed_intersection",
            ForAll(
                [in_current, in_acquired],
                allowed == And(in_current, in_acquired),
            ),
        )
    )

    # --- A13 ---
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
    has_aa, has_effects, acc_join = Ints("has_aa has_effects acc_join")
    used = If(has_aa == 1, acc_join, If(has_effects == 1, IntVal(U), IntVal(C)))
    proved.append(
        prove(
            "a13_empty_accaa_not_green_if_effects",
            ForAll(
                [has_aa, has_effects, acc_join],
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

    # --- LocalContinue（semantics.local_continue_ok） ---
    lc_gate = Bool("lc_gate")
    lc_aa = Bool("lc_aa")
    lc_no_dep = Bool("lc_no_dep")
    lc_not_green = Bool("lc_not_green")
    lc = semantics.local_continue_ok(
        gate_active=lc_gate,
        aa_current=lc_aa,
        no_dependency_on_changed=lc_no_dep,
        not_displaying_green=lc_not_green,
    )
    proved.append(
        prove(
            "thm20_depends_blocks",
            ForAll(
                [lc_gate, lc_aa, lc_no_dep, lc_not_green],
                Implies(Not(lc_no_dep), Not(lc)),
            ),
        )
    )
    proved.append(
        prove(
            "thm20_continue_not_app_green",
            ForAll(
                [lc_gate, lc_aa, lc_no_dep, lc_not_green],
                Implies(Not(lc_not_green), Not(lc)),
            ),
        )
    )

    # --- 本地时钟（semantics.exec_time_ok / tick_ok） ---
    now_a, now_b, tmax2 = Ints("now_a now_b tmax2")
    exec_a = semantics.exec_time_ok(now_a, tmax2)
    proved.append(
        prove(
            "thm21_local_tmax_ignores_peer",
            ForAll(
                [now_a, now_b, tmax2],
                Implies(now_a >= tmax2, Not(exec_a)),
            ),
        )
    )
    now_before, now_after = Ints("now_before now_after")
    tick = semantics.tick_ok(now_after, now_before)
    proved.append(
        prove(
            "thm21_clock_not_set_back",
            ForAll(
                [now_before, now_after],
                Implies(now_after < now_before, Not(tick)),
            ),
        )
    )

    return {
        "status": "PASS",
        "solver": "z3",
        "proved": proved,
        "count": len(proved),
        "shared_semantics": True,
    }
