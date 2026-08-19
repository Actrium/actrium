"""Dolev-Yao 风格的 Actr 握手有界穷举（双侧独立视图）。

诚实方只为「自己当前 Hello + 自己实际看见的对端 Hello」签发联合
transcript MAC。攻击者完全控制信令：可注入、重放、丢弃，并且可以
对两侧分别出示不同的 Hello 与 Auth（A、B 的视图独立枚举）；攻击者
还持有旧会话里诚实方真实签发过的 MAC（重放材料），但不能伪造诚实
方长期键下的新 MAC。

与旧版的关键差别：
- 两侧视图独立，「双方接受后 transcript 一致」是被检验的结论，
  不再是建模方式先验保证的前提（身份误绑 / UKS 在状态空间内）；
- 攻击者语料包含旧会话的真实 MAC，联合 transcript 中的本方新鲜
  临时公钥是拒绝它们的机制（F-CHAN-01 的正向验证）；
- 单方 Hello MAC 的坏定义由 mutants.py 注入并验证可被检出。

范围仍是符号模型：不含计算密码归约，具体套件交 Tamarin/ProVerif。
"""

from __future__ import annotations

from dataclasses import dataclass
from itertools import product


@dataclass(frozen=True)
class Hello:
    actor: str
    sid: str
    eph: str


@dataclass(frozen=True)
class Auth:
    speaker: str
    transcript: tuple[str, ...]
    mac_from: str


def transcript(initiator: Hello, responder: Hello) -> tuple[str, ...]:
    return (
        initiator.actor,
        initiator.sid,
        initiator.eph,
        responder.actor,
        responder.sid,
        responder.eph,
    )


def honest_auth(current: Hello, seen_peer: Hello) -> Auth:
    """诚实方签名：绑定自己当前 Hello 与所见对端的联合 transcript。"""
    if current.actor == "A":
        joint = transcript(current, seen_peer)
    else:
        joint = transcript(seen_peer, current)
    return Auth(speaker=current.actor, transcript=joint, mac_from=current.actor)


def verify(auth: Auth, expected_speaker: str, expected_tr: tuple[str, ...]) -> bool:
    """接收方核验：MAC 来源与完整联合 transcript 都必须匹配。"""
    return (
        auth.mac_from == expected_speaker
        and auth.speaker == expected_speaker
        and auth.transcript == expected_tr
    )


def check_channel() -> dict[str, object]:
    a_now = Hello("A", "sid-A1", "eph-A1")
    a_old = Hello("A", "sid-A0", "eph-A0")
    mal_a = Hello("M", "sid-M", "eph-MA")
    b_now = Hello("B", "sid-B1", "eph-B1")
    b_old = Hello("B", "sid-B0", "eph-B0")
    mal_b = Hello("M", "sid-M", "eph-MB")

    a_candidates = (a_now, a_old, mal_a)  # B 可能被出示的「A 的 Hello」
    b_candidates = (b_now, b_old, mal_b)  # A 可能被出示的「B 的 Hello」

    # 当前会话诚实签名：各自当前 Hello × 所见对端。
    wire: list[Auth] = []
    for seen_b in b_candidates:
        wire.append(honest_auth(a_now, seen_b))
    for seen_a in a_candidates:
        wire.append(honest_auth(b_now, seen_a))
    # 旧会话重放语料：旧纪元里双方真实签过的 MAC（当时的对端候选
    # 只包含当时存在的 Hello；a_now/b_now 的新鲜临时公钥当时不存在）。
    for seen_a in (a_old, mal_a):
        wire.append(honest_auth(b_old, seen_a))
    for seen_b in (b_old, mal_b):
        wire.append(honest_auth(a_old, seen_b))
    # 伪造语料：攻击者可拼任意 transcript，但 MAC 只能出自自己的键。
    forged_trs = sorted(
        {transcript(x, y) for x in a_candidates for y in b_candidates}
    )
    for speaker in ("A", "B"):
        for joint in forged_trs:
            wire.append(Auth(speaker=speaker, transcript=joint, mac_from="M"))
    wire_auths = tuple(wire)

    bad: list[tuple] = []
    accepted = 0
    checked = 0
    for a_view, b_view, auth_to_a, auth_to_b in product(
        b_candidates, a_candidates, wire_auths, wire_auths
    ):
        checked += 1
        tr_a = transcript(a_now, a_view)  # A 期望的联合 transcript
        tr_b = transcript(b_view, b_now)  # B 期望的联合 transcript
        a_ok = verify(auth_to_a, "B", tr_a)
        b_ok = verify(auth_to_b, "A", tr_b)
        if a_ok and (a_view.actor != "B" or a_view.sid != b_now.sid):
            bad.append(("A-accepted-non-current-peer", a_view))
        if b_ok and (b_view.actor != "A" or b_view.sid != a_now.sid):
            bad.append(("B-accepted-non-current-peer", b_view))
        if a_ok and b_ok:
            accepted += 1
            if tr_a != tr_b:
                bad.append(("transcript-disagreement", tr_a, tr_b))
    return {
        "status": "PASS" if not bad else "FAIL",
        "checked_traces": checked,
        "accepted_sessions": accepted,
        "violations": bad[:10],
        "scope": (
            "Dolev-Yao joint-transcript binding, independent per-side views, "
            "replayed old-session MACs; not computational crypto"
        ),
    }
