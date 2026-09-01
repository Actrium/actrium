"""Shared Guard Definition: There is a boundary STS(Specific Boolean) and Z3 . A single defined source for layers (the symbol Boolean).

Have agreed:
- Function only `&`(Assist) and compare parameters;python bool with z3 Expressions are supported.
- Not here to deny: need ¬ When the caller uses its own logic (not / z3.NotThe package.
- Caller must pass `semantics.fn(...)` Module Properties Call, Not from-import Function name:
  mutants.py Verifys the check-out capability of the checkers for the bad definition by replacing this module property.
"""

from __future__ import annotations


def exec_gate_ok(*, gate_active, keys_current, sid_match, time_ok):
    """Exec Prefix 1–2 The Guard Core (A)model.zh.md §7.2)."""
    return gate_active & keys_current & sid_match & time_ok


def exec_time_ok(now, tmax):
    """Duration only (in local clock)Thm 16 / Thm 21)."""
    return now < tmax


def tick_ok(new, old):
    """Local clocks only add to and fall (§7.6)."""
    return new > old


def allowed_effect(in_current, in_acquired):
    """Allowed(a,d) = T_now ∩ T_gen Single Effect Form (Thm 14 / Thm 18)."""
    return in_current & in_acquired


def cap_sid_match(current_sid, cap_sid):
    """Capability (a) State binding;revision Participation sid, content rolls back and does not revive old documents (Thm 10)."""
    return current_sid == cap_sid


def publish_leaf_ok(*, leaf_current, aa_matches_leaf, gate_active):
    """Publish Prefix 2–4 The One-leaf Guard (in the past)Thm 15)."""
    return leaf_current & aa_matches_leaf & gate_active


def ready_ok(*, has_app, pointer_current, none_suspended):
    """A12: Shows the colour prefix to the public."""
    return has_app & pointer_current & none_suspended


def local_continue_ok(
    *, gate_active, aa_current, no_dependency_on_changed, not_displaying_green
):
    """LocalContinue(§7.5 / Thm 20)."""
    return (
        gate_active & aa_current & no_dependency_on_changed & not_displaying_green
    )
