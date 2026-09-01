"""变异回归：证明每个检查器对坏定义具备检出能力。

原理：把 semantics.py（或个别模块）的守卫替换成一个已知错误的定义，
运行对应检查器，断言它必须 FAIL 或抛错；随后恢复原定义。任何一个
变异体存活（未被检出），整个套件 FAIL。

这是对「守卫即模型」类空洞检查的解药：守卫与不变量若是同一段代码，
变异守卫后不变量不报警，变异体存活，立刻暴露。
"""

from __future__ import annotations

from typing import Callable

try:
    from . import channel, semantics, sts, theory
except ImportError:  # pragma: no cover - 直接脚本运行
    import channel
    import semantics
    import sts
    import theory


def _detected(runner: Callable[[], dict]) -> bool:
    try:
        result = runner()
    except Exception:
        return True  # 断言炸掉同样算检出
    return result.get("status") != "PASS"


def _mutate(module, attr: str, replacement, runner) -> bool:
    original = getattr(module, attr)
    setattr(module, attr, replacement)
    try:
        return _detected(runner)
    finally:
        setattr(module, attr, original)


def _unilateral_verify(auth, expected_speaker, expected_tr):
    """F-CHAN-01 的坏定义：B 的 MAC 只覆盖自己一侧的 Hello。"""
    if expected_speaker == "B":
        return (
            auth.mac_from == "B"
            and auth.speaker == "B"
            and auth.transcript[3:] == expected_tr[3:]
        )
    return (
        auth.mac_from == expected_speaker
        and auth.speaker == expected_speaker
        and auth.transcript == expected_tr
    )


def check_mutants() -> dict[str, object]:
    mutations: list[tuple[str, Callable[[], bool]]] = [
        (
            "clocks-expiry-guard-removed",
            lambda: _mutate(
                semantics,
                "exec_time_ok",
                lambda now, tmax: True,
                sts.explore_clocks,
            ),
        ),
        (
            "clocks-rollback-allowed",
            lambda: _mutate(
                semantics,
                "tick_ok",
                lambda new, old: True,
                sts.explore_clocks,
            ),
        ),
        (
            "generation-intersection-dropped",
            lambda: _mutate(
                semantics,
                "allowed_effect",
                lambda in_current, in_acquired: in_current,
                sts.explore_generation,
            ),
        ),
        (
            "rollback-revision-ignored",
            lambda: _mutate(
                semantics,
                "cap_sid_match",
                lambda current, cap: current[-1] == cap[-1],
                sts.explore_rollback,
            ),
        ),
        (
            "ready-ignores-pointer",
            lambda: _mutate(
                sts,
                "ready",
                lambda state: state.app is not None
                and all(a.gate is sts.Gate.ACTIVE for a in state.actors),
                lambda: sts.explore(2),
            ),
        ),
        (
            "depends-guard-dropped",
            lambda: _mutate(
                sts,
                "exec_guard",
                lambda state, i: bool(
                    state.actors[i].gate is sts.Gate.ACTIVE
                    and state.actors[i].keys_current
                ),
                lambda: sts.explore(2),
            ),
        ),
        (
            "channel-unilateral-mac",
            lambda: _mutate(
                channel, "verify", _unilateral_verify, channel.check_channel
            ),
        ),
        (
            "aggregate-uses-min",
            lambda: _mutate(
                theory,
                "join",
                lambda a, b: a if a < b else b,
                theory.check_theory,
            ),
        ),
    ]
    survived = [name for name, run in mutations if not run()]
    return {
        "status": "PASS" if not survived else "FAIL",
        "mutants": len(mutations),
        "survived": survived,
    }
