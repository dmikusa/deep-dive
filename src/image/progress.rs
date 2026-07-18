use tokio::sync::mpsc::Sender;

/// Progress update emitted while a resolver is fetching an image.
#[derive(Debug, Clone)]
pub enum Progress {
    /// A human-readable status message (e.g., "Pulling manifest...").
    Status(String),
    /// A determinate or indeterminate byte-count update.
    Bytes {
        /// Bytes received so far.
        current: u64,
        /// Total bytes expected, if known.
        total: Option<u64>,
    },
}

/// Convenience alias for the channel used to report progress.
pub type ProgressSender = Sender<Progress>;

/// Helper to send a status update, ignoring a closed channel.
pub async fn status(progress: &ProgressSender, message: impl Into<String>) {
    let _ = progress.send(Progress::Status(message.into())).await;
}

/// Helper to send a byte-count update, ignoring a closed channel.
pub async fn bytes(progress: &ProgressSender, current: u64, total: Option<u64>) {
    let _ = progress.send(Progress::Bytes { current, total }).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_status_sends_message() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        status(&tx, "pulling").await;
        assert!(matches!(rx.recv().await, Some(Progress::Status(s)) if s == "pulling"));
    }

    #[tokio::test]
    async fn test_bytes_sends_update() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        bytes(&tx, 42, Some(100)).await;
        assert!(matches!(
            rx.recv().await,
            Some(Progress::Bytes {
                current: 42,
                total: Some(100)
            })
        ));
    }

    #[tokio::test]
    async fn test_status_ignores_closed_channel() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        drop(_rx);
        status(&tx, "closed").await;
    }
}
