use crate::crypto::encrypt;
use crate::crypto::{MasterKey, decrypt, derive_key};
use crate::watcher::LocalClipboardWatcher;
// use crate::database::ClipboardDatabase;
use crate::models::ClipboardEntry;
use crate::models::{ClipboardContentType, ImageData};
use actix_cors::Cors;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use reqwest::ClientBuilder;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use sha2::{Digest, Sha256};
use sled::{Db, Tree};
// use std::default;
use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, Scope, get, middleware, post, web,
};
use arboard::Clipboard;
use std::path::PathBuf;
use std::sync::Arc;

const META_TREE: &str = "meta";
const CLIPS_TREE: &str = "clips";
const SALT_KEY: &[u8] = b"meta:salt";
const VERSION_KEY: &[u8] = b"meta:version";
const PAYLOAD_KEY: &[u8] = b"meta:payload";

pub struct ClipboardDatabase {
    pub db: Db,
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
        // while `sled` prefers big endian when needing ordering, here we just need a fixed
        // representation, so little endian is fine
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

pub enum ClipboardType {
    Local(LocalClipboardWatcher),
    Network(NetworkClipboardDatabase),
}

impl ClipboardType {
    pub fn hash_data(self, data: &[u8]) -> String {
        match self {
            ClipboardType::Local(_) => LocalClipboardWatcher::hash_data(data),
            ClipboardType::Network(_) => NetworkClipboardDatabase::hash_data(data),
        }
    }

    pub async fn process_text(self, text: &str) -> Result<bool> {
        match self {
            ClipboardType::Local(mut db) => db.process_text(text),
            ClipboardType::Network(db) => db.process_text(text).await,
        }
    }

    pub async fn process_image(self, image_data: &arboard::ImageData<'_>) -> Result<bool> {
        match self {
            ClipboardType::Local(mut db) => db.process_image(image_data),
            ClipboardType::Network(db) => db.process_image(image_data).await,
        }
    }

    pub async fn check_clipboard(self) -> Result<bool> {
        match self {
            ClipboardType::Local(mut db) => db.check_clipboard(),
            ClipboardType::Network(mut db) => db.check_clipboard().await,
        }
    }

    pub async fn watch(self) -> Result<()> {
        match self {
            ClipboardType::Local(db) => db.watch(),
            ClipboardType::Network(mut db) => db.watch().await,
        }
    }

    pub async fn list_entries(&self) -> Result<Vec<ClipboardEntry>> {
        match self {
            ClipboardType::Local(db) => db.db.list_entries(),
            ClipboardType::Network(db) => db.list_entries().await,
        }
    }

    pub async fn delete_entry(&self, id: &str) -> Result<bool> {
        match self {
            ClipboardType::Local(db) => db.db.delete_entry(id),
            ClipboardType::Network(db) => db.delete_entry(id).await,
        }
    }

    pub async fn is_initialized(&self) -> Result<bool> {
        match self {
            ClipboardType::Local(db) => db.db.is_initialized(),
            ClipboardType::Network(db) => Ok(true), // Assume network DB is always initialized
        }
    }

    pub async fn get_salt(&self) -> Result<Vec<u8>> {
        match self {
            ClipboardType::Local(db) => db.db.get_salt(),
            ClipboardType::Network(db) => db.get_salt().await,
        }
    }

    pub async fn verify_password(&self, key: &MasterKey) -> Result<bool> {
        match self {
            ClipboardType::Local(db) => db.db.verify_password(key),
            ClipboardType::Network(_) => Ok(true), // Assume network DB password is always valid
        }
    }
}

pub struct NetworkClipboardDatabase {
    client: reqwest::Client,
    base_url: String,
    key: MasterKey,
    clipboard: Clipboard,
    max_entries: Option<usize>,
    poll_interval: std::time::Duration,
}

impl NetworkClipboardDatabase {
    /// Create a new NetworkClipboard with the given base URL
    pub fn new(key: &MasterKey, max_entries: Option<usize>) -> Result<Self> {
        // let mut default_headers = reqwest::header::HeaderMap::new();
        // default_headers.insert(
        //     AUTHORIZATION,
        //     HeaderValue::from_str(&format!("Bearer {}", String::from_utf8_lossy(&key.hash())))
        //         .unwrap(),
        // );
        let client = ClientBuilder::new()
            // .default_headers(default_headers)
            .build()
            .context("Failed to build HTTP client")?;
        let clipboard = Clipboard::new().context("Failed to initialize clipboard")?;
        let base_url = "http://localhost:2573/clipboard".to_string();
        Ok(Self {
            client,
            base_url,
            key: key.clone(),
            max_entries,
            clipboard,
            poll_interval: std::time::Duration::from_millis(500),
        })
    }

    pub async fn list_entries(&self) -> Result<Vec<ClipboardEntry>> {
        let url = format!("{}/list", self.base_url);
        let resp = self.client.get(&url).send().await?;
        // .context("Failed to send list entries request")?;

        if resp.status().is_success() {
            let body = resp.text().await?;
            // .context("Failed to read list entries response body")?;
            let entries: Vec<String> =
                bincode::deserialize(&base64::decode(&body).context("Failed to decode entries")?)
                    .context("Failed to deserialize entries")?;
            let mut entries_decoded = Vec::new();
            for entry_str in entries {
                let entry = ClipboardEntry::from_compressed_string(&entry_str).unwrap();
                entries_decoded.push(entry);
            }
            Ok(entries_decoded)
        } else {
            Err(anyhow::anyhow!(
                "List entries request failed with status {}",
                resp.status()
            ))
        }
    }

    pub async fn get_salt(&self) -> Result<Vec<u8>> {
        let url = format!("{}/salt", self.base_url);
        let resp = self.client.get(&url).send().await?;
        // .context("Failed to send get salt request")?;

        if resp.status().is_success() {
            let body = resp.bytes().await?;
            // .context("Failed to read get salt response body")?;
            Ok(body.to_vec())
        } else {
            Err(anyhow::anyhow!(
                "Get salt request failed with status {}",
                resp.status()
            ))
        }
    }

    pub async fn delete_entry(&self, id: &str) -> Result<bool> {
        let url = format!("{}/delete/{}", self.base_url, id);
        let resp = self.client.get(&url).send().await?;
        // .context("Failed to send delete entry request")?;

        if resp.status().is_success() {
            Ok(true)
        } else if resp.status().as_u16() == 404 {
            Ok(false)
        } else {
            Err(anyhow::anyhow!(
                "Delete entry request failed with status {}",
                resp.status()
            ))
        }
    }

    /// Calculate SHA-256 hash of data

    pub(crate) fn hash_data(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    async fn process_text(&self, text: &str) -> Result<bool> {
        let data = text.as_bytes();
        let hash = Self::hash_data(data);

        // Check if this hash already exists in the database
        let url = format!("{}/check_hash/{}", self.base_url, hash);
        let resp = self.client.get(&url).send().await?;
        // .expect("Failed to send hash check request");

        if resp.status().is_success() {
            let body = resp.text().await?;
            // .expect("Failed to read hash check response body")?;
            if body.trim() == "1" {
                return Ok(false);
            }
        } else {
            return Err(anyhow::anyhow!(
                "Hash check request failed with status {}",
                resp.status()
            ));
        }

        // Encrypt and store
        let encrypted = encrypt(&self.key, data).context("Failed to encrypt clipboard data")?;

        let entry = ClipboardEntry::new(ClipboardContentType::Text, encrypted, hash.clone());

        let url = format!("{}/insert", self.base_url);
        let resp = self
            .client
            .post(&url)
            .body(entry.to_compressed_string())
            .send()
            .await?;
        // .context("Failed to send insert request")?;

        if resp.status().is_success() {
            Ok(true)
        } else {
            Err(anyhow::anyhow!(
                "Insert request failed with status {}",
                resp.status()
            ))
        }
    }

    async fn process_image(&self, image_data: &arboard::ImageData<'_>) -> Result<bool> {
        // Store image metadata along with RGBA bytes
        let img_data = ImageData::new(
            image_data.width,
            image_data.height,
            image_data.bytes.to_vec(),
        );

        // Serialize the image data structure
        let serialized = bincode::serialize(&img_data).context("Failed to serialize image data")?;

        let hash = Self::hash_data(&serialized);

        // Check if this hash already exists in the database
        let url = format!("{}/check_hash/{}", self.base_url, hash);
        let resp = self.client.get(&url).send().await?;
        // .expect("Failed to send hash check request");

        if resp.status().is_success() {
            let body = resp.text().await?;
            // .expect("Failed to read hash check response body")?;
            if body.trim() == "1" {
                return Ok(false);
            }
        } else {
            return Err(anyhow::anyhow!(
                "Hash check request failed with status {}",
                resp.status()
            ));
        }

        // Encrypt and store
        let encrypted =
            encrypt(&self.key, &serialized).context("Failed to encrypt clipboard data")?;

        let entry = ClipboardEntry::new(ClipboardContentType::Image, encrypted, hash.clone());

        let url = format!("{}/insert", self.base_url);
        let resp = self
            .client
            .post(&url)
            .body(entry.to_compressed_string())
            .send()
            .await?;
        // .context("Failed to send insert request")?;

        if resp.status().is_success() {
            Ok(true)
        } else {
            Err(anyhow::anyhow!(
                "Insert request failed with status {}",
                resp.status()
            ))
        }
    }

    pub async fn check_clipboard(&mut self) -> Result<bool> {
        // Try to get text first
        // let clipboard = Clipboard::new().context("Failed to access clipboard")?;
        if let Ok(text) = self.clipboard.get_text()
            && !text.is_empty()
        {
            return self.process_text(&text).await;
        }

        // Try to get image if no text
        if let Ok(image) = self.clipboard.get_image() {
            return self.process_image(&image).await;
        }

        Ok(false)
    }

    pub async fn watch(&mut self) -> Result<()> {
        println!("🔒 Network clipboard watcher started. Press Ctrl+C to stop.");
        println!("📋 Monitoring clipboard for changes...");

        let mut stored_count = 0;

        loop {
            match self.check_clipboard().await {
                Ok(true) => {
                    stored_count += 1;
                    println!("✓ Stored encrypted entry #{}", stored_count);
                }
                Ok(false) => {
                    // No new data
                }
                Err(e) => {
                    eprintln!("⚠️ Error checking clipboard: {}", e);
                }
            }

            // Sleep for a short duration before checking again
            tokio::time::sleep(self.poll_interval).await;
        }
    }
}

pub type WebClipboardData = web::Data<Arc<RwLock<ClipboardDatabase>>>;

#[post("/insert")]
async fn create_entry(
    // req: HttpRequest,
    body: String,
    clipboard_data: WebClipboardData,
) -> impl Responder {
    // Handle the creation of a new clipboard entry
    let entry = ClipboardEntry::from_compressed_string(&body);
    match entry {
        Ok(entry) => {
            let db = clipboard_data.read();
            db.insert_entry(&entry).expect("failed to insert entry");
            HttpResponse::Created().finish()
        }
        Err(_) => HttpResponse::BadRequest().body("Invalid entry format"),
    }
}

#[get("/get/{id}")]
async fn get_entry(req: HttpRequest, clipboard_data: WebClipboardData) -> impl Responder {
    let id = req.match_info().get("id").unwrap();
    let db = clipboard_data.read();
    match db.get_entry(id) {
        Ok(entry) => match entry {
            Some(entry) => HttpResponse::Ok().body(entry.to_compressed_string()),
            None => HttpResponse::NotFound().body("Entry not found"),
        },
        Err(_) => HttpResponse::NotFound().body("Entry not found"),
    }
}

// #[get("/list")]

#[get("/delete/{id}")]
async fn delete_entry(req: HttpRequest, clipboard_data: WebClipboardData) -> impl Responder {
    let id = req.match_info().get("id").unwrap();
    let db = clipboard_data.read();
    match db.delete_entry(id) {
        Ok(deleted) => {
            if deleted {
                HttpResponse::Ok().body("Entry deleted")
            } else {
                HttpResponse::NotFound().body("Entry not found")
            }
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to delete entry"),
    }
}

#[get("/prune/{max}")]
async fn prune_entries(req: HttpRequest, clipboard_data: WebClipboardData) -> impl Responder {
    let max_str = req.match_info().get("max").unwrap();
    let max: usize = match max_str.parse() {
        Ok(m) => m,
        Err(_) => return HttpResponse::BadRequest().body("Invalid max value"),
    };
    let db = clipboard_data.read();
    match db.prune_to_limit(max) {
        Ok(deleted) => HttpResponse::Ok().body(format!("Deleted {} entries", deleted)),
        Err(_) => HttpResponse::InternalServerError().body("Failed to prune entries"),
    }
}

#[get("/check_hash/{hash}")]
async fn check_hash(req: HttpRequest, clipboard_data: WebClipboardData) -> impl Responder {
    let hash = req.match_info().get("hash").unwrap();
    let db = clipboard_data.read();
    match db.hash_exists(hash) {
        Ok(exists) => {
            if exists {
                HttpResponse::Ok().body("1")
            } else {
                HttpResponse::Ok().body("0")
            }
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to check hash"),
    }
}

#[get("/count")]
async fn count_entries(clipboard_data: WebClipboardData) -> impl Responder {
    let db = clipboard_data.read();
    HttpResponse::Ok().body(db.count_entries().to_string())
}

#[get("/salt")]
async fn get_salt(clipboard_data: WebClipboardData) -> impl Responder {
    let db = clipboard_data.read();
    match db.get_salt() {
        Ok(salt) => HttpResponse::Ok().body(salt),
        Err(_) => HttpResponse::InternalServerError().body("Failed to get salt"),
    }
}

#[get("/list")]
async fn list_entries(clipboard_data: WebClipboardData) -> impl Responder {
    let db = clipboard_data.read();
    match db.list_entries() {
        Ok(entries) => {
            // convert each entry to compressed string and return the vec
            let mut compressed_entries = Vec::new();
            for entry in entries {
                compressed_entries.push(entry.to_compressed_string());
            }
            HttpResponse::Ok().body(base64::encode(
                bincode::serialize(&compressed_entries).unwrap(),
            ))
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to list entries"),
    }
}

// #[get("/payload")]
// async fn get_payload(clipboard_data: WebClipboardData) -> impl Responder {
//     let db = clipboard_data.read();
//     let payload = match db.get_payload() {
//         Ok(p) => p,
//         Err(_) => return HttpResponse::InternalServerError().body("Failed to get payload"),
//     };
//     // Handle the retrieval of a clipboard entry payload
//     HttpResponse::Ok().body(payload)
// }

pub fn clipboard_scope() -> Scope {
    web::scope("/clipboard")
        .service(create_entry)
        .service(get_entry)
        .service(delete_entry)
        .service(prune_entries)
        .service(check_hash)
        .service(count_entries)
        .service(get_salt)
        .service(list_entries)
}

pub async fn run_clipboard_server(db: ClipboardDatabase) {
    // let db = ClipboardDatabase::open(db_path).unwrap();
    // let salt = db.get_salt().unwrap();
    // let key = derive_key(&password, &salt).unwrap();
    // if !db.verify_password(&key).unwrap() {
    //     panic!("Invalid password for clipboard database");
    // }
    let payload_size = 1024 * 1024 * 50; // 50 MB
    let db = Arc::new(RwLock::new(db));
    let db = web::Data::new(db);
    let server = HttpServer::new(move || {
        App::new()
            // .wrap(middleware::Compress::default())
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header(),
            )
            .app_data(web::PayloadConfig::new(payload_size))
            .app_data(db.clone())
            .service(clipboard_scope())
    })
    .bind(("127.0.0.1", 2573))
    .unwrap();
    server.run().await.unwrap();
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
