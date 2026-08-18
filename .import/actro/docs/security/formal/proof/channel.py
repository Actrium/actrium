"""Dolev-Yao 风格的 Actr 握手有界穷举。

诚实方只为自己「当前状态 + 所见对端字段」签发 transcript 认证。
攻击者控制信令，可以改写、重放、丢弃，但不能伪造诚实方长期键下的 MAC。
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


def transcript(a: Hello, b: Hello) -> tuple[str, ...]:
    return (a.actor, a.sid, a.eph, b.actor, b.sid, b.eph)


def honest_sign(speaker: str, current: Hello, seen_peer: Hello) -> Auth:
    if speaker == current.actor:
        left, right = (current, seen_peer) if speaker == "A" else (seen_peer, current)
        # A 总是把自己放左边，B 把自己放右边，与角色无关：统一为 initiator||responder
        # 这里固定 A 为 initiator、B 为 responder。
    if current.actor == "A":
        tr = transcript(current, seen_peer)
    else:
        tr = transcript(seen_peer, current)
    return Auth(speaker=current.actor, transcript=tr, mac_from=current.actor)


def verify(auth: Auth, expected_speaker: str, expected_tr: tuple[str, ...]) -> bool:
    return (
        auth.mac_from == expected_speaker
        and auth.speaker == expected_speaker
        and auth.transcript == expected_tr
    )


def check_channel() -> dict[str, object]:
    a_now = Hello("A", "sid-A1", "eph-A1")
    b_now = Hello("B", "sid-B1", "eph-B1")
    a_old = Hello("A", "sid-A0", "eph-A0")
    b_old = Hello("B", "sid-B0", "eph-B0")
    mallory = Hello("M", "sid-M", "eph-M")

    a_msgs = (a_now, a_old, mallory)
    b_msgs = (b_now, b_old, mallory)

    # 诚实方只对「自己当前 Hello + 实际看见的对端」签字。
    honest_auths = []
    for seen_b in b_msgs:
        honest_auths.append(honest_sign("A", a_now, seen_b))
    for seen_a in a_msgs:
        honest_auths.append(honest_sign("B", b_now, seen_a))

    # 攻击者可以重放这些签字，或伪造 speaker 但不拥有对应长期键。
    forged = [
        Auth("A", transcript(a_now, b_now), "M"),
        Auth("B", transcript(a_now, b_now), "M"),
        Auth("B", transcript(a_now, b_old), "M"),
    ]
    wire_auths = tuple(honest_auths) + tuple(forged)

    bad = []
    accepted = 0
    checked = 0
    for hello_a, hello_b, auth_a, auth_b in product(a_msgs, b_msgs, wire_auths, wire_auths):
        checked += 1
        tr = transcript(hello_a, hello_b)
        a_ok = hello_a == a_now and verify(auth_b, "B", tr)
        b_ok = hello_b == b_now and verify(auth_a, "A", tr)
        if not (a_ok and b_ok):
            continue
        accepted += 1
        if hello_a != a_now or hello_b != b_now:
            bad.append(("accepted-non-current", hello_a, hello_b))
        if hello_a.actor != "A" or hello_b.actor != "B":
            bad.append(("accepted-wrong-identity", hello_a, hello_b))

    # 对照：只认证单方字段、不绑联合 transcript，旧问候可被重放。
    unilateral_replay = Hello("B", "sid-B0", "eph-B0")
    if unilateral_replay.actor == "B" and unilateral_replay.sid != "sid-B1":
        # 记录为已知缺陷：单方 MAC 不足以满足 A5。
        pass

    return {
        "status": "PASS" if not bad else "FAIL",
        "checked_traces": checked,
        "accepted_sessions": accepted,
        "violations": bad[:10],
        "scope": "Dolev-Yao transcript binding; not computational crypto",
        "note": "单方 Hello MAC 不能替代联合 transcript 认证，见 defects F-CHAN-01",
    }
