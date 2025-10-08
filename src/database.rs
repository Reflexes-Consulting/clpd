use anyhow::{Context, Result};
use sled::{Db, Tree};
use std::path::PathBuf;

use crate::crypto::{MasterKey, decrypt};
use crate::models::ClipboardEntry;

const META_TREE: &str = "meta";
const CLIPS_TREE: &str = "clips";
const SALT_KEY: &[u8] = b"meta:salt";
const VERSION_KEY: &[u8] = b"meta:version";
const PAYLOAD_KEY: &[u8] = b"meta:payload";

pub struct ClipboardDatabase {
    db: Db,
    meta_tree: Tree,
    clips_tree: Tree,
}

impl ClipboardDatabase {
    /// Open or create a database at the given path
    pub fn open(path: PathBuf) -> Result<Self> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create database directory")?;
        }

        let db = sled::open(&path).context("Failed to open database")?;

        let meta_tree = db
            .open_tree(META_TREE)
            .context("Failed to open meta tree")?;

        let clips_tree = db
            .open_tree(CLIPS_TREE)
            .context("Failed to open clips tree")?;

        Ok(Self {
            db,
            meta_tree,
            clips_tree,
        })
    }

    /// Get the default database path
    pub fn default_path() -> Result<PathBuf> {
        let mut path = dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine local data directory"))?;
        path.push("clpd");
        path.push("db");
        Ok(path)
    }

    /// Check if the database is initialized
    pub fn is_initialized(&self) -> Result<bool> {
        Ok(self.meta_tree.contains_key(SALT_KEY)?)
    }

    /// Initialize the database with a salt and payload
    pub fn initialize(&self, salt: &[u8], payload: &[u8]) -> Result<()> {
        self.meta_tree.insert(SALT_KEY, salt)?;
        self.meta_tree.insert(VERSION_KEY, &1u32.to_le_bytes())?;
        self.meta_tree.insert(PAYLOAD_KEY, payload)?;
        self.meta_tree.flush()?;
        Ok(())
    }

    /// Get the stored salt
    pub fn get_salt(&self) -> Result<Vec<u8>> {
        self.meta_tree
            .get(SALT_KEY)?
            .map(|ivec| ivec.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Database not initialized - run 'clpd init' first"))
    }

    /// Get the payload for password verification
    pub fn get_payload(&self) -> Result<Vec<u8>> {
        self.meta_tree
            .get(PAYLOAD_KEY)?
            .map(|ivec| ivec.to_vec())
            .ok_or_else(|| anyhow::anyhow!("payload not found"))
    }

    /// Verify the password by decrypting the payload
    pub fn verify_password(&self, key: &MasterKey) -> Result<bool> {
        let payload = self.get_payload()?;
        match decrypt(key, &payload) {
            Ok(plaintext) => Ok(plaintext == b"clpd_test"),
            Err(_) => Ok(false),
        }
    }

    /// Insert a clipboard entry
    pub fn insert_entry(&self, entry: &ClipboardEntry) -> Result<()> {
        let serialized = bincode::serialize(entry).context("Failed to serialize entry")?;

        self.clips_tree.insert(entry.id.as_bytes(), serialized)?;
        self.clips_tree.flush()?;
        Ok(())
    }

    /// Get an entry by ID
    pub fn get_entry(&self, id: &str) -> Result<Option<ClipboardEntry>> {
        match self.clips_tree.get(id.as_bytes())? {
            Some(data) => {
                let entry = bincode::deserialize(&data).context("Failed to deserialize entry")?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// List all entries (sorted by timestamp, newest first)
    pub fn list_entries(&self) -> Result<Vec<ClipboardEntry>> {
        let mut entries = Vec::new();

        for item in self.clips_tree.iter() {
            let (_, value) = item?;
            let entry: ClipboardEntry =
                bincode::deserialize(&value).context("Failed to deserialize entry")?;
            entries.push(entry);
        }

        // Sort by timestamp, newest first
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(entries)
    }

    /// Check if an entry with the given hash already exists
    pub fn hash_exists(&self, hash: &str) -> Result<bool> {
        for item in self.clips_tree.iter() {
            let (_, value) = item?;
            let entry: ClipboardEntry =
                bincode::deserialize(&value).context("Failed to deserialize entry")?;
            if entry.hash == hash {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Delete an entry by ID
    pub fn delete_entry(&self, id: &str) -> Result<bool> {
        let removed = self.clips_tree.remove(id.as_bytes())?;
        if removed.is_some() {
            self.clips_tree.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the total number of entries
    pub fn count_entries(&self) -> usize {
        self.clips_tree.len()
    }

    /// Delete the oldest entries to maintain a maximum count
    pub fn prune_to_limit(&self, max_entries: usize) -> Result<usize> {
        let entries = self.list_entries()?;

        if entries.len() <= max_entries {
            return Ok(0);
        }

        let mut deleted = 0;

        // Delete oldest entries (at the end of the sorted list)
        for entry in entries.iter().skip(max_entries) {
            if self.delete_entry(&entry.id)? {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Flush all pending writes
    #[allow(dead_code)]
    pub fn flush(&self) -> Result<()> {
        self.meta_tree.flush()?;
        self.clips_tree.flush()?;
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ClipboardDatabase::open(db_path).unwrap();
        assert!(!db.is_initialized().unwrap());
    }

    #[test]
    fn test_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ClipboardDatabase::open(db_path).unwrap();
        let salt = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let payload = vec![1, 2, 3];

        db.initialize(&salt, &payload).unwrap();
        assert!(db.is_initialized().unwrap());
        assert_eq!(db.get_salt().unwrap(), salt);
    }
}
