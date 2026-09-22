use thiserror::Error;

#[derive(Debug, Error)]
pub enum JurlError {
    #[error("{0}")]
    Usage(String),
    #[error("could not interpret: {0}")]
    Unresolved(String),
    #[error("conflicting words: {0}")]
    Conflict(String),
    #[error("jev: {0}")]
    Jev(String),
    #[error("interpretation rejected (confidence {0:.2} < {1:.2}); rerun with --explain to see why")]
    LowConfidence(f32, f32),
    #[error("aborted")]
    Aborted,
    #[error("curl not found in PATH")]
    CurlMissing,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config: {0}")]
    Config(String),
}

impl JurlError {
    pub fn exit_code(&self) -> i32 {
        match self {
            JurlError::CurlMissing => 127,
            JurlError::Usage(_) | JurlError::Config(_) => 64,
            _ => 2,
        }
    }
}
