use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParsingError {
    #[error("Either the file is unreadable or it doesn't exist: {0}")]
    InvalidFile(String),

    #[error("Failed to read file")]
    UnreadableFile(#[from] io::Error),

    #[error("Failed to parse archive: {0}")]
    ArchiveError(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Failed to parse document: {0}")]
    ParsingError(String),
}
