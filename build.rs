use std::env;
use std::path::{Path, PathBuf};

/// Candidate directories that might contain a LiteRT-LM checkout.
fn litert_lm_candidates(manifest: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(d) = env::var("LITERT_LM_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(parent) = manifest.parent() {
        for name in ["LiteRT-LM", "litert-lm"] {
            dirs.push(parent.join(name));
        }
    }
    dirs
}

/// Map the Rust target triple to the prebuilt subdirectory.
///
/// Prebuilt libraries are produced by `LiteRT-LM/scripts/build_engine_shared.sh`
/// and live under `prebuilt/<platform>/libengine_shared.dylib`.
fn prebuilt_subdir(target: &str) -> &'static str {
    match target {
        t if t.contains("ios-sim") || t.contains("ios_sim") => "prebuilt/ios_sim_arm64",
        t if t.contains("ios") => "prebuilt/ios_arm64",
        t if t.contains("aarch64-apple-darwin") => "prebuilt/macos_arm64",
        // Fallback for other macOS hosts (x86_64) — would need its own prebuilt.
        _ => "prebuilt/macos_arm64",
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LITERT_LM_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LIB_PATH");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();

    // ── Locate LiteRT-LM repo ───────────────────────────────────────────
    let candidates = litert_lm_candidates(&manifest);

    // ── Header resolution ───────────────────────────────────────────────
    let c_header = candidates
        .iter()
        .map(|d| d.join("c/engine.h"))
        .find(|p| p.exists())
        .unwrap_or_else(|| {
            panic!(
                "Could not find c/engine.h in any LiteRT-LM checkout.\n\
                 Searched: {:?}\n\
                 Set LITERT_LM_DIR to the LiteRT-LM repo root.",
                candidates
            );
        });
    println!("cargo:rerun-if-changed={}", c_header.display());

    // ── Bindgen ─────────────────────────────────────────────────────────
    let mut builder = bindgen::Builder::default()
        .header(c_header.to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("litert_lm_.*")
        .allowlist_type("LiteRtLm.*")
        .allowlist_type("InputData.*")
        .allowlist_var("kInput.*")
        .allowlist_type("Type")
        .allowlist_var("kType.*")
        .allowlist_var("kTop.*")
        .allowlist_var("kGreedy")
        .generate_comments(true);

    // Cross-compiling for iOS: tell clang the target and sysroot.
    if target.contains("ios") {
        // Rust uses "aarch64-apple-ios-sim" but clang wants "aarch64-apple-ios-simulator".
        let clang_target = target.replace("ios-sim", "ios-simulator");
        builder = builder.clang_arg(format!("--target={clang_target}"));
        let sdk = if target.contains("sim") { "iphonesimulator" } else { "iphoneos" };
        if let Ok(output) = std::process::Command::new("xcrun")
            .args(["--sdk", sdk, "--show-sdk-path"])
            .output()
        {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sysroot.is_empty() {
                builder = builder.clang_arg(format!("-isysroot{sysroot}"));
            }
        }
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    PathBuf::from(env::var("OUT_DIR").unwrap())
        .join("bindings.rs")
        .pipe(|p| bindings.write_to_file(p).expect("Couldn't write bindings!"));

    // ── Locate libengine_shared.dylib ───────────────────────────────────
    let lib_name = "libengine_shared.dylib";
    let subdir = prebuilt_subdir(&target);
    let mut lib_dirs: Vec<PathBuf> = Vec::new();

    // Explicit override takes priority.
    if let Ok(p) = env::var("LITERT_LM_LIB_PATH") {
        lib_dirs.push(PathBuf::from(p));
    }

    // Prebuilt directory (built by scripts/build_engine_shared.sh).
    for d in &candidates {
        lib_dirs.push(d.join(subdir));
    }

    // Fallback: bazel-bin (only useful for macOS dev builds).
    for d in &candidates {
        lib_dirs.push(d.join("bazel-bin/c"));
    }

    let mut found = false;
    for dir in &lib_dirs {
        let dylib = dir.join(lib_name);
        if dylib.exists() {
            let abs_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            println!("cargo:rustc-link-search=native={}", abs_dir.display());

            // Patch install_name to absolute path on macOS host builds so the
            // binary finds the dylib without DYLD_LIBRARY_PATH.
            // iOS uses @rpath — do not patch.
            if !target.contains("ios") {
                let abs_dylib = abs_dir.join(lib_name);
                let _ = std::process::Command::new("chmod")
                    .args(["u+w", &dylib.to_string_lossy()])
                    .status();
                let _ = std::process::Command::new("install_name_tool")
                    .args(["-id", &abs_dylib.to_string_lossy(), &dylib.to_string_lossy()])
                    .status();
            }

            found = true;
            break;
        }
    }
    if !found {
        println!(
            "cargo:warning=Could not find {lib_name} for target {target}. Searched: {lib_dirs:?}"
        );
    }

    println!("cargo:rustc-link-lib=dylib=engine_shared");
    println!("cargo:rustc-link-lib=dylib=c++");
}

/// Extension trait to avoid a temporary variable for write_to_file.
trait Pipe: Sized {
    fn pipe(self, f: impl FnOnce(Self)) {
        f(self)
    }
}
impl Pipe for PathBuf {}
