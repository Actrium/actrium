use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Copy the generated app's registry overrides into an isolated test project.
pub fn append_upstream_patches(workspace: &Path, manifest: &Path) -> Result<()> {
    let template = workspace.join("crates/actr-cli/fixtures/rust/Cargo.toml.hbs");
    let source: toml::Value = toml::from_str(&fs::read_to_string(template)?)?;
    let original = fs::read_to_string(manifest)?;
    let target: toml::Value = toml::from_str(&original)?;
    let source_patches = source
        .get("patch")
        .and_then(toml::Value::as_table)
        .context("app template is missing upstream patches")?;
    let mut additions = String::new();

    for (registry, entries) in source_patches {
        let entries = entries
            .as_table()
            .context("invalid app template patch table")?;
        for (name, expected) in entries {
            let current = target
                .get("patch")
                .and_then(|patch| patch.get(registry))
                .and_then(|patch| patch.get(name));
            if let Some(current) = current {
                if current != expected {
                    bail!("conflicting patch for {name} in {}", manifest.display());
                }
                continue;
            }
            additions.push_str(&format!("\n[patch.{registry:?}.{name:?}]\n"));
            additions.push_str(&toml::to_string(expected)?);
        }
    }
    if !additions.is_empty() {
        fs::write(manifest, original + &additions)?;
    }
    Ok(())
}

/// Route Actr crates to a local checkout while preserving existing overrides.
pub fn align_with_workspace(workspace: &Path, manifest: &Path) -> Result<()> {
    let original = fs::read_to_string(manifest)?;
    let target: toml::Value = toml::from_str(&original)?;
    let mut additions = String::new();
    if target.get("workspace").is_none() {
        additions.push_str("\n[workspace]\n");
    }
    for name in [
        "actr",
        "actr-protocol",
        "actr-framework",
        "actr-hyper",
        "actr-runtime",
        "actr-config",
        "actr-service-compat",
        "actr-runtime-mailbox",
    ] {
        let path = workspace.join("crates").join(name);
        let value = path.to_string_lossy().replace('\\', "/");
        let current = target
            .get("patch")
            .and_then(|patch| patch.get("crates-io"))
            .and_then(|patch| patch.get(name));
        if let Some(current) = current {
            if current.get("path").and_then(toml::Value::as_str) != Some(value.as_str()) {
                bail!("conflicting patch for {name} in {}", manifest.display());
            }
            continue;
        }
        additions.push_str(&format!("\n[patch.crates-io.{name:?}]\n"));
        additions.push_str(&format!("path = {}\n", toml::Value::String(value)));
    }
    if !additions.is_empty() {
        fs::write(manifest, original + &additions)?;
    }
    append_upstream_patches(workspace, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_template(root: &Path, contents: &str) -> Result<()> {
        let template = root.join("crates/actr-cli/fixtures/rust/Cargo.toml.hbs");
        fs::create_dir_all(template.parent().unwrap())?;
        fs::write(template, contents)?;
        Ok(())
    }

    #[test]
    fn merges_upstream_patches_without_overwriting_existing_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("workspace");
        fs::create_dir(&root)?;
        write_template(
            &root,
            "[patch.crates-io]\nturn = { git = \"https://example.com/turn\", rev = \"abc\" }\n\n[patch.\"https://example.com/webrtc\"]\nrtc = { git = \"https://example.com/rtc\", rev = \"def\" }\n",
        )?;
        let manifest = temp.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[patch.crates-io]\nactr = { path = \"../actr\" }\n",
        )?;
        append_upstream_patches(&root, &manifest)?;
        let updated = fs::read_to_string(&manifest)?;
        let parsed: toml::Value = toml::from_str(&updated)?;
        assert_eq!(
            parsed["patch"]["crates-io"]["actr"]["path"].as_str(),
            Some("../actr")
        );
        assert_eq!(
            parsed["patch"]["crates-io"]["turn"]["rev"].as_str(),
            Some("abc")
        );
        assert_eq!(
            parsed["patch"]["https://example.com/webrtc"]["rtc"]["rev"].as_str(),
            Some("def")
        );
        append_upstream_patches(&root, &manifest)?;
        assert_eq!(fs::read_to_string(&manifest)?, updated);
        Ok(())
    }

    #[test]
    fn rejects_conflicting_patch_without_writing() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("workspace");
        fs::create_dir(&root)?;
        write_template(
            &root,
            "[patch.crates-io]\nturn = { git = \"https://example.com/turn\", rev = \"abc\" }\n",
        )?;
        let manifest = temp.path().join("Cargo.toml");
        let original = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[patch.crates-io]\nturn = { path = \"../turn\" }\n";
        fs::write(&manifest, original)?;
        assert!(append_upstream_patches(&root, &manifest).is_err());
        assert_eq!(fs::read_to_string(&manifest)?, original);
        Ok(())
    }

    #[test]
    fn local_workspace_patches_preserve_an_existing_template_table() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("workspace with spaces");
        fs::create_dir(&root)?;
        write_template(
            &root,
            "[patch.crates-io]\nwebrtc = { git = \"https://example.com/webrtc\", rev = \"abc\" }\n",
        )?;
        let manifest = temp.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[workspace]\n\n[patch.crates-io]\nwebrtc = { git = \"https://example.com/webrtc\", rev = \"abc\" }\n",
        )?;
        align_with_workspace(&root, &manifest)?;
        let updated = fs::read_to_string(&manifest)?;
        let parsed: toml::Value = toml::from_str(&updated)?;
        assert_eq!(
            parsed["patch"]["crates-io"]["actr"]["path"].as_str(),
            Some(root.join("crates/actr").to_string_lossy().as_ref())
        );
        align_with_workspace(&root, &manifest)?;
        assert_eq!(fs::read_to_string(&manifest)?, updated);
        Ok(())
    }
}
