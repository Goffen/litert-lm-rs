//! # LiteRT-LM Rust Bindings
//!
//! Safe Rust wrapper for the [LiteRT-LM](https://github.com/google-ai-edge/LiteRT-LM) C API.
//! Supports text generation, multi-turn conversations with tool calling,
//! and multimodal input (images, audio).
//!
//! ## Quick start
//!
//! ```no_run
//! use litert_lm::{Engine, Conversation};
//!
//! let engine = Engine::new("model.litertlm")?;
//! let mut convo = Conversation::new(&engine)?;
//! let response = convo.send("What is 2 + 2?")?;
//! println!("{}", response.text().unwrap_or_default());
//! # Ok::<(), litert_lm::Error>(())
//! ```

use std::ffi::{CStr, CString};
use std::fmt;

// Auto-generated FFI bindings from bindgen.
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[allow(clippy::all)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use bindings::*;

// ============================================================================
// Public Types
// ============================================================================

/// Backend type for model execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Gpu,
}

impl Backend {
    fn as_str(&self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Gpu => "gpu",
        }
    }
}

/// Error type for LiteRT-LM operations.
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LiteRT-LM: {}", self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Response
// ============================================================================

/// A response from the model, which may contain text, tool calls, or both.
///
/// The underlying data is JSON from the C API. Accessor methods extract
/// specific fields without requiring a JSON library as a dependency.
#[derive(Debug, Clone)]
pub struct Response {
    json: String,
}

impl Response {
    /// Construct a Response, stripping Gemma's `<|"|>` string-quoting tokens
    /// from tool-call arguments so callers get clean JSON.
    fn new(raw_json: String) -> Self {
        Response {
            json: strip_gemma_quote_tokens(&raw_json),
        }
    }

    /// The JSON string returned by the C API (with model artifacts cleaned).
    ///
    /// Typical shapes:
    /// - Text: `{"role":"assistant","content":[{"type":"text","text":"..."}]}`
    /// - Tool call: `{"tool_calls":[{"type":"function","function":{"name":"...","arguments":{...}}}]}`
    pub fn json(&self) -> &str {
        &self.json
    }

    /// Extract the model's text reply, if any.
    ///
    /// Returns `None` when the response is purely a tool call.
    pub fn text(&self) -> Option<String> {
        let text = extract_json_text_field(&self.json);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// Whether the response contains one or more tool calls.
    pub fn has_tool_calls(&self) -> bool {
        self.json.contains("\"tool_calls\"")
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.text() {
            Some(t) => f.write_str(&t),
            None => f.write_str(&self.json),
        }
    }
}

// ============================================================================
// Engine
// ============================================================================

/// Activation data type for GPU inference.
///
/// Lower precision reduces memory usage during model compilation and inference,
/// at the cost of reduced numerical accuracy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ActivationDataType {
    F32 = 0,
    F16 = 1,
    I16 = 2,
    I8 = 3,
}

/// Configuration options for engine creation.
///
/// Use with [`Engine::with_config`] to override defaults.
#[derive(Default, Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of tokens (context window size).
    /// Defaults to the model's built-in value when `None`.
    pub max_num_tokens: Option<i32>,
    /// Activation data type for GPU inference.
    /// Defaults to the engine's built-in value when `None` (typically F32).
    /// Use `F16` on memory-constrained devices to halve activation memory.
    pub activation_data_type: Option<ActivationDataType>,
    /// Backend for the vision encoder. When `None`, defaults to GPU on Apple,
    /// CPU elsewhere. Google's Gallery app uses GPU vision for Gemma 4.
    pub vision_backend: Option<Backend>,
    /// Backend for the audio encoder. When `None`, the audio modality is
    /// disabled — multimodal messages with `{"type":"audio",...}` content
    /// will fail. Set to `Some(Backend::Cpu)` (the safe default) or
    /// `Some(main_backend)` to opt in. Required for Gemma 4 audio input
    /// (ASR, translation, audio understanding).
    pub audio_backend: Option<Backend>,
    /// Directory for caching compiled shaders and XNNPACK weight caches.
    /// Propagates to both main LLM and vision executors.
    /// Critical on iOS: lets XNNPACK mmap weights from disk instead of malloc.
    pub cache_dir: Option<String>,
    /// Override the main LLM backend. When `None`, the platform default is used
    /// (GPU on Apple/DYLD_LIBRARY_PATH, CPU otherwise).
    pub backend: Option<Backend>,
    /// Enable Multi-Token Prediction (MTP) via speculative decoding. Requires
    /// a model that ships an MTP head (e.g. Gemma 4 E4B). When `None`, the
    /// engine uses its default.
    pub enable_speculative_decoding: Option<bool>,
}

/// Loads a model and serves as factory for [`Session`] and [`Conversation`].
pub struct Engine {
    raw: *mut LiteRtLmEngine,
    _settings: *mut LiteRtLmEngineSettings,
}

unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    /// Load a `.litertlm` model with the platform-appropriate backend.
    ///
    /// - Apple (macOS / iOS): tries GPU (Metal) first, falls back to CPU
    ///   if GPU engine creation fails.
    /// - Other platforms: uses GPU only when `DYLD_LIBRARY_PATH` is set
    ///   (indicating GPU libraries are available), otherwise CPU.
    ///
    /// Vision defaults to the same backend as the main LLM; audio is disabled.
    pub fn new(model_path: &str) -> Result<Self> {
        Self::with_config(model_path, EngineConfig::default())
    }

    /// Load a `.litertlm` model with platform-appropriate backend and custom config.
    ///
    /// See [`EngineConfig`] for available options.
    pub fn with_config(model_path: &str, config: EngineConfig) -> Result<Self> {
        let main_backend = config.backend.unwrap_or_else(|| {
            if cfg!(target_vendor = "apple") || std::env::var("DYLD_LIBRARY_PATH").is_ok() {
                Backend::Gpu
            } else {
                Backend::Cpu
            }
        });
        let vision = Some(config.vision_backend.unwrap_or(main_backend));
        // Audio is opt-in — leaving `audio_backend` as `None` preserves the
        // historical behaviour where the audio modality isn't loaded at all.
        let audio = config.audio_backend;
        Self::create(model_path, main_backend, vision, audio, &config)
    }

    fn create(
        model_path: &str,
        backend: Backend,
        vision_backend: Option<Backend>,
        audio_backend: Option<Backend>,
        config: &EngineConfig,
    ) -> Result<Self> {
        let path = to_cstring(model_path, "model path")?;
        let be = to_cstring(backend.as_str(), "backend")?;
        let vis = vision_backend
            .map(|b| to_cstring(b.as_str(), "vision"))
            .transpose()?;
        let aud = audio_backend
            .map(|b| to_cstring(b.as_str(), "audio"))
            .transpose()?;

        unsafe {
            let settings = litert_lm_engine_settings_create(
                path.as_ptr(),
                be.as_ptr(),
                vis.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
                aud.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            );
            if settings.is_null() {
                return Err(Error::new("Failed to create engine settings"));
            }

            if let Some(max_tokens) = config.max_num_tokens {
                litert_lm_engine_settings_set_max_num_tokens(settings, max_tokens);
            }

            if let Some(adt) = config.activation_data_type {
                litert_lm_engine_settings_set_activation_data_type(settings, adt as i32);
            }

            if let Some(ref dir) = config.cache_dir {
                let dir_c = to_cstring(dir, "cache dir")?;
                litert_lm_engine_settings_set_cache_dir(settings, dir_c.as_ptr());
            }

            if let Some(enable) = config.enable_speculative_decoding {
                litert_lm_engine_settings_set_enable_speculative_decoding(
                    settings, enable,
                );
            }

            let engine = litert_lm_engine_create(settings);
            if engine.is_null() {
                litert_lm_engine_settings_delete(settings);
                return Err(Error::new("Failed to create engine"));
            }

            Ok(Engine {
                raw: engine,
                _settings: settings,
            })
        }
    }

    /// Create a low-level [`Session`] (raw text, no chat template).
    pub fn create_session(&self) -> Result<Session> {
        unsafe {
            let session = litert_lm_engine_create_session(self.raw, std::ptr::null_mut());
            if session.is_null() {
                return Err(Error::new("Failed to create session"));
            }
            Ok(Session { raw: session })
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            litert_lm_engine_delete(self.raw);
            litert_lm_engine_settings_delete(self._settings);
        }
    }
}

// ============================================================================
// Session (low-level, no template)
// ============================================================================

/// Low-level text generation without the chat template.
///
/// Use [`Conversation`] instead for chat, tool calls, and multimodal input.
pub struct Session {
    raw: *mut LiteRtLmSession,
}

unsafe impl Send for Session {}

impl Session {
    /// Generate text from a raw prompt (no chat template applied).
    pub fn generate(&self, prompt: &str) -> Result<String> {
        let prompt_cstr = to_cstring(prompt, "prompt")?;

        unsafe {
            let input_data = LiteRtLmInputData {
                type_: LiteRtLmInputDataType_kLiteRtLmInputDataTypeText,
                data: prompt_cstr.as_ptr() as *const std::ffi::c_void,
                size: prompt.len(),
            };

            let responses = litert_lm_session_generate_content(self.raw, &input_data, 1);
            if responses.is_null() {
                return Err(Error::new("Failed to generate content"));
            }

            let text_ptr = litert_lm_responses_get_response_text_at(responses, 0);
            let result = if !text_ptr.is_null() {
                CStr::from_ptr(text_ptr).to_string_lossy().into_owned()
            } else {
                litert_lm_responses_delete(responses);
                return Err(Error::new("No response generated"));
            };

            litert_lm_responses_delete(responses);
            Ok(result)
        }
    }

    /// Get benchmark info (only available if benchmarking was enabled).
    pub fn get_benchmark_info(&self) -> Result<BenchmarkInfo> {
        unsafe {
            let info = litert_lm_session_get_benchmark_info(self.raw);
            if info.is_null() {
                return Err(Error::new("Failed to get benchmark info"));
            }
            let result = BenchmarkInfo {
                time_to_first_token: litert_lm_benchmark_info_get_time_to_first_token(info),
                num_prefill_turns: litert_lm_benchmark_info_get_num_prefill_turns(info) as usize,
                num_decode_turns: litert_lm_benchmark_info_get_num_decode_turns(info) as usize,
            };
            litert_lm_benchmark_info_delete(info);
            Ok(result)
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { litert_lm_session_delete(self.raw) }
    }
}

// ============================================================================
// Conversation (high-level: chat template, tool calls, multimodal)
// ============================================================================

/// Multi-turn conversation with chat template, tool calling, and multimodal
/// support.
///
/// # Tool calling
///
/// ```no_run
/// # use litert_lm::*;
/// # let engine = Engine::new("m.litertlm")?;
/// let tools = r#"[{"name":"get_weather","description":"Get weather","parameters":{
///     "type":"object","properties":{"location":{"type":"string"}},"required":["location"]}}]"#;
///
/// let mut convo = Conversation::with_config(&engine, None, Some(tools))?;
/// let resp = convo.send("What's the weather in Paris?")?;
///
/// if resp.has_tool_calls() {
///     // Execute the tool, then send the result back:
///     let tool_result = r#"[{"role":"tool","content":{"location":"Paris","temp":22}}]"#;
///     let final_resp = convo.send_json(tool_result)?;
///     println!("{}", final_resp.text().unwrap());
/// }
/// # Ok::<(), Error>(())
/// ```
///
/// # Multimodal (audio/image)
///
/// ```no_run
/// # use litert_lm::*;
/// # let engine = Engine::new("m.litertlm")?;
/// # let mut convo = Conversation::new(&engine)?;
/// let msg = r#"{"role":"user","content":[
///     {"type":"audio","path":"/tmp/recording.wav"},
///     {"type":"text","text":"Describe this audio."}
/// ]}"#;
/// let resp = convo.send_json(msg)?;
/// # Ok::<(), Error>(())
/// ```
pub struct Conversation {
    raw: *mut LiteRtLmConversation,
}

unsafe impl Send for Conversation {}

impl Conversation {
    /// Create a conversation with default config (no tools, no system message).
    pub fn new(engine: &Engine) -> Result<Self> {
        unsafe {
            let raw = litert_lm_conversation_create(engine.raw, std::ptr::null_mut());
            if raw.is_null() {
                return Err(Error::new("Failed to create conversation"));
            }
            Ok(Conversation { raw })
        }
    }

    /// Create a conversation with optional system message and tool declarations.
    ///
    /// - `system_message`: plain text system prompt (e.g. `"You are a helpful assistant."`)
    /// - `tools_json`: JSON array of tool declarations (see [tool-use docs])
    ///
    /// [tool-use docs]: https://github.com/google-ai-edge/LiteRT-LM/blob/main/docs/api/cpp/tool-use.md
    pub fn with_config(
        engine: &Engine,
        system_message: Option<&str>,
        tools_json: Option<&str>,
    ) -> Result<Self> {
        let sys_cstr = system_message
            .map(|s| to_cstring(s, "system_message"))
            .transpose()?;
        let tools_cstr = tools_json
            .map(|s| to_cstring(s, "tools_json"))
            .transpose()?;

        unsafe {
            let config = litert_lm_conversation_config_create();
            if config.is_null() {
                return Err(Error::new("Failed to create conversation config"));
            }

            if let Some(ref s) = sys_cstr {
                litert_lm_conversation_config_set_system_message(config, s.as_ptr());
            }
            if let Some(ref t) = tools_cstr {
                litert_lm_conversation_config_set_tools(config, t.as_ptr());
                litert_lm_conversation_config_set_enable_constrained_decoding(
                    config, true,
                );
            }

            let raw = litert_lm_conversation_create(engine.raw, config);
            litert_lm_conversation_config_delete(config);

            if raw.is_null() {
                return Err(Error::new("Failed to create conversation"));
            }
            Ok(Conversation { raw })
        }
    }

    /// Send a plain-text user message.
    ///
    /// The message is automatically wrapped in `{"role":"user","content":"..."}`.
    pub fn send(&mut self, message: &str) -> Result<Response> {
        let escaped = json_encode_string(message);
        let json = format!(r#"{{"role":"user","content":{escaped}}}"#);
        self.send_raw_json(&json)
    }

    /// Send a raw JSON message.
    ///
    /// Use this for:
    /// - **Tool responses**: `[{"role":"tool","content":{...}}]`
    /// - **Multimodal input**: `{"role":"user","content":[{"type":"image","path":"..."},{"type":"text","text":"..."}]}`
    /// - **Audio input**: `{"role":"user","content":[{"type":"audio","path":"..."},{"type":"text","text":"..."}]}`
    ///
    /// The JSON is passed directly to the C API without modification.
    pub fn send_json(&mut self, json: &str) -> Result<Response> {
        self.send_raw_json(json)
    }

    fn send_raw_json(&mut self, json: &str) -> Result<Response> {
        let cstr = to_cstring(json, "message json")?;

        unsafe {
            let resp = litert_lm_conversation_send_message(
                self.raw,
                cstr.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if resp.is_null() {
                return Err(Error::new("Failed to send message"));
            }

            let json_ptr = litert_lm_json_response_get_string(resp);
            let result = if !json_ptr.is_null() {
                CStr::from_ptr(json_ptr).to_string_lossy().into_owned()
            } else {
                litert_lm_json_response_delete(resp);
                return Err(Error::new("No response from conversation"));
            };

            litert_lm_json_response_delete(resp);
            Ok(Response::new(result))
        }
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        unsafe { litert_lm_conversation_delete(self.raw) }
    }
}

// ============================================================================
// BenchmarkInfo
// ============================================================================

#[derive(Debug, Clone)]
pub struct BenchmarkInfo {
    pub time_to_first_token: f64,
    pub num_prefill_turns: usize,
    pub num_decode_turns: usize,
}

// ============================================================================
// Internal helpers
// ============================================================================

fn to_cstring(s: &str, label: &str) -> Result<CString> {
    CString::new(s).map_err(|e| Error::new(format!("Invalid {label}: {e}")))
}

/// Strip Gemma 4's `<|"|>` string-quoting tokens from a JSON response.
///
/// The model wraps tool-call string arguments in `<|"|>...<|"|>` tokens.
/// Inside JSON these appear as `<|\\\"|>` (escaped quote). We strip both
/// the escaped and unescaped forms so callers get plain values.
fn strip_gemma_quote_tokens(json: &str) -> String {
    json.replace("<|\\\"|>", "").replace("<|\"|>", "")
}

/// Encode a Rust string as a JSON string literal (with surrounding quotes).
fn json_encode_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Extract the `"text"` value from the last `{"type":"text","text":"..."}` in a
/// JSON response. Minimal parser — avoids a serde_json dependency.
fn extract_json_text_field(json: &str) -> String {
    if let Some(pos) = json.rfind("\"text\":\"") {
        let start = pos + "\"text\":\"".len();
        let rest = &json[start..];
        let mut end = 0;
        let bytes = rest.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'\\' {
                end += 2;
            } else if bytes[end] == b'"' {
                break;
            } else {
                end += 1;
            }
        }
        let escaped = &rest[..end];
        return escaped
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    String::new()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_enum() {
        assert_eq!(Backend::Cpu.as_str(), "cpu");
        assert_eq!(Backend::Gpu.as_str(), "gpu");
    }

    #[test]
    fn test_error_display() {
        let err = Error::new("test error");
        assert_eq!(format!("{}", err), "LiteRT-LM: test error");
    }

    #[test]
    fn test_json_encode_string() {
        assert_eq!(json_encode_string("hello"), "\"hello\"");
        assert_eq!(json_encode_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_encode_string("line\nnext"), "\"line\\nnext\"");
    }

    #[test]
    fn test_extract_text_field() {
        let json = r#"{"role":"assistant","content":[{"type":"text","text":"2 + 2 is **4**."}]}"#;
        assert_eq!(extract_json_text_field(json), "2 + 2 is **4**.");
    }

    #[test]
    fn test_extract_text_empty_on_tool_call() {
        let json = r#"{"tool_calls":[{"type":"function","function":{"name":"get_weather"}}]}"#;
        assert_eq!(extract_json_text_field(json), "");
    }

    #[test]
    fn test_response_has_tool_calls() {
        let r = Response::new(r#"{"tool_calls":[{"type":"function"}]}"#.into());
        assert!(r.has_tool_calls());
        assert!(r.text().is_none());

        let r2 = Response::new(r#"{"content":[{"type":"text","text":"hello"}]}"#.into());
        assert!(!r2.has_tool_calls());
        assert_eq!(r2.text().unwrap(), "hello");
    }

    #[test]
    fn test_gemma_quote_tokens_stripped() {
        // The C API returns tool-call string args wrapped in <|\"|>...<|\"|>
        // (the \" is an escaped quote inside JSON string values).
        let raw = r#"{"tool_calls":[{"function":{"name":"search","arguments":{"query":"<|\"|>hello<|\"|>"}}}]}"#;
        let r = Response::new(raw.into());
        assert!(r.json().contains(r#""query":"hello""#), "got: {}", r.json());
        assert!(
            !r.json().contains("<|"),
            "tokens not stripped: {}",
            r.json()
        );
    }
}
