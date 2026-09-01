//! Compile-time proof that the generated seal is effective.
//!
//! The generator unit tests pin the emitted tokens (`pub(super) mod
//! private`); these tests pin the *language-level property* that the
//! emitted visibility implies, using the same module shape the generator
//! produces under `src/generated/`:
//!
//! - the sibling `actr_handler` module CAN implement the seal and the
//!   dispatch trait (the generated adapter keeps working),
//! - business code OUTSIDE `generated/` CANNOT name `private::Sealed`, so
//!   a rogue type that implements both `Sealed` and `ActrDispatchHandler`
//!   fails to compile.

use std::process::Command;

/// Mirrors the generated `generated/mod.rs` + `actr_dispatch.rs` seal
/// shape, with framework types replaced by plain bounds.
const GENERATED_SHAPE: &str = r#"
pub mod generated {
    pub mod actr_dispatch {
        pub(super) mod private {
            pub trait Sealed {}
        }

        #[allow(private_bounds)]
        pub trait ActrDispatchHandler: private::Sealed + Send + Sync + 'static {}

        impl private::Sealed for () {}
        impl ActrDispatchHandler for () {}
    }
}
"#;

fn compile_snippet(test_name: &str, source: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("actr_seal_test_{test_name}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let file = dir.join("lib.rs");
    std::fs::write(&file, source).expect("write snippet");
    Command::new("rustc")
        .arg("--edition")
        .arg("2024")
        .arg("--crate-type")
        .arg("lib")
        .arg("--out-dir")
        .arg(&dir)
        .arg(&file)
        .output()
        .expect("run rustc")
}

#[test]
fn sibling_generated_adapter_can_implement_sealed_dispatch_handler() {
    // The adapter module is nested INSIDE `generated/`, exactly like the
    // real `generated/actr_handler.rs`: as a descendant of `generated` it
    // may name the `pub(super)` seal and implement the dispatch trait.
    let source = GENERATED_SHAPE.replace(
        "pub mod generated {",
        "pub mod generated {\n    pub mod actr_handler {\n        pub struct ActrHandler;\n        impl super::actr_dispatch::private::Sealed for ActrHandler {}\n        impl super::actr_dispatch::ActrDispatchHandler for ActrHandler {}\n    }",
    );
    let output = compile_snippet("adapter_ok", &source);
    assert!(
        output.status.success(),
        "generated adapter must compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn user_code_cannot_implement_both_sealed_and_dispatch_handler() {
    let source = format!(
        "
{GENERATED_SHAPE}

// Rogue business code outside `generated/` trying to bypass the adapter.
pub struct Rogue;
impl crate::generated::actr_dispatch::private::Sealed for Rogue {{}}
impl crate::generated::actr_dispatch::ActrDispatchHandler for Rogue {{}}
"
    );
    let output = compile_snippet("rogue_fails", &source);
    assert!(
        !output.status.success(),
        "rogue impl of a sealed trait must fail to compile"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E0603") || stderr.contains("module `private` is private"),
        "expected a privacy error for `private`:\n{stderr}"
    );
}
