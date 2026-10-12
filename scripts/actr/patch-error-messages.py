#!/usr/bin/env python3
"""Route UniFFI SDK error descriptions through Rust's Display implementation.

Run immediately after binding generation. Fail on template drift rather than
silently publishing SDKs with field-name messages. No build tools are invoked.
"""

import argparse
from pathlib import Path
import re


VARIANTS = {
    "Unavailable", "ConnectionNotReady", "TimedOut", "NotFound",
    "PermissionDenied", "InvalidArgument", "UnknownRoute", "DependencyNotFound",
    "DecodeFailure", "NotImplemented", "Internal", "Config", "DeliveryUncertain",
}

KOTLIN_MESSAGE_HELPER = '''
    // Keep exception inspection safe before native loading or after linkage failure.
    internal fun displayMessage(nativeMessage: (ActrException) -> String = ::actrErrorMessage): String {
        try {
            return nativeMessage(this)
        } catch (_: LinkageError) {
            // Includes UnsatisfiedLinkError, ExceptionInInitializerError and
            // NoClassDefFoundError on subsequent access to a failed initializer.
            return when (this) {
                is Unavailable -> "unavailable: $msg"
                is ConnectionNotReady -> "connection not ready: " +
                    (info.retryAfterMs?.let { "retry after ${it}ms" } ?: "no retry hint")
                is TimedOut -> "timed out"
                is NotFound -> "not found: $msg"
                is PermissionDenied -> "permission denied: $msg"
                is InvalidArgument -> "invalid argument: $msg"
                is UnknownRoute -> "unknown route: $msg"
                is DependencyNotFound -> "dependency '$serviceName' not found: $detail"
                is DecodeFailure -> "decode failure: $msg"
                is NotImplemented -> "not implemented: $msg"
                is Internal -> "internal error: $msg"
                is Config -> "config: $msg"
                is DeliveryUncertain -> "delivery uncertain for operation $operationId: $causeDetail"
            }
        }
    }
'''


def patch(source: str, language: str) -> str:
    if not re.search(r"\bfun[c]?\s+`?actrErrorMessage`?\s*\(", source):
        raise ValueError("actrErrorMessage export missing; regenerate from the updated libactr")
    if language == "kotlin":
        pattern = r"sealed class ActrException\b.*?public object FfiConverterTypeActrError\b.*?^}"
        getter = (
            r'(^[ \t]*override val message[ \t]*\n[ \t]*get\(\) = )'
            r'(?:"(?:[^"\\\n]|\\[^\n])*"|actrErrorMessage\(this\)|displayMessage\(\))'
            r'(?=[ \t]*\n[ \t]*}[ \t]*$)'
        )
        replacement = r"\g<1>displayMessage()"
    else:
        pattern = r"(public enum ActrError:.*?)(?=#if compiler\(>=6\))"
        getter = (
            r"(^[ \t]*public var errorDescription: String\? \{[ \t]*\n[ \t]*)"
            r"(?:String\(reflecting: self\)|actrErrorMessage\(err: self\))"
            r"(?=[ \t]*\n[ \t]*}[ \t]*$)"
        )
        replacement = r"\g<1>actrErrorMessage(err: self)"
    blocks = list(re.finditer(pattern, source, re.DOTALL | re.MULTILINE))
    if len(blocks) != 1:
        raise ValueError(f"expected one {language} ActrError block")
    block = blocks[0]
    body = block.group()
    if language == "kotlin":
        variants = re.findall(r"\bclass (\w+)\s*\(", body)
    else:
        variants = re.findall(r"^\s*case (\w+)\b", body, re.MULTILINE)
    if len(variants) != len(VARIANTS) or set(variants) != VARIANTS:
        raise ValueError(f"{language} error variants changed; update message coverage")
    patched, count = re.subn(getter, replacement, body, flags=re.MULTILINE)
    expected = len(VARIANTS) if language == "kotlin" else 1
    if count != expected:
        raise ValueError(f"expected {expected} {language} message getters, found {count}")
    if language == "kotlin":
        if "internal fun displayMessage" in patched:
            if KOTLIN_MESSAGE_HELPER not in patched:
                raise ValueError("Kotlin message fallback changed; update message coverage")
        else:
            header = "sealed class ActrException: kotlin.Exception() {"
            if patched.count(header) != 1:
                raise ValueError("Kotlin ActrException header template changed")
            patched = patched.replace(header, header + KOTLIN_MESSAGE_HELPER, 1)
        # Retain the existing Kotlin field name: Throwable.cause is already a
        # Throwable?, so UniFFI's String-valued `cause` cannot compile as-is.
        # Include its converter accesses; the Rust field and wire layout stay intact.
        patched = patched.replace("val `cause`: kotlin.String", "val causeDetail: kotlin.String")
        patched = patched.replace("value.`cause`", "value.causeDetail")
        if patched.count("val causeDetail: kotlin.String") != 1 or patched.count("value.causeDetail") != 2:
            raise ValueError("DeliveryUncertain cause field/converter template changed")
    return source[:block.start()] + patched + source[block.end():]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("language", choices=("kotlin", "swift"))
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    source = args.source.read_text()
    args.source.write_text(patch(source, args.language))


if __name__ == "__main__":
    main()
