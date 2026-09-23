mod dispatch_spec;
mod kotlin;
mod metadata;
mod proto_model;
mod python;
mod rust;
mod scaffold;
mod swift;
mod traits;
mod typescript;

use std::path::{Path, PathBuf};

pub use crate::commands::SupportedLanguage;
use crate::error::Result;
pub use dispatch_spec::{
    DISPATCH_SPEC_FILE, DISPATCH_SPEC_SCHEMA_VERSION, DispatchRoute, DispatchSpec, ReplyMode,
    reject_bare_method_name_collisions,
};
use kotlin::KotlinGenerator;
pub use metadata::{
    ACTR_GEN_META_FILE, ActrGenMetadata, TypeRef, load_metadata, load_required_metadata,
    metadata_path, write_metadata,
};
pub use proto_model::{
    MethodModel, ProtoFileModel, ProtoModel, ProtoSide, ServiceModel, TypeOwner, TypeOwnerIndex,
};
use python::PythonGenerator;
use rust::RustGenerator;
pub use scaffold::{ScaffoldCatalog, ScaffoldMethod, ScaffoldService};
use swift::SwiftGenerator;
use tracing::info;
pub use traits::{GenContext, LanguageGenerator, ScaffoldType};
use typescript::TypeScriptGenerator;

pub struct GeneratorFactory;

impl GeneratorFactory {
    pub fn get_generator(language: SupportedLanguage) -> Box<dyn LanguageGenerator> {
        match language {
            SupportedLanguage::Rust => Box::new(RustGenerator),
            SupportedLanguage::Python => Box::new(PythonGenerator),
            SupportedLanguage::Swift => Box::new(SwiftGenerator),
            SupportedLanguage::Kotlin => Box::new(KotlinGenerator),
            SupportedLanguage::TypeScript => Box::new(TypeScriptGenerator),
        }
    }
}

fn should_validate(skip_validation: bool) -> bool {
    !skip_validation
}

/// Extracts the version token from a protoc plugin `--version` output.
///
/// Plugins print either a bare version (`0.10.0-alpha.3`) or a
/// `name version` pair (`protoc-gen-actrframework-swift 0.1.10`). Returns the
/// first whitespace token starting with an ASCII digit so pre-release and
/// build-metadata suffixes (`-alpha.3`, `+build`) parse correctly (#644).
pub(crate) fn parse_plugin_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

/// Locates a workspace-local protoc plugin directory (e.g.
/// `tools/actr/protoc-gen/kotlin`) for the developer shortcut.
///
/// Debug builds only: published binaries skip the shortcut because the
/// compile-time checkout never exists on user machines, and discovery walks
/// up from the current directory instead. Debug binaries compiled inside a
/// checkout keep working when invoked from unrelated project directories via
/// the `CARGO_MANIFEST_DIR` fallback.
pub(crate) fn find_workspace_plugin_dir(tool_dir: &str) -> Result<Option<PathBuf>> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }

    if let Some(plugin_dir) = plugin_dir_from(&std::env::current_dir()?, tool_dir) {
        return Ok(Some(plugin_dir));
    }

    Ok(plugin_dir_from(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        tool_dir,
    ))
}

fn plugin_dir_from(start: &Path, tool_dir: &str) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|path| path.join("Cargo.toml").is_file() && path.join(tool_dir).is_dir())
        .map(|root| root.join(tool_dir))
}

pub async fn execute_codegen(language: SupportedLanguage, context: &GenContext) -> Result<()> {
    let generator = GeneratorFactory::get_generator(language);
    run_codegen_pipeline(language, generator.as_ref(), context).await
}

/// Runs the language-independent generation pipeline. All `GenContext` flag
/// gates (`no_scaffold`, `no_format`, `skip_validation`) are enforced here so
/// individual generators never need to re-check them.
async fn run_codegen_pipeline(
    language: SupportedLanguage,
    generator: &dyn LanguageGenerator,
    context: &GenContext,
) -> Result<()> {
    let mut all_files = generator.generate_infrastructure(context).await?;
    let metadata = load_required_metadata(&context.output, language)?;
    metadata::validate_metadata_for_context(&metadata, context)?;
    if !context.no_scaffold {
        let catalog = ScaffoldCatalog::from_metadata(&metadata);
        all_files.extend(generator.generate_scaffold(context, &catalog).await?);
    }
    if !context.no_format {
        generator.format_code(context, &all_files).await?;
    }

    if should_validate(context.skip_validation) {
        generator.validate_code(context).await?;
    } else {
        info!("⏭️  Skipped code validation (--skip-validation)");
    }

    info!("Code generation completed");

    generator.finalize_generation(context).await?;

    generator.print_next_steps(context);
    Ok(())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
