#![allow(dead_code)]

use std::time::Duration;

use tokio::task::JoinHandle;

/// A simple stderr spinner that prints while async work is in progress.
pub struct Loader {
    handle: Option<JoinHandle<()>>,
}

impl Loader {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let handle = tokio::spawn(async move {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut i = 0;
            loop {
                eprint!("\r\x1B[2K{} {}", frames[i % frames.len()], message);
                i += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        Self {
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear the current line.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            eprint!("\r\x1B[2K");
        }
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.stop();
    }
}
