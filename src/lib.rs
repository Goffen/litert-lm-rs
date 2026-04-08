//! # LiteRT-LM Rust Bindings
//!
//! Safe, idiomatic Rust wrapper for the LiteRT-LM C API.
//!
//! ## Features
//!
//! - **Safe API**: Memory-safe wrappers around C FFI
//! - **Automatic cleanup**: RAII-based resource management
//! - **Thread-safe**: Proper Send/Sync implementations
//! - **Error handling**: Result-based error handling
//!
//! ## Example
//!
//! ```no_run
//! use litert_lm::{Engine, Backend};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create engine
//!     let engine = Engine::new("model.tflite", Backend::Cpu)?;
//!
//!     // Create session
//!     let session = engine.create_session()?;
//!
//!     // Generate text
//!     let response = session.generate("Hello, how are you?")?;
//!     println!("Response: {}", response);
//!
//!     Ok(())
//! }
//! ```

use std::ffi::{CStr, CString};
use std::fmt;

// Include auto-generated bindings from bindgen
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

/// Backend type for model execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// CPU backend
    Cpu,
    /// GPU backend (if available)
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

/// Error type for LiteRT-LM operations
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LiteRT-LM Error: {}", self.message)
    }
}

impl std::error::Error for Error {}

/// Result type for LiteRT-LM operations
pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Engine
// ============================================================================

/// LiteRT-LM Engine - the main entry point for loading models
///
/// The Engine loads a model file and prepares it for inference.
/// Create sessions from the engine to perform text generation.
pub struct Engine {
    raw: *mut LiteRtLmEngine,
    _settings: *mut LiteRtLmEngineSettings, // Keep settings alive
}

// Safety: The C API allows engines to be shared between threads
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    /// Create a new Engine from a model file
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the .tflite model file
    /// * `backend` - Backend to use (Cpu or Gpu)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use litert_lm::{Engine, Backend};
    ///
    /// let engine = Engine::new("model.tflite", Backend::Cpu)?;
    /// # Ok::<(), litert_lm::Error>(())
    /// ```
    pub fn new(model_path: &str, backend: Backend) -> Result<Self> {
        let model_path_cstr = CString::new(model_path)
            .map_err(|e| Error::new(format!("Invalid model path: {}", e)))?;

        let backend_cstr = CString::new(backend.as_str())
            .map_err(|e| Error::new(format!("Invalid backend string: {}", e)))?;

        unsafe {
            // Create engine settings
            let settings = litert_lm_engine_settings_create(
                model_path_cstr.as_ptr(),
                backend_cstr.as_ptr(),
                std::ptr::null(), // vision_backend_str
                std::ptr::null(), // audio_backend_str
            );

            if settings.is_null() {
                return Err(Error::new("Failed to create engine settings"));
            }

            // Create engine
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

    /// Create a new session for text generation
    ///
    /// Sessions maintain conversation history and state.
    /// You can create multiple sessions from the same engine.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use litert_lm::{Engine, Backend};
    ///
    /// let engine = Engine::new("model.tflite", Backend::Cpu)?;
    /// let session = engine.create_session()?;
    /// # Ok::<(), litert_lm::Error>(())
    /// ```
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
// Session
// ============================================================================

/// LiteRT-LM Session - represents a conversation context
///
/// A session maintains the conversation history and can generate
/// text responses to prompts.
pub struct Session {
    raw: *mut LiteRtLmSession,
}

// Safety: Sessions can be moved between threads but not shared
unsafe impl Send for Session {}

impl Session {
    /// Generate text from a prompt
    ///
    /// # Arguments
    ///
    /// * `prompt` - The input text prompt
    ///
    /// # Returns
    ///
    /// The generated text response
    ///
    /// # Example
    ///
    /// ```no_run
    /// use litert_lm::{Engine, Backend};
    ///
    /// let engine = Engine::new("model.tflite", Backend::Cpu)?;
    /// let session = engine.create_session()?;
    /// let response = session.generate("What is 2+2?")?;
    /// println!("Response: {}", response);
    /// # Ok::<(), litert_lm::Error>(())
    /// ```
    pub fn generate(&self, prompt: &str) -> Result<String> {
        let prompt_cstr = CString::new(prompt)
            .map_err(|e| Error::new(format!("Invalid prompt: {}", e)))?;

        unsafe {
            // Create InputData for text
            let input_data = InputData {
                type_: InputDataType_kInputText,
                data: prompt_cstr.as_ptr() as *const std::ffi::c_void,
                size: prompt.len(),
            };

            // Generate content
            let responses = litert_lm_session_generate_content(self.raw, &input_data, 1);

            if responses.is_null() {
                return Err(Error::new("Failed to generate content"));
            }

            // Get response text
            let text_ptr = litert_lm_responses_get_response_text_at(responses, 0);

            let result = if !text_ptr.is_null() {
                CStr::from_ptr(text_ptr).to_string_lossy().into_owned()
            } else {
                litert_lm_responses_delete(responses);
                return Err(Error::new("No response generated"));
            };

            // Clean up responses
            litert_lm_responses_delete(responses);

            Ok(result)
        }
    }

    /// Get benchmark information (if benchmarking is enabled)
    ///
    /// Returns information about performance metrics like tokens per second.
    pub fn get_benchmark_info(&self) -> Result<BenchmarkInfo> {
        unsafe {
            let info = litert_lm_session_get_benchmark_info(self.raw);

            if info.is_null() {
                return Err(Error::new("Failed to get benchmark info"));
            }

            let time_to_first_token =
                litert_lm_benchmark_info_get_time_to_first_token(info);
            let num_prefill_turns = litert_lm_benchmark_info_get_num_prefill_turns(info);
            let num_decode_turns = litert_lm_benchmark_info_get_num_decode_turns(info);

            let result = BenchmarkInfo {
                time_to_first_token,
                num_prefill_turns: num_prefill_turns as usize,
                num_decode_turns: num_decode_turns as usize,
            };

            litert_lm_benchmark_info_delete(info);

            Ok(result)
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            litert_lm_session_delete(self.raw);
        }
    }
}


// ============================================================================
// Conversation
// ============================================================================

/// A multi-turn conversation context that applies the model's chat template.
///
/// Unlike [`Session`], which sends raw text, `Conversation` wraps prompts in
/// the model's Jinja chat template and correctly handles thinking channels,
/// stop tokens, and tool calls. Use this for all chat-style interactions.
pub struct Conversation {
    raw: *mut LiteRtLmConversation,
}

unsafe impl Send for Conversation {}

impl Conversation {
    /// Create a conversation from an engine with default config.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use litert_lm::{Engine, Backend, Conversation};
    ///
    /// let engine = Engine::new("model.litertlm", Backend::Gpu)?;
    /// let mut convo = Conversation::new(&engine)?;
    /// let reply = convo.send("What is 2 + 2?")?;
    /// println!("{reply}");
    /// # Ok::<(), litert_lm::Error>(())
    /// ```
    pub fn new(engine: &Engine) -> Result<Self> {
        unsafe {
            // Pass NULL config so the C API calls
            // ConversationConfig::CreateDefault(*engine) which properly
            // initializes from the engine's metadata.
            let raw = litert_lm_conversation_create(engine.raw, std::ptr::null_mut());

            if raw.is_null() {
                return Err(Error::new("Failed to create conversation"));
            }
            Ok(Conversation { raw })
        }
    }

    /// Send a plain-text message and get the model's response.
    ///
    /// The message is automatically wrapped in the model's chat template.
    /// Returns the model's text response with thinking content stripped.
    pub fn send(&mut self, message: &str) -> Result<String> {
        // The C API expects a JSON object: {"role": "user", "content": "..."}.
        let content_escaped = serde_json_mini_encode(message);
        let message_json = format!(
            r#"{{"role":"user","content":{content_escaped}}}"#
        );
        let message_cstr = CString::new(message_json)
            .map_err(|e| Error::new(format!("Invalid message: {}", e)))?;

        unsafe {
            let response = litert_lm_conversation_send_message(
                self.raw,
                message_cstr.as_ptr(),
                std::ptr::null(), // no extra context
            );

            if response.is_null() {
                return Err(Error::new("Failed to send message"));
            }

            let json_ptr = litert_lm_json_response_get_string(response);
            let result = if !json_ptr.is_null() {
                let raw = CStr::from_ptr(json_ptr).to_string_lossy();
                // Response is JSON: {"role":"assistant","content":[{"type":"text","text":"..."}]}
                // Extract the text field from the first content item.
                extract_response_text(&raw)
            } else {
                litert_lm_json_response_delete(response);
                return Err(Error::new("No response from conversation"));
            };

            litert_lm_json_response_delete(response);
            Ok(result)
        }
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        unsafe {
            litert_lm_conversation_delete(self.raw);
        }
    }
}

/// Minimal JSON string encoding (no serde dependency needed).
/// Wraps `s` in double quotes, escaping \ " and control characters.
fn serde_json_mini_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Extract the assistant's text from a conversation JSON response.
/// Expected format: {"role":"assistant","content":[{"type":"text","text":"..."}]}
/// Falls back to returning the raw string if parsing fails.
fn extract_response_text(json: &str) -> String {
    // Minimal JSON extraction without pulling in serde_json:
    // Find the last "text":" and extract its value.
    if let Some(pos) = json.rfind("\"text\":\"") {
        let start = pos + "\"text\":\"".len();
        let rest = &json[start..];
        // Find the closing quote, handling escaped quotes.
        let mut end = 0;
        let bytes = rest.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'\\' {
                end += 2; // skip escaped char
            } else if bytes[end] == b'"' {
                break;
            } else {
                end += 1;
            }
        }
        let escaped = &rest[..end];
        // Unescape basic JSON sequences.
        return escaped
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    // Fallback: return as-is.
    json.to_string()
}
// ============================================================================
// Benchmark Info
// ============================================================================

/// Benchmark information for a session
#[derive(Debug, Clone)]
pub struct BenchmarkInfo {
    /// Time to first token in seconds
    pub time_to_first_token: f64,
    /// Number of prefill turns
    pub num_prefill_turns: usize,
    /// Number of decode turns
    pub num_decode_turns: usize,
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
        assert_eq!(format!("{}", err), "LiteRT-LM Error: test error");
    }
}
