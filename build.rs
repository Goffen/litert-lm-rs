use std::env;
use std::path::PathBuf;

/// Candidate directories that might contain a LiteRT-LM checkout.
fn litert_lm_candidates(manifest: &PathBuf) -> Vec<PathBuf> {
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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LITERT_LM_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LIB_PATH");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // ── Locate LiteRT-LM repo ───────────────────────────────────────────
    let candidates = litert_lm_candidates(&manifest);

    // ── Header resolution ───────────────────────────────────────────────
    // Prefer the upstream header from the LiteRT-LM repo so it's always
    // in sync with the built shared library.
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
    let bindings = bindgen::Builder::default()
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
        .generate_comments(true)
        .generate()
        .expect("Unable to generate bindings");

    PathBuf::from(env::var("OUT_DIR").unwrap())
        .join("bindings.rs")
        .pipe(|p| bindings.write_to_file(p).expect("Couldn't write bindings!"));

    // ── Locate libengine_shared.dylib ───────────────────────────────────
    let lib_name = "libengine_shared.dylib";
    let mut lib_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(p) = env::var("LITERT_LM_LIB_PATH") {
        lib_dirs.push(PathBuf::from(p));
    }
    for d in &candidates {
        lib_dirs.push(d.join("bazel-bin/c"));
    }

    let mut found = false;
    for dir in &lib_dirs {
        let dylib = dir.join(lib_name);
        if dylib.exists() {
            let abs_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            let abs_dylib = abs_dir.join(lib_name);

            println!("cargo:rustc-link-search=native={}", abs_dir.display());

            // Patch install_name to absolute path so downstream consumers
            // find it without rpath (cargo:rustc-link-arg doesn't propagate
            // from dependency build scripts).
            let _ = std::process::Command::new("chmod")
                .args(["u+w", &dylib.to_string_lossy()])
                .status();
            let _ = std::process::Command::new("install_name_tool")
                .args(["-id", &abs_dylib.to_string_lossy(), &dylib.to_string_lossy()])
                .status();

            found = true;
            break;
        }
    }
    if !found {
        println!("cargo:warning=Could not find {lib_name}. Set LITERT_LM_DIR or LITERT_LM_LIB_PATH.");
    }

    println!("cargo:rustc-link-lib=dylib=engine_shared");

    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=dylib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

/// Extension trait to avoid a temporary variable for write_to_file.
trait Pipe: Sized {
    fn pipe(self, f: impl FnOnce(Self)) {
        f(self)
    }
}
impl Pipe for PathBuf {}
