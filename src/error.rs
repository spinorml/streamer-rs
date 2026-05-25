/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use crate::codec::Codec;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StreamError {
    #[error("backend error: {0}")]
    Backend(Box<dyn std::error::Error + Send + Sync>),
    #[error("codec not supported: {0:?}")]
    UnsupportedCodec(Codec),
    #[error("pipeline error: {message}")]
    Pipeline { message: String },
    #[error("end of stream")]
    EndOfStream,
}

pub type Result<T> = std::result::Result<T, StreamError>;
