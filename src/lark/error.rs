#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Lark(#[from] open_lark::core::error::LarkAPIError),

    #[error("missing environment variable {name}: {reason}")]
    MissingEnvVar { name: &'static str, reason: String },

    #[error("response data missing for {context}")]
    MissingResponseData { context: &'static str },

    #[error("invalid response shape for {context}: expected {expected}")]
    InvalidResponseShape {
        context: &'static str,
        expected: &'static str,
    },

    #[error("cache type mismatch for key `{key}`; expected {expected}")]
    CacheTypeMismatch { key: String, expected: &'static str },
}
