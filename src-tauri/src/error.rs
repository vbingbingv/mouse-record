use serde::Serialize;

#[derive(Debug, Clone, thiserror::Error)]
pub enum AutomationError {
    #[error("already recording")]
    AlreadyRecording,

    #[error("not recording")]
    NotRecording,

    #[error("already replaying")]
    AlreadyReplaying,

    #[error("not replaying")]
    NotReplaying,

    #[error("input backend error: {0}")]
    InputBackend(String),

    #[error("invalid recording: {0}")]
    InvalidRecording(String),

    #[error("invalid replay options: {0}")]
    InvalidOptions(String),

    #[error("invalid recording file: {0}")]
    InvalidFile(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("input listener unavailable: {0}")]
    ListenerUnavailable(String),
}

impl From<std::io::Error> for AutomationError {
    fn from(e: std::io::Error) -> Self {
        AutomationError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AutomationError {
    fn from(e: serde_json::Error) -> Self {
        AutomationError::InvalidFile(e.to_string())
    }
}

impl Serialize for AutomationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_plain_string() {
        let json = serde_json::to_string(&AutomationError::AlreadyRecording).unwrap();
        assert_eq!(json, "\"already recording\"");
    }
}
