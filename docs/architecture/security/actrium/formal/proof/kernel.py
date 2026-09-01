"""Results are certified: Theorems can only be derived from limited carrier determinations, Boolean determinations or registered justices.

An honest statement: That's not it. LCF kernel.`decide_finite` - Yes. Formula Just a label.
And the one that was judged. Python There is no machine to check between the terms; the semantic of the subject is the caller
. Prevents the word and document from drifting by mutants.py The mutation returns to bear,
This module ensures only that there is no "theorem" without a determination and without a just registration
It just came out of nowhere."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable


Formula = tuple


def atom(name: str, *args: object) -> Formula:
    return ("atom", name, args)


def land(*xs: Formula) -> Formula:
    acc: Formula = ("true",)
    for item in xs:
        acc = ("and", acc, item)
    return acc


def limpl(a: Formula, b: Formula) -> Formula:
    return ("imp", a, b)


def lnot(a: Formula) -> Formula:
    return ("not", a)


def forall(var: str, sort: str, body: Formula) -> Formula:
    return ("forall", var, sort, body)


def eq(a: object, b: object) -> Formula:
    return atom("eq", a, b)


@dataclass(frozen=True)
class Thm:
    hyps: frozenset[Formula]
    concl: Formula
    reason: str


class KernelError(Exception):
    pass


class Kernel:
    def __init__(self) -> None:
        self.axioms: dict[str, Formula] = {}
        self.theorems: dict[str, Thm] = {}

    def axiom(self, name: str, formula: Formula) -> Thm:
        if name in self.axioms and self.axioms[name] != formula:
            raise KernelError(f"axiom {name} Registered and different content")
        self.axioms[name] = formula
        thm = Thm(frozenset(), formula, f"axiom:{name}")
        self.theorems[name] = thm
        return thm

    def assume(self, formula: Formula) -> Thm:
        return Thm(frozenset({formula}), formula, "assume")

    def refl(self, value: object) -> Thm:
        return Thm(frozenset(), eq(value, value), "refl")

    def modus_ponens(self, imp: Thm, ant: Thm) -> Thm:
        if imp.concl[0] != "imp":
            raise KernelError("modus_ponens Need to have a sense of what you're doing.")
        _, left, right = imp.concl
        if left != ant.concl:
            raise KernelError("modus_ponens Previous does not match")
        return Thm(imp.hyps | ant.hyps, right, "mp")

    def and_intro(self, left: Thm, right: Thm) -> Thm:
        return Thm(left.hyps | right.hyps, ("and", left.concl, right.concl), "andI")

    def discharge(self, hyp: Formula, body: Thm) -> Thm:
        return Thm(body.hyps - {hyp}, limpl(hyp, body.concl), "impI")

    def lemma(self, name: str, thm: Thm) -> Thm:
        if thm.hyps:
            raise KernelError(f"{name} There are still unresolved assumptions.")
        self.theorems[name] = thm
        return thm

    def decide_finite(
        self,
        name: str,
        formula: Formula,
        domain: Iterable[object],
        predicate: Callable[[object], bool],
        *,
        var: str = "x",
        sort: str = "finite",
    ) -> Thm:
        """The full determination of the limited carrier is given the full term theorem."""
        items = list(domain)
        if not items:
            raise KernelError("The empty carrier cannot be declared full.")
        failures = [item for item in items if not predicate(item)]
        if failures:
            raise KernelError(f"{name} Failed to rule: {failures[:5]}")
        thm = Thm(frozenset(), formula, f"decide:{sort}:{len(items)}")
        self.theorems[name] = thm
        return thm

    def decide_bool(self, name: str, formula: Formula, holds: bool) -> Thm:
        if not holds:
            raise KernelError(f"{name} For fake")
        thm = Thm(frozenset(), formula, "decide:bool")
        self.theorems[name] = thm
        return thm

    def summary(self) -> dict[str, int]:
        return {
            "axioms": len(self.axioms),
            "theorems": len(self.theorems),
        }
