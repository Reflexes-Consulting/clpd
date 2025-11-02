use crate::crypto::compress;
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;

/// Type of clipboard content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClipboardContentType {
    Text,
    Image,
}

/// Image metadata and data for clipboard storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>, // RGBA bytes
}

impl ImageData {
    pub fn new(width: usize, height: usize, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            bytes,
        }
    }
}

/// A clipboard entry stored in the database
/// The payload field contains: nonce || encrypted data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub content_type: ClipboardContentType,
    pub payload: Vec<u8>, // encrypted: nonce || ciphertext
    pub hash: String,     // SHA-256 hash of plaintext for deduplication
}

impl ClipboardEntry {
    pub fn new(content_type: ClipboardContentType, payload: Vec<u8>, hash: String) -> Self {
        let timestamp = Utc::now();
        let id = format!("{}-{}", timestamp.timestamp_millis(), rand::random::<u32>());

        Self {
            id,
            timestamp,
            content_type,
            payload,
            hash,
        }
    }

    /// Get a preview of the entry for display (just metadata, no decryption)
    pub fn preview(&self) -> String {
        format!(
            "[{}] {} - {:?}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.id,
            self.content_type
        )
    }

    pub fn to_compressed_string(&self) -> String {
        let serialized = bincode::serialize(self).expect("Failed to serialize entry");
        let serialized = compress(&serialized);
        general_purpose::STANDARD.encode(&serialized)
    }

    pub fn from_compressed_string(s: &str) -> Result<Self, Box<dyn Error>> {
        let decoded = general_purpose::STANDARD.decode(s)?;
        let decompressed = crate::crypto::decompress(&decoded)?;
        let entry: ClipboardEntry = bincode::deserialize(&decompressed)?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_creation() {
        let entry = ClipboardEntry::new(
            ClipboardContentType::Text,
            vec![1, 2, 3, 4],
            "abc123".to_string(),
        );

        assert!(entry.id.contains("-"));
        assert_eq!(entry.content_type, ClipboardContentType::Text);
        assert_eq!(entry.payload, vec![1, 2, 3, 4]);
        assert_eq!(entry.hash, "abc123");
    }
}
