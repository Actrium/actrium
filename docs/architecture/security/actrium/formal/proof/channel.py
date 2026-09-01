"""Dolev-Yao Style Actr The handshake is a massive one (side-side independent view).

The honest side is only for "your present." Hello + The opposite of the one you actually see. Hello* The United Nations
transcript MACThe assailant has complete control of the command: it can be injected, re-played, discarded and it can be
Show them on each side. Hello with Auth(A, B ; the assailant
And the honest parties who actually signed the old speech. MAC(re-display material) but cannot forge honesty
New under the square long-term key MAC.

The key differences with the old version:
- Both sides stand alone. "When both sides accept transcript The "unanimous" is the result of a test.
  It's no longer a prerequisite for a priori assurance of modelling. / UKS (a) Within the state space;
- The attacker's text contains the truth of the old conversation. MACUnion transcript It's new in the middle.
  Temporary public key is the mechanism for rejecting them (F-CHAN-01 ; and
- Unilateral Hello MAC ..the definition of bad by mutants.py Injecting and verifying can be detected.

Range remains a symbol model: no code of calculation, specific package delivery Tamarin/ProVerif.
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
    """Honesty signature: binding your current Hello Joint with the opposite side of the picture. transcript."""
    if current.actor == "A":
        joint = transcript(current, seen_peer)
    else:
        joint = transcript(seen_peer, current)
    return Auth(speaker=current.actor, transcript=joint, mac_from=current.actor)


def verify(auth: Auth, expected_speaker: str, expected_tr: tuple[str, ...]) -> bool:
    """Receiving Party Verifying:MAC Source and integrity transcript It has to match."""
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

    a_candidates = (a_now, a_old, mal_a)  # B "may be presented."A It's... Hello”
    b_candidates = (b_now, b_old, mal_b)  # A "may be presented."B It's... Hello”

    # Current session honestly signed: each current Hello × Seen the opposite.
    wire: list[Auth] = []
    for seen_b in b_candidates:
        wire.append(honest_auth(a_now, seen_b))
    for seen_a in a_candidates:
        wire.append(honest_auth(b_now, seen_a))
    # The old speech is re-posted: the old era was signed by both sides. MAC(Current candidate)
    # Only what was there. Hello;a_now/b_now The new temporary public key did not exist at the time.
    for seen_a in (a_old, mal_a):
        wire.append(honest_auth(b_old, seen_a))
    for seen_b in (b_old, mal_b):
        wire.append(honest_auth(a_old, seen_b))
    # Counterfeiting material: the assailant can spell it out as it pleases. transcript, but MAC Only from your own key.
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
        tr_a = transcript(a_now, a_view)  # A The desired coalition transcript
        tr_b = transcript(b_view, b_now)  # B The desired coalition transcript
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
