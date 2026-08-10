use crate::commands::codegen::scaffold::ScaffoldCatalog;
use crate::commands::codegen::traits::{GenContext, LanguageGenerator};
use crate::error::{ActrCliError, Result};
use crate::utils::to_snake_case;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tracing::{debug, info, warn};

pub struct RustGenerator;

const HANDLER_SCAFFOLD_MARKER: &str = "// ACTR: generated Rust handler scaffold";
const ENTRY_PLACEHOLDER_MARKER: &str = "// ACTR: generated Rust workload entry placeholder";
const ENTRY_SCAFFOLD_MARKER: &str = "// ACTR: generated Rust workload entry scaffold";
const UNIFIED_HANDLER_MODULE: &str = "actr_handler";
const UNIFIED_HANDLER_TYPE: &str = "ActrHandler";

#[async_trait]
impl LanguageGenerator for RustGenerator {
    async fn generate_infrastructure(&self, context: &GenContext) -> Result<Vec<PathBuf>> {
        info!("🔧 Generating infrastructure code...");

        let prost_plugin_path = self.ensure_prost_plugin(&context.config_path)?;
        let plugin_path = self.ensure_protoc_plugin()?;

        let manufacturer = context.config.package.actr_type.manufacturer.clone();
        debug!("Using manufacturer from manifest.toml: {}", manufacturer);

        let output = &context.output;

        // `GenCommand::preprocess` handles `--clean` before writing the
        // DispatchSpec under `<output>/.actr/codegen`. Keep that control file
        // intact while making previously generated, read-only Rust files
        // writable for protoc to replace.
        if output.exists() {
            make_writable_recursive(output)?;
        }
        std::fs::create_dir_all(output).map_err(|e| {
            ActrCliError::command_error(format!("Failed to create output directory: {e}"))
        })?;

        self.run_protoc_passes(
            context,
            output,
            &prost_plugin_path,
            &plugin_path,
            &manufacturer,
        )?;

        self.generate_mod_rs(output).await?;

        info!("✅ Infrastructure code generation completed");
        // Return empty Vec — Rust's format/validate scan the directory themselves
        Ok(vec![])
    }

    async fn generate_scaffold(
        &self,
        context: &GenContext,
        catalog: &ScaffoldCatalog,
    ) -> Result<Vec<PathBuf>> {
        info!("📝 Generating user code scaffold...");

        if !catalog.local_services.is_empty() {
            self.generate_unified_handler_scaffold(
                catalog,
                &context.output,
                context.overwrite_user_code,
            )
            .await?;
        }

        if !catalog.local_services.is_empty() || !catalog.remote_services.is_empty() {
            self.generate_entry_scaffold(catalog, &context.output, context.overwrite_user_code)
                .await?;
        }

        info!("✅ User code scaffold generation completed");
        Ok(vec![])
    }

    async fn format_code(&self, context: &GenContext, _files: &[PathBuf]) -> Result<()> {
        info!("🎨 Formatting generated code...");

        let mut cmd = StdCommand::new("rustfmt");
        cmd.arg("--edition")
            .arg("2024")
            .arg("--config")
            .arg("max_width=100");

        for entry in std::fs::read_dir(&context.output).map_err(|e| {
            ActrCliError::config_error(format!("Failed to read output directory: {e}"))
        })? {
            let entry = entry.map_err(|e| ActrCliError::config_error(e.to_string()))?;
            let path = entry.path();
            if path.extension().unwrap_or_default() == "rs" {
                cmd.arg(&path);
            }
        }

        let output = cmd
            .output()
            .map_err(|e| ActrCliError::command_error(format!("Failed to execute rustfmt: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("rustfmt execution warning: {}", stderr);
        } else {
            info!("✅ Code formatting completed");
        }

        Ok(())
    }

    async fn validate_code(&self, _context: &GenContext) -> Result<()> {
        info!("🔍 Validating generated code...");

        let project_root = self.find_project_root()?;

        let mut cmd = StdCommand::new("cargo");
        cmd.arg("check").arg("--quiet").current_dir(&project_root);

        let output = cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to execute cargo check: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "Generated code has compilation warnings or errors:\n{}",
                stderr
            );
            info!("💡 This is usually normal because the user code scaffold contains TODO markers");
        } else {
            info!("✅ Code validation passed");
        }

        Ok(())
    }

    async fn finalize_generation(&self, context: &GenContext) -> Result<()> {
        self.set_generated_files_readonly(&context.output)
    }

    fn print_next_steps(&self, context: &GenContext) {
        let has_local_services = context
            .proto_model
            .local_services
            .iter()
            .any(|service| !service.methods.is_empty());

        println!("\n🎉 Code generation completed!");
        println!("\n📋 Next steps:");
        println!("1. 📖 View generated code: {:?}", context.output);
        if has_local_services {
            if !context.no_scaffold {
                println!(
                    "2. ✏️  Implement business logic: in the *_service.rs files in the src/ directory"
                );
                println!("3. 📦 Build package: actr build");
                println!("4. 🚀 Host the resulting .actr package from your runtime config");
            } else {
                println!("2. 📦 Build package: actr build");
                println!("3. 🚀 Host the resulting .actr package from your runtime config");
            }
        } else if !context.no_scaffold {
            println!(
                "2. ✏️  Implement business logic: in the *_service.rs files in the src/ directory"
            );
            println!("3. 🔧 Add dependencies: add required packages in Cargo.toml");
            println!("4. 🏗️  Build project: cargo build");
            println!("5. 🧪 Run tests: cargo test");
            println!("6. 🚀 Start service: cargo run");
        } else {
            println!("2. 🏗️  Build project: cargo build");
            println!("3. 🧪 Run tests: cargo test");
            println!("4. 🚀 Start service: cargo run");
        }
        println!("\n💡 Tip: Check the detailed user guide in the generated user code files");
    }
}

impl RustGenerator {
    fn run_protoc_passes(
        &self,
        context: &GenContext,
        output: &Path,
        prost_plugin_path: &Path,
        plugin_path: &Path,
        manufacturer: &str,
    ) -> Result<()> {
        // The Rust plugin now reads the DispatchSpec for local/remote
        // classification and dependency aliases — the legacy
        // `LocalFiles`/`RemoteFiles`/`RemoteFileActrTypes` params are no
        // longer passed via --actrframework_opt. The `manufacturer` param
        // is retained for backward compat with older plugin versions.
        let mut opt_str = format!("manufacturer={}", manufacturer);

        // Pass dispatch spec path and hash to the protoc plugin.
        opt_str.push_str(&format!(
            ",dispatch_spec={}",
            context.dispatch_spec_path.display()
        ));
        opt_str.push_str(&format!(
            ",dispatch_spec_hash={}",
            context.dispatch_spec_hash
        ));

        for proto_file in &context.proto_files {
            debug!("Processing proto file: {:?}", proto_file);

            // Step 1: prost for protobuf message types
            let mut cmd = StdCommand::new("protoc");
            cmd.arg(format!("--proto_path={}", context.input_path.display()))
                .arg(format!(
                    "--plugin=protoc-gen-prost={}",
                    prost_plugin_path.display()
                ))
                .arg("--prost_opt=flat_output_dir")
                .arg(format!("--prost_out={}", output.display()))
                .arg(proto_file);

            debug!("Executing protoc (prost): {:?}", cmd);
            let output_cmd = cmd.output().map_err(|e| {
                ActrCliError::command_error(format!("Failed to execute protoc (prost): {e}"))
            })?;

            if !output_cmd.status.success() {
                let stderr = String::from_utf8_lossy(&output_cmd.stderr);
                return Err(ActrCliError::command_error(format!(
                    "protoc (prost) execution failed: {stderr}"
                )));
            }
        }

        // Step 2: actrframework plugin for Actor framework code.
        // Run once across the full proto set so local generators can see remote services.
        let mut cmd = StdCommand::new("protoc");
        cmd.arg(format!("--proto_path={}", context.input_path.display()))
            .arg(format!(
                "--plugin=protoc-gen-actrframework={}",
                plugin_path.display()
            ))
            .arg(format!("--actrframework_opt={}", opt_str))
            .arg(format!("--actrframework_out={}", output.display()));

        for proto_file in &context.proto_files {
            cmd.arg(proto_file);
        }

        debug!("Executing protoc (actrframework): {:?}", cmd);
        let output_cmd = cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to execute protoc (actrframework): {e}"))
        })?;

        if !output_cmd.status.success() {
            let stderr = String::from_utf8_lossy(&output_cmd.stderr);
            return Err(ActrCliError::command_error(format!(
                "protoc (actrframework) execution failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output_cmd.stdout);
        if !stdout.is_empty() {
            debug!("protoc output: {}", stdout);
        }

        Ok(())
    }

    fn set_generated_files_readonly(&self, output: &Path) -> Result<()> {
        use std::fs;

        for entry in fs::read_dir(output).map_err(|e| {
            ActrCliError::config_error(format!("Failed to read output directory: {e}"))
        })? {
            let entry = entry.map_err(|e| ActrCliError::config_error(e.to_string()))?;
            let path = entry.path();

            if path.is_file() && path.extension().unwrap_or_default() == "rs" {
                let metadata = fs::metadata(&path).map_err(|e| {
                    ActrCliError::config_error(format!("Failed to get file metadata: {e}"))
                })?;
                let mut permissions = metadata.permissions();

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = permissions.mode();
                    permissions.set_mode(mode & !0o222);
                }

                #[cfg(not(unix))]
                {
                    permissions.set_readonly(true);
                }

                fs::set_permissions(&path, permissions).map_err(|e| {
                    ActrCliError::config_error(format!("Failed to set file permissions: {e}"))
                })?;

                debug!("Set read-only attribute: {:?}", path);
            }
        }

        Ok(())
    }

    async fn generate_mod_rs(&self, output: &Path) -> Result<()> {
        let mod_path = output.join("mod.rs");

        let mut proto_modules = Vec::new();
        let mut service_modules = Vec::new();

        use std::fs;
        for entry in fs::read_dir(output).map_err(|e| {
            ActrCliError::config_error(format!("Failed to read output directory: {e}"))
        })? {
            let entry = entry.map_err(|e| ActrCliError::config_error(e.to_string()))?;
            let path = entry.path();

            if path.is_file()
                && path.extension().unwrap_or_default() == "rs"
                && let Some(file_name) = path.file_stem().and_then(|s| s.to_str())
            {
                if file_name == "mod" {
                    continue;
                }

                if file_name.ends_with("_actor") || file_name.ends_with("_client") {
                    service_modules.push(format!("pub mod {file_name};"));
                } else {
                    proto_modules.push(format!("pub mod {file_name};"));
                }
            }
        }

        proto_modules.sort();
        service_modules.sort();

        let mod_content = format!(
            "//! Automatically generated code module\n\
             //!\n\
             //! This module is automatically generated by the `actr gen` command, including:\n\
             //! - protobuf message type definitions\n\
             //! - Actor framework code (router, traits)\n\
             //!\n\
             //! ⚠️ Do not manually modify files in this directory\n\
             \n\
             // Protobuf message types (generated by prost)\n\
             {}\n\
             \n\
             // Actor framework code (generated by protoc-gen-actrframework)\n\
             {}\n\
             \n\
             // Common types are defined in their respective modules, please import as needed\n",
            proto_modules.join("\n"),
            service_modules.join("\n"),
        );

        std::fs::write(&mod_path, mod_content)
            .map_err(|e| ActrCliError::config_error(format!("Failed to write mod.rs: {e}")))?;

        debug!("Generated mod.rs: {:?}", mod_path);
        Ok(())
    }

    async fn generate_unified_handler_scaffold(
        &self,
        catalog: &ScaffoldCatalog,
        output: &Path,
        overwrite_user_code: bool,
    ) -> Result<()> {
        let src_dir = src_dir_from_output(output);
        std::fs::create_dir_all(&src_dir).map_err(|e| {
            ActrCliError::config_error(format!("Failed to create src directory: {e}"))
        })?;
        let path = src_dir.join(format!("{UNIFIED_HANDLER_MODULE}.rs"));
        let scaffold = self.generate_unified_handler_scaffold_content(catalog);
        if path.exists() && !overwrite_user_code {
            let current = std::fs::read_to_string(&path).map_err(|e| {
                ActrCliError::config_error(format!("Failed to read {}: {e}", path.display()))
            })?;
            if current != scaffold {
                info!("⏭️  Skipping existing user code file: {:?}", path);
                return Ok(());
            }
        }
        std::fs::write(&path, scaffold).map_err(|e| {
            ActrCliError::config_error(format!("Failed to write Rust handler scaffold: {e}"))
        })?;
        info!("📄 Generated unified Rust handler scaffold: {:?}", path);
        Ok(())
    }

    async fn generate_entry_scaffold(
        &self,
        catalog: &ScaffoldCatalog,
        output: &Path,
        overwrite_user_code: bool,
    ) -> Result<()> {
        let src_dir = src_dir_from_output(output);
        std::fs::create_dir_all(&src_dir).map_err(|e| {
            ActrCliError::config_error(format!("Failed to create src directory: {e}"))
        })?;
        let lib_path = src_dir.join("lib.rs");
        let scaffold_content = self.generate_entry_scaffold_content(catalog);

        if lib_path.exists() && !overwrite_user_code {
            if self.should_overwrite_entry_scaffold(&lib_path, &scaffold_content)? {
                info!(
                    "🔄 Overwriting generated Rust entry scaffold: {:?}",
                    lib_path
                );
            } else {
                info!("⏭️  Skipping existing Rust entry file: {:?}", lib_path);
                return Ok(());
            }
        }

        if lib_path.exists() {
            make_writable_recursive(&lib_path)?;
        }

        std::fs::write(&lib_path, scaffold_content).map_err(|e| {
            ActrCliError::config_error(format!("Failed to write Rust entry scaffold: {e}"))
        })?;

        info!("📄 Generated Rust entry scaffold: {:?}", lib_path);
        Ok(())
    }

    fn should_overwrite_entry_scaffold(
        &self,
        path: &Path,
        expected_scaffold: &str,
    ) -> Result<bool> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ActrCliError::config_error(format!("Failed to read {}: {e}", path.display()))
        })?;

        if content == expected_scaffold {
            return Ok(true);
        }

        Ok(is_generated_entry_placeholder(&content) || is_default_cargo_lib_rs(&content))
    }

    fn generate_entry_scaffold_content(&self, catalog: &ScaffoldCatalog) -> String {
        let (handler_module, handler_import, handler_type, handler_value) =
            if catalog.local_services.is_empty() {
                (String::new(), String::new(), "()", "()")
            } else {
                (
                    format!("pub mod {UNIFIED_HANDLER_MODULE};\n"),
                    format!("pub use crate::{UNIFIED_HANDLER_MODULE}::{UNIFIED_HANDLER_TYPE};\n"),
                    UNIFIED_HANDLER_TYPE,
                    "ActrHandler::new()",
                )
            };

        format!(
            r#"{ENTRY_SCAFFOLD_MARKER}
//! Entry point for the generated unified ACTR workload.

pub mod generated;
{handler_module}
use actr_framework::entry;
use generated::actr_dispatch::ActrWorkload;
{handler_import}
entry!(
    ActrWorkload<{handler_type}>,
    ActrWorkload::new({handler_value})
);
"#
        )
    }

    fn generate_unified_handler_scaffold_content(&self, catalog: &ScaffoldCatalog) -> String {
        let methods = catalog
            .local_services
            .iter()
            .flat_map(|service| {
                service.methods.iter().map(move |method| {
                    let method_name =
                        format!("{}_{}", to_snake_case(&service.name), method.snake_name);
                    let input_type =
                        rust_scaffold_qualified_type(&method.input_ref, &method.input_type);
                    let output_type =
                        rust_scaffold_qualified_type(&method.output_ref, &method.output_type);
                    format!(
                        r#"    async fn {method_name}<C: Context>(
        &self,
        _req: {input_type},
        _ctx: &C,
    ) -> ActorResult<{output_type}> {{
        todo!("Implement {service_name}.{rpc_name}")
    }}
"#,
                        service_name = service.name,
                        rpc_name = method.name,
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"{HANDLER_SCAFFOLD_MARKER}
//! Unified business-logic handler for every local DispatchSpec route.

use crate::generated::actr_dispatch::ActrDispatchHandler;
use actr_framework::Context;
use actr_protocol::ActorResult;

pub struct {UNIFIED_HANDLER_TYPE};

impl {UNIFIED_HANDLER_TYPE} {{
    pub fn new() -> Self {{
        Self
    }}
}}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ActrDispatchHandler for {UNIFIED_HANDLER_TYPE} {{
{methods}
}}
"#
        )
    }

    fn find_project_root(&self) -> Result<PathBuf> {
        let mut current = std::env::current_dir().map_err(ActrCliError::Io)?;

        loop {
            if current.join("Cargo.toml").exists() {
                return Ok(current);
            }

            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }

        std::env::current_dir().map_err(ActrCliError::Io)
    }

    fn ensure_protoc_plugin(&self) -> Result<PathBuf> {
        const PLUGIN_NAME: &str = "protoc-gen-actrframework";
        let required_version = env!("CARGO_PKG_VERSION");

        if let Some(plugin_path) = self.try_use_local_workspace_plugin()? {
            return Ok(plugin_path);
        }

        let installed_version = self.check_installed_plugin_version()?;

        match installed_version {
            Some(version) if version == required_version => {
                info!("✅ Using installed protoc-gen-actrframework v{}", version);
                self.locate_installed_plugin(PLUGIN_NAME)
            }
            Some(version) => {
                info!(
                    "🔄 Version mismatch: installed v{}, need v{}",
                    version, required_version
                );
                info!("🔨 Upgrading plugin...");
                let path = self.install_or_upgrade_plugin(required_version)?;
                self.ensure_required_plugin_version(required_version)?;
                Ok(path)
            }
            None => {
                info!("📦 protoc-gen-actrframework not found, installing...");
                let path = self.install_or_upgrade_plugin(required_version)?;
                self.ensure_required_plugin_version(required_version)?;
                Ok(path)
            }
        }
    }

    fn try_use_local_workspace_plugin(&self) -> Result<Option<PathBuf>> {
        if !cfg!(debug_assertions) {
            return Ok(None);
        }

        let Some(workspace_root) = self.find_development_actr_workspace_root()? else {
            return Ok(None);
        };

        let plugin_path = self.local_workspace_plugin_path(&workspace_root);

        info!(
            "🧪 Building local workspace plugin in debug build: {}",
            workspace_root.display()
        );
        let mut build_cmd = StdCommand::new("cargo");
        build_cmd
            .arg("build")
            .arg("--quiet")
            .arg("-p")
            .arg("actr-framework-protoc-codegen")
            .arg("--bin")
            .arg("protoc-gen-actrframework")
            .current_dir(&workspace_root);

        let output = build_cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to build local protoc plugin: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActrCliError::command_error(format!(
                "Failed to build local protoc plugin:\n{stderr}"
            )));
        }

        if plugin_path.exists() {
            info!("✅ Built local workspace plugin: {}", plugin_path.display());
            Ok(Some(plugin_path))
        } else {
            Err(ActrCliError::command_error(format!(
                "Local plugin build succeeded but binary was not found at {}",
                plugin_path.display()
            )))
        }
    }

    fn check_installed_plugin_version(&self) -> Result<Option<String>> {
        let output = StdCommand::new("protoc-gen-actrframework")
            .arg("--version")
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let version_info = String::from_utf8_lossy(&output.stdout);
                let version = version_info
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .map(|v| v.to_string());

                debug!("Detected installed version: {:?}", version);
                Ok(version)
            }
            _ => {
                debug!("Plugin not found in PATH");
                Ok(None)
            }
        }
    }

    fn ensure_prost_plugin(&self, _config_path: &Path) -> Result<PathBuf> {
        const PLUGIN_NAME: &str = "protoc-gen-prost";

        if self.find_plugin_in_path(PLUGIN_NAME)?.is_some() {
            return self.locate_installed_plugin(PLUGIN_NAME);
        }

        info!("📦 protoc-gen-prost not found, installing from crates.io...");
        self.install_prost_plugin_from_registry()
    }

    fn install_or_upgrade_plugin(&self, required_version: &str) -> Result<PathBuf> {
        if self.is_ci_environment() {
            info!("🔧 CI detected, installing protoc-gen-actrframework from GitHub source...");
            return self.install_plugin_from_github_source();
        }

        if let Some(workspace_root) = self.find_actr_workspace_root()? {
            info!("🔍 Found actr workspace at: {}", workspace_root.display());

            match self.install_plugin_from_local_path(&workspace_root) {
                Ok(path) => return Ok(path),
                Err(error) => {
                    warn!(
                        "Local plugin installation failed, falling back to crates.io: {}",
                        error
                    );
                }
            }
        } else {
            info!("🔍 No local actr workspace found, falling back to crates.io install...");
        }

        self.install_plugin_from_registry(required_version)
    }

    fn is_ci_environment(&self) -> bool {
        std::env::var_os("CI").is_some()
    }

    fn find_actr_workspace_root(&self) -> Result<Option<PathBuf>> {
        let current_dir = std::env::current_dir()?;
        let workspace_root = current_dir.ancestors().find(|p| {
            let is_workspace =
                p.join("Cargo.toml").exists() && p.join("tools/protoc-gen/rust").exists();
            if is_workspace {
                debug!("Found workspace root: {:?}", p);
            }
            is_workspace
        });

        Ok(workspace_root.map(Path::to_path_buf))
    }

    fn find_development_actr_workspace_root(&self) -> Result<Option<PathBuf>> {
        if let Some(workspace_root) = self.find_actr_workspace_root()? {
            return Ok(Some(workspace_root));
        }

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        if let Some(parent_workspace) = manifest_dir.parent()
            && parent_workspace.join("Cargo.toml").exists()
            && parent_workspace.join("tools/protoc-gen/rust").exists()
        {
            return Ok(Some(parent_workspace.to_path_buf()));
        }

        let sibling_workspace = manifest_dir.join("../actr");
        if sibling_workspace.join("Cargo.toml").exists()
            && sibling_workspace.join("tools/protoc-gen/rust").exists()
        {
            return Ok(Some(sibling_workspace));
        }

        Ok(None)
    }

    fn local_workspace_plugin_path(&self, workspace_root: &Path) -> PathBuf {
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    workspace_root.join(path)
                }
            })
            .unwrap_or_else(|| workspace_root.join("target"));

        target_dir.join("debug").join(format!(
            "protoc-gen-actrframework{}",
            std::env::consts::EXE_SUFFIX
        ))
    }

    fn install_plugin_from_local_path(&self, workspace_root: &Path) -> Result<PathBuf> {
        info!("Installing protoc-gen-actrframework from local path...");
        let mut install_cmd = StdCommand::new("cargo");
        install_cmd
            .arg("install")
            .arg("--path")
            .arg(workspace_root.join("tools/protoc-gen/rust"))
            .arg("--bin")
            .arg("protoc-gen-actrframework")
            .arg("--force");

        debug!("Running: {:?}", install_cmd);
        let output = install_cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to run local plugin installation: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActrCliError::command_error(format!(
                "Failed to install plugin from local path:\n{stderr}"
            )));
        }

        info!("✅ Plugin installed successfully from local path");
        self.locate_installed_plugin("protoc-gen-actrframework")
    }

    fn install_plugin_from_registry(&self, required_version: &str) -> Result<PathBuf> {
        const PACKAGE_NAME: &str = "actr-framework-protoc-codegen";
        const PLUGIN_NAME: &str = "protoc-gen-actrframework";

        info!(
            "Installing {} v{} from crates.io...",
            PLUGIN_NAME, required_version
        );

        let mut install_cmd = StdCommand::new("cargo");
        install_cmd
            .arg("install")
            .arg(PACKAGE_NAME)
            .arg("--version")
            .arg(required_version)
            .arg("--bin")
            .arg(PLUGIN_NAME)
            .arg("--force");

        debug!("Running: {:?}", install_cmd);
        let output = install_cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to run crates.io plugin installation: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActrCliError::command_error(format!(
                "Failed to install plugin from crates.io:\n{stderr}"
            )));
        }

        info!("✅ Plugin installed successfully from crates.io");
        self.locate_installed_plugin(PLUGIN_NAME)
    }

    fn install_plugin_from_github_source(&self) -> Result<PathBuf> {
        const PACKAGE_NAME: &str = "actr-framework-protoc-codegen";
        const PLUGIN_NAME: &str = "protoc-gen-actrframework";
        const REPOSITORY_URL: &str = "https://github.com/Actrium/actr.git";

        let mut install_cmd = StdCommand::new("cargo");
        install_cmd
            .arg("install")
            .arg("--git")
            .arg(REPOSITORY_URL)
            .arg("--branch")
            .arg("main")
            .arg(PACKAGE_NAME)
            .arg("--bin")
            .arg(PLUGIN_NAME)
            .arg("--force");

        debug!("Running: {:?}", install_cmd);
        let output = install_cmd.output().map_err(|e| {
            ActrCliError::command_error(format!(
                "Failed to run GitHub source plugin installation: {e}"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActrCliError::command_error(format!(
                "Failed to install plugin from GitHub source:\n{stderr}"
            )));
        }

        info!("✅ Plugin installed successfully from GitHub source");
        self.locate_installed_plugin(PLUGIN_NAME)
    }

    fn install_prost_plugin_from_registry(&self) -> Result<PathBuf> {
        const PACKAGE_NAME: &str = "protoc-gen-prost";
        const PLUGIN_NAME: &str = "protoc-gen-prost";

        let mut install_cmd = StdCommand::new("cargo");
        install_cmd.arg("install").arg(PACKAGE_NAME).arg("--locked");

        debug!("Running: {:?}", install_cmd);
        let output = install_cmd.output().map_err(|e| {
            ActrCliError::command_error(format!("Failed to run protoc-gen-prost install: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActrCliError::command_error(format!(
                "Failed to install protoc-gen-prost from crates.io:\n{stderr}"
            )));
        }

        info!("✅ protoc-gen-prost installed successfully from crates.io");
        self.locate_installed_plugin(PLUGIN_NAME)
    }

    fn find_plugin_in_path(&self, plugin_name: &str) -> Result<Option<PathBuf>> {
        let which_output = StdCommand::new("which")
            .arg(plugin_name)
            .output()
            .map_err(|e| {
                ActrCliError::command_error(format!("Failed to locate plugin in PATH: {e}"))
            })?;

        if !which_output.status.success() {
            return Ok(None);
        }

        let path = String::from_utf8_lossy(&which_output.stdout)
            .trim()
            .to_string();
        if path.is_empty() {
            return Ok(None);
        }

        Ok(Some(PathBuf::from(path)))
    }

    fn locate_installed_plugin(&self, plugin_name: &str) -> Result<PathBuf> {
        self.find_plugin_in_path(plugin_name)?.ok_or_else(|| {
            ActrCliError::command_error(format!(
                "Failed to locate installed plugin: {} is not in PATH",
                plugin_name
            ))
        })
    }

    fn ensure_required_plugin_version(&self, required_version: &str) -> Result<()> {
        let installed_version = self.check_installed_plugin_version()?;
        let Some(installed_version) = installed_version else {
            return Err(ActrCliError::command_error(
                "Failed to determine installed protoc-gen-actrframework version after install"
                    .to_string(),
            ));
        };

        if installed_version == required_version {
            return Ok(());
        }

        Err(ActrCliError::command_error(format!(
            "protoc-gen-actrframework version {} does not match actr version {}",
            installed_version, required_version
        )))
    }
}

fn src_dir_from_output(output: &Path) -> PathBuf {
    output
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("src"))
}

fn rust_message_import_name(type_ref: &crate::commands::codegen::TypeRef) -> String {
    let Some(relative_type) = owner_relative_proto_type(type_ref) else {
        return type_ref.type_name.clone();
    };
    let parts = relative_type.split('.').collect::<Vec<_>>();
    let Some((type_name, parents)) = parts.split_last() else {
        return type_ref.type_name.clone();
    };
    if parents.is_empty() {
        type_name.to_string()
    } else {
        let parent_modules = parents
            .iter()
            .map(|part| to_snake_case(part))
            .collect::<Vec<_>>()
            .join("::");
        format!("{parent_modules}::{type_name}")
    }
}

fn owner_relative_proto_type(type_ref: &crate::commands::codegen::TypeRef) -> Option<&str> {
    let proto_type = type_ref.proto_type.trim().trim_start_matches('.');
    if proto_type.is_empty() {
        return None;
    }
    if type_ref.proto_package.is_empty() {
        Some(proto_type)
    } else {
        proto_type
            .strip_prefix(&type_ref.proto_package)
            .and_then(|relative| relative.strip_prefix('.'))
    }
}

fn rust_scaffold_qualified_type(
    type_ref: &crate::commands::codegen::TypeRef,
    fallback: &str,
) -> String {
    let module = if type_ref.proto_package.is_empty() {
        type_ref
            .proto_file
            .rsplit('/')
            .next()
            .and_then(|file| file.strip_suffix(".proto"))
            .map(to_snake_case)
            .unwrap_or_else(|| "proto".to_string())
    } else {
        type_ref.proto_package.replace('.', "_")
    };
    let name = rust_message_import_name(type_ref);
    if name.is_empty() {
        format!("crate::generated::{module}::{fallback}")
    } else {
        format!("crate::generated::{module}::{name}")
    }
}

fn is_default_cargo_lib_rs(content: &str) -> bool {
    content.contains("pub fn add(left: u64, right: u64) -> u64")
        && content.contains("fn it_works()")
}

fn is_generated_entry_placeholder(content: &str) -> bool {
    let mut lines = content.lines();
    let Some(marker) = lines.next() else {
        return false;
    };

    // `ENTRY_SCAFFOLD_MARKER` was used by the empty project template before
    // placeholders received their own marker. Match the complete placeholder
    // shape so legacy projects are migrated without treating a full scaffold
    // or a user-edited placeholder as tool-owned.
    if marker != ENTRY_PLACEHOLDER_MARKER && marker != ENTRY_SCAFFOLD_MARKER {
        return false;
    }

    let Some(title) = lines.next() else {
        return false;
    };
    if !title.starts_with("//! ") || !title.ends_with(" - empty Actor-RTC workload shell") {
        return false;
    }

    [
        "//!",
        "//! Add service definitions to `protos/local/local.proto`, then run",
        "//! `actr gen -l rust` to generate framework code under `src/generated/`.",
        "",
        "#[cfg(actr_has_generated)]",
        "pub mod generated;",
    ]
    .into_iter()
    .all(|expected| lines.next() == Some(expected))
        && lines.next().is_none()
}

/// Recursively make all files in a directory (or a file) writable.
fn make_writable_recursive(path: &Path) -> Result<()> {
    use std::fs;

    if path.is_file() {
        let metadata = fs::metadata(path).map_err(|e| {
            ActrCliError::config_error(format!("Failed to read file metadata: {e}"))
        })?;
        let mut permissions = metadata.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = permissions.mode();
            permissions.set_mode(mode | 0o222);
        }

        #[cfg(not(unix))]
        {
            permissions.set_readonly(false);
        }

        fs::set_permissions(path, permissions).map_err(|e| {
            ActrCliError::config_error(format!("Failed to reset file permissions: {e}"))
        })?;
    } else if path.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|e| ActrCliError::config_error(format!("Failed to read directory: {e}")))?
        {
            let entry = entry.map_err(|e| ActrCliError::config_error(e.to_string()))?;
            make_writable_recursive(&entry.path())?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "rust_tests.rs"]
mod tests;
