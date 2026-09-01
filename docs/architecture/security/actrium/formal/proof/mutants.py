"""Mute regression: Demonstrates that each inspector has the capability to detect the definition of bad.

Principle: Put semantics.pyThe guards (or individual modules) were replaced by a known definition of error.
Run the corresponding checker, say it must FAIL or drops the error; then restores the original definition.
Variable survival (not detected), whole package FAIL.

This is a cure for the "guard is model" type of empty-space inspection: if the guard is the same code as the non-variant, the security guard will be able to use the same code.
The mutate guards do not report the alarm, the mutate survives and is exposed immediately.
"""

from __future__ import annotations

from typing import Callable

try:
    from . import channel, semantics, sts, theory
except ImportError:  # pragma: no cover - Direct script running
    import channel
    import semantics
    import sts
    import theory


def _detected(runner: Callable[[], dict]) -> bool:
    try:
        result = runner()
    except Exception:
        return True  # It's the same thing that says blow up.
    return result.get("status") != "PASS"


def _mutate(module, attr: str, replacement, runner) -> bool:
    original = getattr(module, attr)
    setattr(module, attr, replacement)
    try:
        return _detected(runner)
    finally:
        setattr(module, attr, original)


def _unilateral_verify(auth, expected_speaker, expected_tr):
    """F-CHAN-01 ♪ Bad definition:B It's... MAC Only cover one side of the line. Hello."""
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
