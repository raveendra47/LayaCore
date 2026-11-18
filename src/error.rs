use std::fmt;

#[derive(Debug)]
pub enum AnalyzerError {
    IoError(std::io::Error),
    DecodeError(String),
    UnsupportedFormat(String),
    InvalidData(String),
}

impl fmt::Display for AnalyzerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalyzerError::IoError(e) => write!(f, "I/O error: {}", e),
            AnalyzerError::DecodeError(msg) => write!(f, "Decoding error: {}", msg),
            AnalyzerError::UnsupportedFormat(fmt) => write!(f, "Unsupported format: {}", fmt),
            AnalyzerError::InvalidData(msg) => write!(f, "Invalid audio data: {}", msg),
        }
    }
}

impl std::error::Error for AnalyzerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AnalyzerError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AnalyzerError {
    fn from(err: std::io::Error) -> Self {
        AnalyzerError::IoError(err)
    }
}
