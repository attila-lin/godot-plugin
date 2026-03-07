#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Lark(#[from] open_lark::core::error::LarkAPIError),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("missing environment variable {name}: {reason}")]
    MissingEnvVar { name: &'static str, reason: String },

    #[error("response data missing for {context}")]
    MissingResponseData { context: &'static str },

    #[error("invalid response shape for {context}: expected {expected}")]
    InvalidResponseShape {
        context: &'static str,
        expected: &'static str,
    },

    #[error("Feishu API error during {context}: code={code}, msg={msg}")]
    FeishuApi {
        context: &'static str,
        code: i64,
        msg: String,
    },

    #[error("invalid spec: {context}")]
    InvalidSpec { context: String },

    #[error("table `{table}` not found in target bitable app")]
    TableNotFound { table: String },

    #[error("record in table `{table}` is missing key field `{key_field}`")]
    MissingRecordKey { table: String, key_field: String },

    #[error("cache type mismatch for key `{key}`; expected {expected}")]
    CacheTypeMismatch { key: String, expected: &'static str },
}
