use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LITERT_LM_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LIB_PATH");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_path = PathBuf::from(&manifest_dir);

    // ── Header resolution ───────────────────────────────────────────────
    let c_header = if manifest_path.join("c/engine.h").exists() {
        manifest_path.join("c/engine.h")
    } else {
        manifest_path.parent().unwrap().join("c/engine.h")
    };
    if !c_header.exists() {
        panic!("Could not find c/engine.h at: {}", c_header.display());
    }
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

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    // ── Locate libengine_shared.dylib ───────────────────────────────────
    //
    // Search order:
    //   1. LITERT_LM_LIB_PATH  — explicit directory
    //   2. LITERT_LM_DIR       — repo root, we look in bazel-bin/c/
    //   3. Sibling directory    — ../LiteRT-LM/bazel-bin/c/
    let lib_name = "libengine_shared.dylib";
    let mut lib_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(p) = env::var("LITERT_LM_LIB_PATH") {
        lib_dirs.push(PathBuf::from(p));
    }
    if let Ok(d) = env::var("LITERT_LM_DIR") {
        lib_dirs.push(PathBuf::from(d).join("bazel-bin/c"));
    }
    if let Some(parent) = manifest_path.parent() {
        for name in ["LiteRT-LM", "litert-lm"] {
            lib_dirs.push(parent.join(name).join("bazel-bin/c"));
        }
    }

    let mut found = false;
    for dir in &lib_dirs {
        let dylib = dir.join(lib_name);
        if dylib.exists() {
            let abs_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            let abs_dylib = abs_dir.join(lib_name);

            // Tell the linker where to find the library at build time.
            println!("cargo:rustc-link-search=native={}", abs_dir.display());

            // Patch the dylib's install_name to its absolute path so that
            // any binary linking against it — including downstream consumers
            // — can load it at runtime without needing rpath or
            // DYLD_LIBRARY_PATH.  (cargo:rustc-link-arg does NOT propagate
            // from dependency build scripts to the final binary, so rpath
            // is not a viable strategy for library crates.)
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

    // Link the shared library.
    println!("cargo:rustc-link-lib=dylib=engine_shared");

    // Link C++ standard library.
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=dylib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");
}
