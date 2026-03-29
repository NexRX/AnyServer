fn main() {
    // ts-rs generates bindings at compile time via the #[derive(TS)] macro.
    // The actual TypeScript file generation happens when running `cargo test`,
    // which triggers the ts-rs export attributes on our types.
    // We rerun if types.rs changes so the build is aware of type changes.
    println!("cargo:rerun-if-changed=src/types.rs");

    // When the `bundle-frontend` feature is enabled, build the frontend and
    // embed the output into the binary via rust-embed.
    #[cfg(feature = "bundle-frontend")]
    {
        use std::path::Path;
        use std::process::Command;

        let frontend_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend");
        let dist_dir = frontend_dir.join("dist");

        // Rerun if any frontend source files change.
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("src").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("index.html").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("package.json").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("vite.config.ts").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("tsconfig.json").display()
        );

        // Skip the build if the dist directory already exists and SKIP_FRONTEND_BUILD is set.
        // This is useful for CI caching or when iterating on the backend only.
        if std::env::var("SKIP_FRONTEND_BUILD").is_ok() && dist_dir.exists() {
            println!(
                "cargo:warning=SKIP_FRONTEND_BUILD set and dist/ exists — skipping frontend build"
            );
            return;
        }

        // Detect package manager: prefer pnpm, then npm.
        let (pm, install_args, build_args) = if which_exists("pnpm") {
            ("pnpm", vec!["install"], vec!["run", "build"])
        } else if which_exists("npm") {
            ("npm", vec!["install"], vec!["run", "build"])
        } else {
            println!("cargo:warning=Neither pnpm nor npm found — skipping frontend build. Pre-build the frontend manually into frontend/dist/");
            return;
        };

        // Install dependencies.
        println!("cargo:warning=Installing frontend dependencies with {pm}...");
        let status = Command::new(pm)
            .args(&install_args)
            .current_dir(&frontend_dir)
            .status()
            .unwrap_or_else(|e| panic!("Failed to run `{pm} install`: {e}"));

        if !status.success() {
            panic!("`{pm} install` failed with status {status}");
        }

        // Build the frontend.
        println!("cargo:warning=Building frontend with {pm}...");
        let status = Command::new(pm)
            .args(&build_args)
            .current_dir(&frontend_dir)
            .status()
            .unwrap_or_else(|e| panic!("Failed to run `{pm} run build`: {e}"));

        if !status.success() {
            panic!("`{pm} run build` failed with status {status}");
        }

        if !dist_dir.join("index.html").exists() {
            panic!(
                "Frontend build succeeded but {}/index.html not found",
                dist_dir.display()
            );
        }

        println!("cargo:warning=Frontend build complete.");
    }
}

/// Check whether a command exists on PATH.
#[cfg(feature = "bundle-frontend")]
fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
