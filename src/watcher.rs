use anyhow::{Context, Result};
use arboard::Clipboard;
use sha2::{Digest, Sha256};
use std::thread;
use std::time::Duration;

use crate::crypto::{MasterKey, encrypt};
use crate::database::ClipboardDatabase;
use crate::models::{ClipboardContentType, ClipboardEntry, ImageData};

pub struct LocalClipboardWatcher {
    clipboard: Clipboard,
    pub db: ClipboardDatabase,
    key: MasterKey,
    last_hash: Option<String>,
    max_entries: Option<usize>,
    poll_interval: Duration,
}

impl LocalClipboardWatcher {
    pub fn new(db: ClipboardDatabase, key: MasterKey, max_entries: Option<usize>) -> Result<Self> {
        let clipboard = Clipboard::new().context("Failed to initialize clipboard")?;

        Ok(Self {
            clipboard,
            db,
            key,
            last_hash: None,
            max_entries,
            poll_interval: Duration::from_millis(500),
        })
    }

    /// Calculate SHA-256 hash of data
    pub(crate) fn hash_data(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Process text clipboard content
    pub(crate) fn process_text(&mut self, text: &str) -> Result<bool> {
        let data = text.as_bytes();
        let hash = Self::hash_data(data);

        // Check if this is a duplicate
        if self.last_hash.as_ref() == Some(&hash) {
            return Ok(false);
        }

        // Check if this hash already exists in the database
        if self.db.hash_exists(&hash)? {
            self.last_hash = Some(hash);
            return Ok(false);
        }

        // Encrypt and store
        let encrypted = encrypt(&self.key, data).context("Failed to encrypt clipboard data")?;

        let entry = ClipboardEntry::new(ClipboardContentType::Text, encrypted, hash.clone());

        self.db
            .insert_entry(&entry)
            .context("Failed to insert entry")?;

        self.last_hash = Some(hash);

        // Prune if necessary
        if let Some(max) = self.max_entries {
            self.db.prune_to_limit(max)?;
        }

        Ok(true)
    }

    /// Process image clipboard content
    pub(crate) fn process_image(&mut self, image_data: &arboard::ImageData) -> Result<bool> {
        // Store image metadata along with RGBA bytes
        let img_data = ImageData::new(
            image_data.width,
            image_data.height,
            image_data.bytes.to_vec(),
        );

        // Serialize the image data structure
        let serialized = bincode::serialize(&img_data).context("Failed to serialize image data")?;

        let hash = Self::hash_data(&serialized);

        // Check if this is a duplicate
        if self.last_hash.as_ref() == Some(&hash) {
            return Ok(false);
        }

        // Check if this hash already exists in the database
        if self.db.hash_exists(&hash)? {
            self.last_hash = Some(hash);
            return Ok(false);
        }

        // Encrypt and store
        let encrypted =
            encrypt(&self.key, &serialized).context("Failed to encrypt clipboard image")?;

        let entry = ClipboardEntry::new(ClipboardContentType::Image, encrypted, hash.clone());

        self.db
            .insert_entry(&entry)
            .context("Failed to insert entry")?;

        self.last_hash = Some(hash);

        // Prune if necessary
        if let Some(max) = self.max_entries {
            self.db.prune_to_limit(max)?;
        }

        Ok(true)
    }

    /// Check clipboard once
    pub fn check_clipboard(&mut self) -> Result<bool> {
        // Try to get text first
        if let Ok(text) = self.clipboard.get_text()
            && !text.is_empty()
        {
            return self.process_text(&text);
        }

        // Try to get image if no text
        if let Ok(image) = self.clipboard.get_image() {
            return self.process_image(&image);
        }

        Ok(false)
    }

    /// Start watching the clipboard in a loop
    pub fn watch(mut self) -> Result<()> {
        println!("🔒 Clipboard watcher started. Press Ctrl+C to stop.");
        println!("📋 Monitoring clipboard for changes...");

        let mut stored_count = 0;

        loop {
            match self.check_clipboard() {
                Ok(true) => {
                    stored_count += 1;
                    println!("✓ Stored encrypted entry #{}", stored_count);
                }
                Ok(false) => {
                    // No change or duplicate, continue silently
                }
                Err(e) => {
                    eprintln!("⚠ Warning: Failed to process clipboard: {}", e);
                }
            }

            thread::sleep(self.poll_interval);
        }
    }
}

pub fn start_watcher(
    db: ClipboardDatabase,
    key: MasterKey,
    max_entries: Option<usize>,
) -> Result<()> {
    let watcher = LocalClipboardWatcher::new(db, key, max_entries)?;
    watcher.watch()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_data() {
        let data = b"test data";
        let hash = LocalClipboardWatcher::hash_data(data);

        // SHA-256 produces 64 hex characters
        assert_eq!(hash.len(), 64);

        // Same data produces same hash
        let hash2 = LocalClipboardWatcher::hash_data(data);
        assert_eq!(hash, hash2);

        // Different data produces different hash
        let hash3 = LocalClipboardWatcher::hash_data(b"different data");
        assert_ne!(hash, hash3);
    }
}
