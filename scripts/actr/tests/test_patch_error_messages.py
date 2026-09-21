#!/usr/bin/env python3
"""Regression coverage for binding regeneration; no native runtime needed."""

import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "patch_error_messages", ROOT / "scripts/actr/patch-error-messages.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
KOTLIN = ROOT / "packages/actr-kotlin/actr-kotlin/src/main/kotlin/io/actrium/actr/actr.kt"
# Swift bindings are generated at build time; use a focused template fixture here.
SWIFT = ROOT / "scripts/actr/tests/fixtures/ActrError.swift"


class PatchErrorMessagesTests(unittest.TestCase):
    def test_binding_and_fixture_are_patched_and_idempotent(self):
        for language, path in (("kotlin", KOTLIN), ("swift", SWIFT)):
            with self.subTest(language=language):
                source = path.read_text()
                self.assertEqual(MODULE.patch(source, language), source)

    def test_regeneration_restores_message_getters(self):
        for language, path, patched, generated in (
            ("kotlin", KOTLIN, "get() = actrErrorMessage(this)", 'get() = "msg=${ `msg` }"'),
            ("swift", SWIFT, "actrErrorMessage(err: self)", "String(reflecting: self)"),
        ):
            with self.subTest(language=language):
                source = path.read_text()
                self.assertIn(patched, source)
                self.assertEqual(MODULE.patch(source.replace(patched, generated), language), source)

    def test_missing_export_and_template_drift_fail_closed(self):
        for language, path in (("kotlin", KOTLIN), ("swift", SWIFT)):
            source = path.read_text()
            with self.subTest(language=language), self.assertRaises(ValueError):
                MODULE.patch(source.replace("actrErrorMessage", "missingMessage"), language)
        source = KOTLIN.read_text()
        with self.assertRaises(ValueError):
            MODULE.patch(source.replace("class TimedOut(", "class FutureVariant("), "kotlin")
        with self.assertRaises(ValueError):
            MODULE.patch(source.replace("get() = actrErrorMessage(this)", "get() = null", 1), "kotlin")

    def test_regeneration_preserves_delivery_uncertain_field_name(self):
        source = KOTLIN.read_text()
        generated = source.replace("val causeDetail:", "val `cause`:").replace(
            "value.causeDetail", "value.`cause`"
        )
        self.assertEqual(MODULE.patch(generated, "kotlin"), source)

    def test_swift_variant_drift_fails_closed(self):
        source = SWIFT.read_text()
        for changed in (
            source.replace("case TimedOut", "case FutureVariant", 1),
            source.replace("case TimedOut", "case TimedOut\n    case FutureVariant", 1),
        ):
            with self.assertRaises(ValueError):
                MODULE.patch(changed, "swift")

    def test_getter_expression_suffix_fails_closed(self):
        for language, path, getter in (
            ("kotlin", KOTLIN, "get() = actrErrorMessage(this)"),
            ("swift", SWIFT, "actrErrorMessage(err: self)"),
        ):
            with self.subTest(language=language), self.assertRaises(ValueError):
                MODULE.patch(path.read_text().replace(getter, getter + '.trim()', 1), language)

    def test_kotlin_compound_getter_fails_closed(self):
        source = KOTLIN.read_text()
        for expression in ('"msg".trim() + "suffix"', '"prefix" + "suffix"'):
            with self.subTest(expression=expression), self.assertRaises(ValueError):
                MODULE.patch(source.replace(
                    "get() = actrErrorMessage(this)", f"get() = {expression}", 1
                ), "kotlin")

    def test_swift_prefixed_getter_fails_closed(self):
        source = SWIFT.read_text()
        for expression in ("String(reflecting: self)", "actrErrorMessage(err: self)"):
            with self.subTest(expression=expression), self.assertRaises(ValueError):
                MODULE.patch(source.replace(
                    "actrErrorMessage(err: self)", f'"prefix" + {expression}', 1
                ), "swift")

    def test_multiline_getter_continuation_fails_closed(self):
        for language, path, getter in (
            ("kotlin", KOTLIN, "get() = actrErrorMessage(this)"),
            ("swift", SWIFT, "actrErrorMessage(err: self)"),
        ):
            with self.subTest(language=language), self.assertRaises(ValueError):
                MODULE.patch(path.read_text().replace(
                    getter, getter + '\n                + "suffix"', 1
                ), language)

    def test_commented_getter_fails_closed(self):
        for language, path, getter in (
            ("kotlin", KOTLIN, "get() = actrErrorMessage(this)"),
            ("swift", SWIFT, "actrErrorMessage(err: self)"),
        ):
            with self.subTest(language=language), self.assertRaises(ValueError):
                MODULE.patch(path.read_text().replace(getter, "// " + getter, 1), language)


if __name__ == "__main__":
    unittest.main()
