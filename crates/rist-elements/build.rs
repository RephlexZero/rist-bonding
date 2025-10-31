// Build script to automatically build GStreamer submodule if needed
// This ensures the custom GStreamer with patched RIST plugin is available

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = crate_dir.parent().unwrap().parent().unwrap().to_path_buf();
    let gstreamer_dir = workspace_dir.join("gstreamer");
    let target_root = workspace_dir.join("target").join("gstreamer");
    let install_prefix = target_root.join("install");
    let overlay_dir = target_root.join("overlay");
    let build_script = workspace_dir.join("build_gstreamer.sh");

    // Tell cargo to rerun this script if key files change
    println!("cargo:rerun-if-changed=../../gstreamer/meson.build");
    println!("cargo:rerun-if-changed=../../build_gstreamer.sh");
    println!("cargo:rerun-if-env-changed=FORCE_GSTREAMER_BUILD");

    // Check if GStreamer submodule is initialized, and initialize it if needed
    if !gstreamer_dir.join("meson.build").exists() {
        eprintln!();
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!("GStreamer submodule not initialized - initializing now...");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!();

        // Try to initialize the submodule automatically
        let status = Command::new("git")
            .args(["submodule", "update", "--init", "--recursive", "gstreamer"])
            .current_dir(&workspace_dir)
            .status();

        match status {
            Ok(s) if s.success() => {
                eprintln!("✓ GStreamer submodule initialized successfully");
                eprintln!();
            }
            Ok(s) => {
                eprintln!();
                eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                eprintln!("ERROR: Failed to initialize GStreamer submodule");
                eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                eprintln!();
                eprintln!("Exit code: {:?}", s.code());
                eprintln!();
                eprintln!("Please initialize manually:");
                eprintln!("  git submodule update --init --recursive gstreamer");
                eprintln!();
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!();
                eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                eprintln!("ERROR: Failed to run git command");
                eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                eprintln!();
                eprintln!("Error: {}", e);
                eprintln!();
                eprintln!("This might be because:");
                eprintln!("  - git is not installed");
                eprintln!("  - you're building from a tarball (not a git repo)");
                eprintln!();
                eprintln!("Please initialize the submodule manually:");
                eprintln!("  git submodule update --init --recursive gstreamer");
                eprintln!();
                std::process::exit(1);
            }
        }

        // Verify it worked
        if !gstreamer_dir.join("meson.build").exists() {
            eprintln!();
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            eprintln!("ERROR: Submodule initialization reported success but files not found");
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            eprintln!();
            eprintln!("Please try manually:");
            eprintln!("  git submodule update --init --recursive");
            eprintln!();
            std::process::exit(1);
        }
    }

    // Check if GStreamer is built
    let gst_is_built = install_prefix.exists() && overlay_dir.exists();

    // Check if we should force a rebuild
    let force_build = env::var("FORCE_GSTREAMER_BUILD").is_ok();

    if !gst_is_built || force_build {
        eprintln!();
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        if force_build {
            eprintln!("FORCE_GSTREAMER_BUILD set - rebuilding GStreamer...");
        } else {
            eprintln!("GStreamer not found - building it now...");
        }
        eprintln!("This may take several minutes on first build.");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!();

        // Run the build script
        let status = Command::new("bash")
            .arg(&build_script)
            .current_dir(&workspace_dir)
            .env("TARGET_ROOT", &target_root)
            .status()
            .expect("Failed to execute build_gstreamer.sh");

        if !status.success() {
            eprintln!();
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            eprintln!("ERROR: GStreamer build failed");
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            eprintln!();
            eprintln!("The build_gstreamer.sh script failed. Check the output above.");
            eprintln!("You may need to install build dependencies:");
            eprintln!("  - meson (>= 1.4)");
            eprintln!("  - ninja");
            eprintln!("  - GStreamer development packages");
            eprintln!("  - C/C++ compiler toolchain");
            eprintln!();
            std::process::exit(1);
        }

        eprintln!();
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!("GStreamer build complete!");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!();
    }

    // Find the library directories
    let lib_dirs = [
        install_prefix.join("lib"),
        install_prefix.join("lib64"),
        install_prefix.join("lib").join("x86_64-linux-gnu"),
    ];

    // Tell cargo where to find libraries at link time
    for lib_dir in &lib_dirs {
        if lib_dir.exists() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());

            // Also set rpath so runtime linking works
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
        }
    }
}
