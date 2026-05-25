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
/// (macOS / iOS) or `bazel build --config=android_arm64 //c:libengine_shared.dylib`
/// (Android) and live under `prebuilt/<platform>/libengine_shared.{dylib,so}`.
fn prebuilt_subdir(target: &str) -> &'static str {
    match target {
        t if t.contains("ios-sim") || t.contains("ios_sim") => "prebuilt/ios_sim_arm64",
        t if t.contains("ios") => "prebuilt/ios_arm64",
        t if t.contains("aarch64-linux-android") => "prebuilt/android_arm64",
        t if t.contains("aarch64-apple-darwin") => "prebuilt/macos_arm64",
        // Fallback for other macOS hosts (x86_64) — would need its own prebuilt.
        _ => "prebuilt/macos_arm64",
    }
}

/// Library file name for the engine on this target. Android uses ELF `.so`;
/// every other supported target is Mach-O `.dylib`.
fn engine_lib_name(target: &str) -> &'static str {
    if target.contains("android") {
        "libengine_shared.so"
    } else {
        "libengine_shared.dylib"
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
        .allowlist_var("kLiteRtLm.*")
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

    // Cross-compiling for Android: point bindgen's libclang at the NDK
    // sysroot so it can find <stdint.h>, <stdbool.h>, etc. Honors
    // ANDROID_NDK_HOME first, then NDK_HOME.
    if target.contains("android") {
        builder = builder.clang_arg(format!("--target={target}"));
        let ndk = env::var("ANDROID_NDK_HOME")
            .or_else(|_| env::var("NDK_HOME"))
            .ok();
        if let Some(ndk) = ndk {
            // NDK r23+: prebuilt/<host>/sysroot. Detect host dir.
            let toolchains = PathBuf::from(&ndk).join("toolchains/llvm/prebuilt");
            if let Ok(read) = std::fs::read_dir(&toolchains) {
                if let Some(host) = read
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .find(|p| p.is_dir())
                {
                    let sysroot = host.join("sysroot");
                    if sysroot.exists() {
                        builder = builder.clang_arg(format!("--sysroot={}", sysroot.display()));
                    }
                }
            }
        }
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    PathBuf::from(env::var("OUT_DIR").unwrap())
        .join("bindings.rs")
        .pipe(|p| bindings.write_to_file(p).expect("Couldn't write bindings!"));

    // ── Locate engine shared library ────────────────────────────────────
    let lib_name = engine_lib_name(&target);
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

    // Re-run if any candidate path appears or disappears. Without this
    // cargo caches the very first lookup result (including a "not found"
    // warning + missing rustc-link-search) and never re-runs build.rs when
    // the .so is staged later — a successful subsequent build silently
    // re-emits the cached link error.
    for dir in &lib_dirs {
        println!("cargo:rerun-if-changed={}", dir.join(lib_name).display());
    }

    let mut found = false;
    for dir in &lib_dirs {
        let lib_path = dir.join(lib_name);
        if lib_path.exists() {
            let abs_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            println!("cargo:rustc-link-search=native={}", abs_dir.display());

            // Patch install_name to absolute path on macOS host builds so the
            // binary finds the dylib without DYLD_LIBRARY_PATH. iOS uses @rpath
            // (set at engine build time). Android uses ELF + RUNPATH which the
            // APK packaging handles via jniLibs — install_name_tool is Mach-O
            // only and would just no-op anyway.
            //
            // Idempotency matters: install_name_tool rewrites the dylib
            // unconditionally, updating its mtime. With the rerun-if-changed
            // lines above watching this same path, an unconditional patch
            // would invalidate the build script fingerprint every build.
            // Read the current `LC_ID_DYLIB` first and skip when it already
            // matches.
            if !target.contains("ios") && !target.contains("android") {
                let abs_lib = abs_dir.join(lib_name);
                let want = abs_lib.to_string_lossy();
                let current = std::process::Command::new("otool")
                    .args(["-D", &lib_path.to_string_lossy()])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    // `otool -D` prints "<path>:\n<install_name>".
                    .and_then(|s| s.lines().nth(1).map(str::to_string))
                    .unwrap_or_default();
                if current.trim() != want {
                    let _ = std::process::Command::new("chmod")
                        .args(["u+w", &lib_path.to_string_lossy()])
                        .status();
                    let _ = std::process::Command::new("install_name_tool")
                        .args(["-id", &want, &lib_path.to_string_lossy()])
                        .status();
                }
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
    // C++ stdlib: NDK ships libc++_shared.so; Apple targets ship libc++.dylib.
    let cxx = if target.contains("android") {
        "c++_shared"
    } else {
        "c++"
    };
    println!("cargo:rustc-link-lib=dylib={cxx}");
}

/// Extension trait to avoid a temporary variable for write_to_file.
trait Pipe: Sized {
    fn pipe(self, f: impl FnOnce(Self)) {
        f(self)
    }
}
impl Pipe for PathBuf {}
