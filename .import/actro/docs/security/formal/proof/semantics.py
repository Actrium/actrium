"""共享守卫定义：有界 STS（具体布尔）与 Z3 层（符号布尔）的单一定义源。

约定：
- 函数体只用 `&`（合取）与比较组合参数；python bool 与 z3 表达式都支持。
- 不在这里做否定：需要 ¬ 时由调用方用各自逻辑（not / z3.Not）包裹。
- 调用方必须经 `semantics.fn(...)` 模块属性调用，不得 from-import 函数名：
  mutants.py 通过替换本模块属性验证各检查器对坏定义的检出能力。
"""

from __future__ import annotations


def exec_gate_ok(*, gate_active, keys_current, sid_match, time_ok):
    """Exec 前置 1–2 的守卫核心（model.zh.md §7.2）。"""
    return gate_active & keys_current & sid_match & time_ok


def exec_time_ok(now, tmax):
    """期限只看执行方本地时钟（Thm 16 / Thm 21）。"""
    return now < tmax


def tick_ok(new, old):
    """本地时钟只增不减（§7.6）。"""
    return new > old


def allowed_effect(in_current, in_acquired):
    """Allowed(a,d) = T_now ∩ T_gen 的单效果形式（Thm 14 / Thm 18）。"""
    return in_current & in_acquired


def cap_sid_match(current_sid, cap_sid):
    """Capability 状态绑定；revision 参与 sid，内容回滚不复活旧凭证（Thm 10）。"""
    return current_sid == cap_sid


def publish_leaf_ok(*, leaf_current, aa_matches_leaf, gate_active):
    """Publish 前置 2–4 的单叶守卫（Thm 15）。"""
    return leaf_current & aa_matches_leaf & gate_active


def ready_ok(*, has_app, pointer_current, none_suspended):
    """A12：对外展示颜色的前提。"""
    return has_app & pointer_current & none_suspended


def local_continue_ok(
    *, gate_active, aa_current, no_dependency_on_changed, not_displaying_green
):
    """LocalContinue（§7.5 / Thm 20）。"""
    return (
        gate_active & aa_current & no_dependency_on_changed & not_displaying_green
    )
