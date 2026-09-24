use super::*;
use actr_config::ConfigParser;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

#[test]
fn skips_validation_when_requested() {
    assert!(!should_validate(true));
}

#[test]
fn runs_validation_by_default() {
    assert!(should_validate(false));
}

#[test]
fn generator_factory_returns_all_languages() {
    for language in [
        SupportedLanguage::Rust,
        SupportedLanguage::Python,
        SupportedLanguage::Swift,
        SupportedLanguage::Kotlin,
        SupportedLanguage::TypeScript,
    ] {
        let _ = GeneratorFactory::get_generator(language);
    }
}

/// Records how many times each pipeline step is invoked; every step succeeds.
#[derive(Default)]
struct SpyGenerator {
    infrastructure_calls: AtomicUsize,
    scaffold_calls: AtomicUsize,
    format_calls: AtomicUsize,
    validate_calls: AtomicUsize,
    finalize_calls: AtomicUsize,
    emit_metadata: bool,
}

#[async_trait]
impl LanguageGenerator for SpyGenerator {
    async fn generate_infrastructure(&self, context: &GenContext) -> Result<Vec<PathBuf>> {
        self.infrastructure_calls.fetch_add(1, Ordering::SeqCst);
        if self.emit_metadata {
            write_metadata(
                &context.output,
                &ActrGenMetadata {
                    plugin_version: "test-plugin".into(),
                    language: "rust".into(),
                    local_services: vec![],
                    remote_services: vec![],
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![])
    }

    async fn generate_scaffold(
        &self,
        _context: &GenContext,
        _catalog: &ScaffoldCatalog,
    ) -> Result<Vec<PathBuf>> {
        self.scaffold_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![])
    }

    async fn format_code(&self, _context: &GenContext, _files: &[PathBuf]) -> Result<()> {
        self.format_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn validate_code(&self, _context: &GenContext) -> Result<()> {
        self.validate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn finalize_generation(&self, _context: &GenContext) -> Result<()> {
        self.finalize_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn print_next_steps(&self, _context: &GenContext) {}
}

fn test_context(
    tmp: &TempDir,
    no_scaffold: bool,
    no_format: bool,
    skip_validation: bool,
) -> GenContext {
    let config_path = tmp.path().join("manifest.toml");
    std::fs::write(
        &config_path,
        r#"edition = 1
exports = []

[package]
name = "Demo"
publisher = "acme"
version = "0.1.0"

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.issuer_endpoint]
url = "http://127.0.0.1:8080/issuer"

[system.deployment]
realm = "dev@example.com"
"#,
    )
    .unwrap();
    let config = ConfigParser::from_manifest_file(&config_path).unwrap();

    GenContext {
        proto_files: vec![],
        proto_model: ProtoModel {
            files: vec![],
            local_services: vec![],
            remote_services: vec![],
        },
        input_path: tmp.path().join("protos"),
        output: tmp.path().join("generated"),
        config_path,
        config,
        no_scaffold,
        overwrite_user_code: false,
        no_format,
        debug: false,
        skip_validation,
        dispatch_spec: DispatchSpec::default(),
        dispatch_spec_path: PathBuf::new(),
        dispatch_spec_hash: String::new(),
    }
}

fn run_pipeline_with_spy(context: &GenContext) -> SpyGenerator {
    let spy = SpyGenerator {
        emit_metadata: true,
        ..SpyGenerator::default()
    };
    tokio_test::block_on(run_codegen_pipeline(SupportedLanguage::Rust, &spy, context)).unwrap();
    spy
}

#[test]
fn pipeline_requires_plugin_metadata_after_infrastructure() {
    let tmp = TempDir::new().unwrap();
    let context = test_context(&tmp, true, true, true);
    let spy = SpyGenerator::default();

    let err = tokio_test::block_on(run_codegen_pipeline(
        SupportedLanguage::Rust,
        &spy,
        &context,
    ))
    .expect_err("pipeline must fail when the plugin did not write metadata");

    assert!(
        err.to_string().contains("actr-gen-meta.json"),
        "expected missing metadata error, got: {err}"
    );
    assert_eq!(spy.infrastructure_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.scaffold_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.format_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.validate_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.finalize_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pipeline_skips_validation_when_flag_is_set() {
    let tmp = TempDir::new().unwrap();
    let context = test_context(&tmp, false, false, true);

    let spy = run_pipeline_with_spy(&context);

    assert_eq!(spy.validate_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.infrastructure_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.finalize_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn pipeline_validates_by_default() {
    let tmp = TempDir::new().unwrap();
    let context = test_context(&tmp, false, false, false);

    let spy = run_pipeline_with_spy(&context);

    assert_eq!(spy.validate_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.scaffold_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.format_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn pipeline_skips_scaffold_when_flag_is_set() {
    let tmp = TempDir::new().unwrap();
    let context = test_context(&tmp, true, false, false);

    let spy = run_pipeline_with_spy(&context);

    assert_eq!(spy.scaffold_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.format_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.validate_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn pipeline_skips_format_when_flag_is_set() {
    let tmp = TempDir::new().unwrap();
    let context = test_context(&tmp, false, true, false);

    let spy = run_pipeline_with_spy(&context);

    assert_eq!(spy.format_calls.load(Ordering::SeqCst), 0);
    assert_eq!(spy.scaffold_calls.load(Ordering::SeqCst), 1);
    assert_eq!(spy.validate_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn parses_bare_pre_release_plugin_version() {
    // Regression for #644: the Swift plugin prints a bare pre-release token
    // that the previous digits-and-dots-only parser rejected.
    assert_eq!(
        parse_plugin_version("0.10.0-alpha.3\n"),
        Some("0.10.0-alpha.3".to_string())
    );
}

#[test]
fn parses_plugin_version_with_pre_release_and_build_metadata() {
    assert_eq!(
        parse_plugin_version("1.2.3-rc.1+build"),
        Some("1.2.3-rc.1+build".to_string())
    );
}

#[test]
fn parses_plugin_version_prefixed_by_plugin_name() {
    assert_eq!(
        parse_plugin_version("protoc-gen-actrframework-swift 0.1.10\n"),
        Some("0.1.10".to_string())
    );
}

#[test]
fn parses_plugin_version_from_multiline_output() {
    assert_eq!(
        parse_plugin_version("protoc-gen-x\nbanner text\n0.9.7\n"),
        Some("0.9.7".to_string())
    );
}

#[test]
fn returns_none_when_plugin_output_has_no_version_token() {
    assert_eq!(
        parse_plugin_version("protoc-gen-actrframework-swift\n"),
        None
    );
    assert_eq!(parse_plugin_version(""), None);
    // A leading `v` does not count as a version; shipped plugins print bare
    // versions.
    assert_eq!(parse_plugin_version("v1.2.3"), None);
}
