mod cli;
mod crypto;
mod database;
mod models;
mod watcher;

use anyhow::{Context, Result};
use arboard::Clipboard;
use std::io::{self, Write};

use cli::{Commands, parse_args};
use crypto::{decrypt, derive_key, encrypt, generate_salt};
use database::ClipboardDatabase;
use models::{ClipboardContentType, ImageData};
use watcher::start_watcher;

fn main() -> Result<()> {
    let args = parse_args();

    // Get database path
    let db_path = match args.database {
        Some(path) => path,
        None => ClipboardDatabase::default_path()?,
    };

    // Open database
    let db = ClipboardDatabase::open(db_path)?;

    // Handle commands
    match args.command {
        Commands::Init => cmd_init(db),
        Commands::Start { max_entries } => cmd_start(db, max_entries),
        Commands::List { verbose, limit } => cmd_list(db, verbose, limit),
        Commands::Show { id } => cmd_show(db, &id),
        Commands::Copy { id } => cmd_copy(db, &id),
        Commands::Delete { id, yes } => cmd_delete(db, &id, yes),
        Commands::Clear { yes } => cmd_clear(db, yes),
        Commands::Stats => cmd_stats(db),
    }
}

/// Initialize the database
fn cmd_init(db: ClipboardDatabase) -> Result<()> {
    // Check if already initialized
    if db.is_initialized()? {
        println!("⚠ Database is already initialized.");
        print!(
            "Do you want to reinitialize? This will NOT delete existing entries but will change the password. (y/N): "
        );
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Initialization cancelled.");
            return Ok(());
        }
    }

    println!("🔐 Initializing encrypted clipboard database");
    println!();

    // Get password from user
    let password = rpassword::prompt_password("Enter master password: ")?;
    let password_confirm = rpassword::prompt_password("Confirm master password: ")?;

    if password != password_confirm {
        anyhow::bail!("Passwords do not match!");
    }

    if password.len() < 8 {
        anyhow::bail!("Password must be at least 8 characters long");
    }

    // Generate salt
    let salt = generate_salt();

    // Derive key
    println!("\n⏳ Deriving encryption key...");
    let key = derive_key(&password, &salt)?;

    // Create payload
    let test_payload = encrypt(&key, b"clpd_test")?;

    // Store in database
    db.initialize(&salt, &test_payload)?;

    println!("✓ Database initialized successfully!");
    println!("\n💡 Use 'clpd start' to begin watching your clipboard.");

    Ok(())
}

/// Start the clipboard watcher
fn cmd_start(db: ClipboardDatabase, max_entries: Option<usize>) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    // Get password
    let password = rpassword::prompt_password("Enter master password: ")?;

    // Get salt and derive key
    let salt = db.get_salt()?;
    let key = derive_key(&password, &salt)?;

    // Verify password
    if !db.verify_password(&key)? {
        anyhow::bail!("❌ Incorrect password!");
    }

    println!("✓ Password verified");
    println!();

    if let Some(max) = max_entries {
        println!("📊 Maximum entries: {}", max);
    }

    // Start watcher
    start_watcher(db, key, max_entries)
}

/// List all entries
fn cmd_list(db: ClipboardDatabase, verbose: bool, limit: Option<usize>) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    let entries = db.list_entries()?;

    if entries.is_empty() {
        println!("No entries found. Start the watcher with 'clpd start'.");
        return Ok(());
    }

    let display_count = limit.unwrap_or(entries.len()).min(entries.len());

    println!(
        "📋 Clipboard History ({} entries, showing {})",
        entries.len(),
        display_count
    );
    println!();

    for entry in entries.iter().take(display_count) {
        if verbose {
            println!("ID: {}", entry.id);
            println!(
                "  Timestamp: {}",
                entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f %Z")
            );
            println!("  Type: {:?}", entry.content_type);
            println!("  Size: {} bytes (encrypted)", entry.payload.len());
            println!("  Hash: {}", entry.hash);
            println!();
        } else {
            println!("{}", entry.preview());
        }
    }

    if display_count < entries.len() {
        println!(
            "\n... and {} more entries. Use --limit to show more or --verbose for details.",
            entries.len() - display_count
        );
    }

    Ok(())
}

/// Show a specific entry
fn cmd_show(db: ClipboardDatabase, id: &str) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    // Get password
    let password = rpassword::prompt_password("Enter master password: ")?;

    // Get salt and derive key
    let salt = db.get_salt()?;
    let key = derive_key(&password, &salt)?;

    // Verify password
    if !db.verify_password(&key)? {
        anyhow::bail!("❌ Incorrect password!");
    }

    // Get entry
    let entry = db
        .get_entry(id)?
        .ok_or_else(|| anyhow::anyhow!("Entry '{}' not found", id))?;

    // Decrypt
    let plaintext = decrypt(&key, &entry.payload).context("Failed to decrypt entry")?;

    println!("📋 Entry: {}", entry.id);
    println!(
        "⏰ Timestamp: {}",
        entry.timestamp.format("%Y-%m-%d %H:%M:%S %Z")
    );
    println!("📝 Type: {:?}", entry.content_type);
    println!();

    match entry.content_type {
        ClipboardContentType::Text => {
            let text = String::from_utf8_lossy(&plaintext);
            println!("Content:");
            println!("─────────────────────────────────────");
            println!("{}", text);
            println!("─────────────────────────────────────");
        }
        ClipboardContentType::Image => {
            // Deserialize to show image dimensions
            match bincode::deserialize::<ImageData>(&plaintext) {
                Ok(img_data) => {
                    println!("Content: Image");
                    println!(
                        "  Dimensions: {} x {} pixels",
                        img_data.width, img_data.height
                    );
                    println!("  Size: {} bytes (raw RGBA)", img_data.bytes.len());
                    println!(
                        "💡 Use 'clpd copy {}' to copy this image to clipboard",
                        entry.id
                    );
                }
                Err(_) => {
                    println!("Content: Image data ({} bytes)", plaintext.len());
                    println!(
                        "💡 Use 'clpd copy {}' to copy this image to clipboard",
                        entry.id
                    );
                }
            }
        }
    }

    Ok(())
}

/// Copy an entry back to clipboard
fn cmd_copy(db: ClipboardDatabase, id: &str) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    // Get password
    let password = rpassword::prompt_password("Enter master password: ")?;

    // Get salt and derive key
    let salt = db.get_salt()?;
    let key = derive_key(&password, &salt)?;

    // Verify password
    if !db.verify_password(&key)? {
        anyhow::bail!("❌ Incorrect password!");
    }

    // Get entry
    let entry = db
        .get_entry(id)?
        .ok_or_else(|| anyhow::anyhow!("Entry '{}' not found", id))?;

    // Decrypt
    let plaintext = decrypt(&key, &entry.payload).context("Failed to decrypt entry")?;

    // Copy to clipboard
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;

    match entry.content_type {
        ClipboardContentType::Text => {
            let text = String::from_utf8(plaintext).context("Entry contains invalid UTF-8")?;
            clipboard
                .set_text(text)
                .context("Failed to set clipboard text")?;
            println!("✓ Text copied to clipboard");
        }
        ClipboardContentType::Image => {
            // Deserialize the ImageData structure
            let img_data: ImageData =
                bincode::deserialize(&plaintext).context("Failed to deserialize image data")?;

            // Create arboard ImageData from our stored data
            let arboard_img = arboard::ImageData {
                width: img_data.width,
                height: img_data.height,
                bytes: img_data.bytes.into(),
            };

            clipboard
                .set_image(arboard_img)
                .context("Failed to set clipboard image")?;

            println!(
                "✓ Image copied to clipboard ({} x {} pixels)",
                img_data.width, img_data.height
            );
        }
    }

    Ok(())
}

/// Delete an entry
fn cmd_delete(db: ClipboardDatabase, id: &str, yes: bool) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    // Confirm deletion
    if !yes {
        print!("⚠ Delete entry '{}'? (y/N): ", id);
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Deletion cancelled.");
            return Ok(());
        }
    }

    // Delete
    if db.delete_entry(id)? {
        println!("✓ Entry '{}' deleted", id);
    } else {
        println!("⚠ Entry '{}' not found", id);
    }

    Ok(())
}

/// Clear all entries
fn cmd_clear(db: ClipboardDatabase, yes: bool) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    let count = db.count_entries();

    if count == 0 {
        println!("Database is already empty.");
        return Ok(());
    }

    // Confirm clearing
    if !yes {
        print!(
            "⚠ Delete all {} entries? This cannot be undone! (y/N): ",
            count
        );
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Clear cancelled.");
            return Ok(());
        }
    }

    // Get all entries and delete them
    let entries = db.list_entries()?;
    let mut deleted = 0;

    for entry in entries {
        if db.delete_entry(&entry.id)? {
            deleted += 1;
        }
    }

    println!("✓ Deleted {} entries", deleted);

    Ok(())
}

/// Show database statistics
fn cmd_stats(db: ClipboardDatabase) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    }

    let entries = db.list_entries()?;
    let total_count = entries.len();

    if total_count == 0 {
        println!("📊 Database Statistics");
        println!();
        println!("Total entries: 0");
        println!("💡 Start the watcher with 'clpd start' to begin collecting clipboard history.");
        return Ok(());
    }

    let text_count = entries
        .iter()
        .filter(|e| e.content_type == ClipboardContentType::Text)
        .count();
    let image_count = entries
        .iter()
        .filter(|e| e.content_type == ClipboardContentType::Image)
        .count();

    let total_size: usize = entries.iter().map(|e| e.payload.len()).sum();

    let oldest = entries.last().unwrap();
    let newest = entries.first().unwrap();

    println!("📊 Database Statistics");
    println!();
    println!("Total entries: {}", total_count);
    println!("  - Text: {}", text_count);
    println!("  - Images: {}", image_count);
    println!();
    println!(
        "Total encrypted size: {} bytes ({:.2} KB)",
        total_size,
        total_size as f64 / 1024.0
    );
    println!(
        "Average size per entry: {:.2} bytes",
        total_size as f64 / total_count as f64
    );
    println!();
    println!(
        "Oldest entry: {}",
        oldest.timestamp.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "Newest entry: {}",
        newest.timestamp.format("%Y-%m-%d %H:%M:%S")
    );

    Ok(())
}
