fn main() {
    // Rebuild when admin UI source changes
    println!("cargo:rerun-if-changed=admin/web/src");
    println!("cargo:rerun-if-changed=admin/web/index.html");
    println!("cargo:rerun-if-changed=admin/web/package.json");

    #[cfg(feature = "admin-ui")]
    {
        let dist_path = std::path::Path::new("admin/web/dist");
        if !dist_path.exists() {
            println!("cargo:warning=Admin UI dist not found, building...");

            // Check if pnpm is available
            let has_pnpm = std::process::Command::new("pnpm")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            let package_manager = if has_pnpm {
                "pnpm"
            } else {
                // Fallback to npm
                "npm"
            };

            // Install dependencies if node_modules doesn't exist
            let node_modules = std::path::Path::new("admin/web/node_modules");
            if !node_modules.exists() {
                println!("cargo:warning=Installing npm dependencies...");
                let status = std::process::Command::new(package_manager)
                    .args(["install"])
                    .current_dir("admin/web")
                    .status()
                    .expect("Failed to run package manager");

                if !status.success() {
                    panic!("Failed to install npm dependencies");
                }
            }

            // Build the frontend
            println!("cargo:warning=Building Admin UI...");
            let status = std::process::Command::new(package_manager)
                .args(["run", "build"])
                .current_dir("admin/web")
                .status()
                .expect("Failed to build admin UI");

            if !status.success() {
                panic!("Failed to build admin UI");
            }

            println!("cargo:warning=Admin UI built successfully");
        }
    }
}
