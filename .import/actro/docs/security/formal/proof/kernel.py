"""结果登记核：定理条目只能来自有限载体判定、布尔判定或已登记公理。

诚实声明：这不是 LCF 内核。`decide_finite` 里的 Formula 只是标签，
与被判定的 Python 谓词之间没有机器可检查的对应；命题的语义在调用方
的谓词代码里。防止谓词与文档漂移由 mutants.py 的变异回归承担，
本模块只保证：没有任何「定理」是在既无判定、又无公理登记的情况下
凭空出现的。"""

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
            raise KernelError(f"axiom {name} 已登记且内容不同")
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
            raise KernelError("modus_ponens 需要蕴涵")
        _, left, right = imp.concl
        if left != ant.concl:
            raise KernelError("modus_ponens 前件不匹配")
        return Thm(imp.hyps | ant.hyps, right, "mp")

    def and_intro(self, left: Thm, right: Thm) -> Thm:
        return Thm(left.hyps | right.hyps, ("and", left.concl, right.concl), "andI")

    def discharge(self, hyp: Formula, body: Thm) -> Thm:
        return Thm(body.hyps - {hyp}, limpl(hyp, body.concl), "impI")

    def lemma(self, name: str, thm: Thm) -> Thm:
        if thm.hyps:
            raise KernelError(f"{name} 仍有未消解假设")
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
        """对有限载体做完全判定，得到全称定理。"""
        items = list(domain)
        if not items:
            raise KernelError("空载体不能判定全称")
        failures = [item for item in items if not predicate(item)]
        if failures:
            raise KernelError(f"{name} 判定失败: {failures[:5]}")
        thm = Thm(frozenset(), formula, f"decide:{sort}:{len(items)}")
        self.theorems[name] = thm
        return thm

    def decide_bool(self, name: str, formula: Formula, holds: bool) -> Thm:
        if not holds:
            raise KernelError(f"{name} 为假")
        thm = Thm(frozenset(), formula, "decide:bool")
        self.theorems[name] = thm
        return thm

    def summary(self) -> dict[str, int]:
        return {
            "axioms": len(self.axioms),
            "theorems": len(self.theorems),
        }
