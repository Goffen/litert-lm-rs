# litert-lm-rs

Rust bindings for [LiteRT-LM](https://github.com/google-ai-edge/LiteRT-LM) -- Google's on-device LLM runtime.

## Prerequisites

### 1. Get the engine binaries

**Apple (iOS / macOS) — recommended:** download Google's official
prebuilt `CLiteRTLM` xcframeworks (self-contained C-API binaries; no fork,
no bazel). They're gitignored (~165 MB), so a clean checkout runs this once:

```bash
./scripts/fetch_xcframeworks.sh       # → vendor/CLiteRTLM{,_mac}.xcframework
```

`build.rs` links these automatically on Apple targets.

**Android / source build:** the engine still comes from a bazel build of
`libengine_shared.so`:

```bash
git clone https://github.com/google-ai-edge/LiteRT-LM
cd LiteRT-LM
git lfs pull                          # fetch prebuilt GPU dylibs
./scripts/build_engine_shared.sh      # builds android_arm64 (+ Apple, if no xcframework)
```

> The bazel target is a `cc_binary(linkshared=True)`, not
> `cc_shared_library`. `cc_shared_library` does not transitively link Rust
> static libraries (the Jinja template engine is Rust via CXX bridge),
> leaving symbols unresolved at runtime.

### 2. Download a model

```bash
./scripts/download-model.sh
# defaults to litert-community/gemma-4-E2B-it-litert-lm
# caches to ~/.litert-lm/models/
```

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
litert-lm-rs = { path = "../litert-lm-rs" }
```

`build.rs` selects the engine source per target:
- **Apple**: links the vendored `CLiteRTLM` xcframework (framework on iOS,
  `CLiteRTLM_mac.dylib` on macOS). The dylib is left pristine
  (`@rpath/CLiteRTLM_mac.dylib`, Google's signature); host binaries
  resolve it via an rpath in the consuming workspace's
  `.cargo/config.toml` (build-script rpaths don't propagate to dependents).
- **Android / fallback**: locates `libengine_shared.{so,dylib}` in a
  sibling `LiteRT-LM/` checkout, or via `LITERT_LM_DIR` /
  `LITERT_LM_LIB_PATH`.

### Conversation API (recommended)

Applies the model's chat template; handles thinking channels and stop tokens
correctly. Short factual answers work.

```rust
use litert_lm::{Engine, Backend, Conversation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::new(
        &std::env::var("HOME")
            .map(|h| format!("{h}/.litert-lm/models/litert-community--gemma-4-E2B-it-litert-lm/gemma-4-E2B-it.litertlm"))?,
        Backend::Gpu,
    )?;

    let mut convo = Conversation::new(&engine)?;
    let reply = convo.send("What is 2 + 2?")?;
    println!("{reply}");
    // => "2 + 2 is **4**."

    Ok(())
}
```

### Session API (low-level)

Sends raw text without the chat template. Works for open-ended prompts;
short factual questions may return empty strings.

```rust
let session = engine.create_session()?;
let response = session.generate("Write a haiku about Rust.")?;
```

## GPU support

The engine runs on GPU by default (`Backend::Gpu`). The GPU accelerator
and sampler are loaded dynamically at runtime from the prebuilt dylibs in
`LiteRT-LM/prebuilt/macos_arm64/`. Set `DYLD_LIBRARY_PATH` so `dlopen`
can find them:

```bash
export DYLD_LIBRARY_PATH=/path/to/LiteRT-LM/prebuilt/macos_arm64
cargo run --bin myapp
```

`libengine_shared.dylib` itself does **not** need `DYLD_LIBRARY_PATH` --
`build.rs` patches its `install_name` to an absolute path.

## API

| Type           | Purpose                                                            |
| -------------- | ------------------------------------------------------------------ |
| `Engine`       | Loads a `.litertlm` model. Create one, share across conversations. |
| `Conversation` | Multi-turn chat with template formatting. Use `send(&str)`.        |
| `Session`      | Low-level single-turn generation. Use `generate(&str)`.            |
| `Backend`      | `Cpu` or `Gpu`.                                                    |

## Running examples

```bash
# Batch inference (Conversation API)
DYLD_LIBRARY_PATH=/path/to/LiteRT-LM/prebuilt/macos_arm64 \
  cargo run --example batch_inference /path/to/model.litertlm

# Interactive chat
DYLD_LIBRARY_PATH=/path/to/LiteRT-LM/prebuilt/macos_arm64 \
  cargo run --example simple_chat /path/to/model.litertlm
```

## License

Apache-2.0
