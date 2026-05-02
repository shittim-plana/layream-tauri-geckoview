use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayreamError {
    #[error("RPack decode failed")]
    RpackDecode,

    #[error("decompression failed: {0}")]
    Decompress(#[from] std::io::Error),

    #[error("msgpack decode failed: {0}")]
    MsgpackDecode(#[from] rmp_serde::decode::Error),

    #[error("msgpack encode failed: {0}")]
    MsgpackEncode(#[from] rmp_serde::encode::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("AES-GCM decryption failed")]
    AesDecrypt,

    #[error("AES-GCM encryption failed")]
    AesEncrypt,

    #[error("unknown preset format")]
    UnknownPresetFormat,

    #[error("unsupported file format: {0}")]
    UnsupportedFileFormat(String),

    #[error("invalid preset version: {0}")]
    InvalidPresetVersion(u32),

    #[error("invalid preset type: {0}")]
    InvalidPresetType(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("OAuth error: {0}")]
    OAuthError(String),

    #[error("invalid_grant: refresh token expired or revoked")]
    InvalidGrant,

    #[error("API error {status}: {body}")]
    ApiError { status: u16, body: String },
}
