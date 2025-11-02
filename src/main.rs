mod cli;
mod crypto;
mod database;
mod middleware;
mod models;
mod tui;
mod watcher;
use anyhow::{Context, Result};
use arboard::Clipboard;
use mimalloc::MiMalloc;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio::runtime;

use cli::{Commands, parse_args};
use crypto::{decrypt, derive_key, encrypt, generate_salt};
use database::ClipboardDatabase;
use models::{ClipboardContentType, ImageData};
use watcher::start_watcher;

use crate::crypto::MasterKey;
use crate::database::{ClipboardType, NetworkClipboardDatabase};
use crate::watcher::LocalClipboardWatcher;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();

    // Handle install command separately (doesn't need database)
    if matches!(args.command, Commands::Install) {
        return cmd_install();
    }

    if matches!(args.command, Commands::NetStart { max_entries }) {
        return cmd_net_start(None).await;
    }

    if matches!(args.command, Commands::NetBrowse) {
        // let clipboard_db = ClipboardType::Network(NetworkClipboardDatabase);
        return cmd_net_browse(None).await;
    }

    // Get database path
    let db_path = match args.database {
        Some(path) => path,
        None => ClipboardDatabase::default_path()?,
    };

    // Open database
    let db = ClipboardDatabase::open(db_path)?;

    // Handle commands
    match args.command {
        Commands::Init => cmd_init(db)?,
        Commands::NetListen => cmd_net_listen(db).await?,
        // Commands::NetStart { max_entries } => cmd_net_start(max_entries).await?,
        Commands::Start { max_entries } => cmd_start(db, max_entries)?,
        Commands::List { verbose, limit } => cmd_list(db, verbose, limit)?,
        Commands::Show { id } => cmd_show(db, &id)?,
        Commands::Copy { id } => cmd_copy(db, &id)?,
        Commands::Delete { id, yes } => cmd_delete(db, &id, yes)?,
        Commands::Clear { yes } => cmd_clear(db, yes)?,
        Commands::Stats => cmd_stats(db)?,
        Commands::Dump { directory, yes } => cmd_dump(db, directory, yes)?,
        Commands::Browse => {
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
            let db = LocalClipboardWatcher::new(db, key.clone(), None)?;
            let db = ClipboardType::Local(db);
            cmd_browse(db, key).await?
        }
        Commands::Install => unreachable!(), // Handled above
        Commands::NetStart { max_entries } => unreachable!(), // Handled above
        Commands::NetBrowse => unreachable!(), // Handled above
    };
    // Clean up by deleting any temporary files if needed
    let temp_dir = std::env::temp_dir().join("clpd_temp");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).context("Failed to clean up temporary files")?;
    };
    Ok(())
}

async fn cmd_net_listen(db: ClipboardDatabase) -> Result<()> {
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

    // Start server and remain running
    database::run_clipboard_server(db).await;
    Ok(())
}

async fn cmd_net_browse(max_entries: Option<usize>) -> Result<()> {
    // Get password
    let password = rpassword::prompt_password("Enter master password: ")?;

    // Get salt and derive key
    // let salt = db.get_salt()?;

    let temp_client = reqwest::Client::new();
    let salt_resp = temp_client
        .get("http://localhost:2573/clipboard/salt")
        .send()
        .await?;
    let salt = salt_resp.text().await?;
    let salt = salt.as_bytes();

    let key = derive_key(&password, &salt)?;

    let network_clip = NetworkClipboardDatabase::new(&key, max_entries)?;
    let network_clip = ClipboardType::Network(network_clip);

    println!("✓ Password verified");
    println!();
    cmd_browse(network_clip, key).await?;
    Ok(())
}

async fn cmd_net_start(max_entries: Option<usize>) -> Result<()> {
    // Get password
    let password = rpassword::prompt_password("Enter master password: ")?;

    // Get salt and derive key
    // let salt = db.get_salt()?;

    let temp_client = reqwest::Client::new();
    let salt_resp = temp_client
        .get("http://localhost:2573/clipboard/salt")
        .send()
        .await?;
    let salt = salt_resp.text().await?;
    let salt = salt.as_bytes();

    let key = derive_key(&password, &salt)?;

    let mut network_clip = NetworkClipboardDatabase::new(&key, max_entries)?;

    println!("✓ Password verified");
    println!();

    // Start watcher
    network_clip.watch().await
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

/// Dump all entries to a directory
fn cmd_dump(db: ClipboardDatabase, directory: PathBuf, yes: bool) -> Result<()> {
    // Check if initialized
    if !db.is_initialized()? {
        anyhow::bail!("Database not initialized. Run 'clipd init' first.");
    }

    let entries = db.list_entries()?;

    if entries.is_empty() {
        println!("No entries to dump.");
        return Ok(());
    }

    // Create directory if it doesn't exist
    if directory.exists() {
        if !yes {
            print!(
                "⚠ Directory '{}' already exists. Files may be overwritten. Continue? (y/N): ",
                directory.display()
            );
            io::stdout().flush()?;

            let mut response = String::new();
            io::stdin().read_line(&mut response)?;

            if !response.trim().eq_ignore_ascii_case("y") {
                println!("Dump cancelled.");
                return Ok(());
            }
        }
    } else {
        fs::create_dir_all(&directory).context("Failed to create output directory")?;
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
    println!(
        "📁 Dumping {} entries to '{}'",
        entries.len(),
        directory.display()
    );
    println!();

    // Create CSV file for text entries
    let csv_path = directory.join("clipboard_text_entries.csv");
    let mut csv_writer = csv::Writer::from_path(&csv_path).context("Failed to create CSV file")?;

    // Write CSV header
    csv_writer.write_record(["ID", "Timestamp", "Content"])?;

    let mut text_count = 0;
    let mut image_count = 0;
    let mut errors = 0;

    // Process each entry
    for entry in entries.iter() {
        // Decrypt entry
        let plaintext = match decrypt(&key, &entry.payload) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("⚠ Failed to decrypt entry {}: {}", entry.id, e);
                errors += 1;
                continue;
            }
        };

        match entry.content_type {
            ClipboardContentType::Text => {
                // Write to CSV
                let text = String::from_utf8_lossy(&plaintext).to_string();
                csv_writer.write_record([
                    &entry.id,
                    &entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                    &text,
                ])?;
                text_count += 1;
                print!(".");
                io::stdout().flush()?;
            }
            ClipboardContentType::Image => {
                // Deserialize image data
                match bincode::deserialize::<ImageData>(&plaintext) {
                    Ok(img_data) => {
                        // Save as PNG
                        let image_filename = format!(
                            "image_{}_{}.png",
                            entry.timestamp.format("%Y%m%d_%H%M%S"),
                            &entry.id[entry.id.len().saturating_sub(8)..]
                        );
                        let image_path = directory.join(&image_filename);

                        // Convert RGBA to PNG using image crate
                        match image::RgbaImage::from_raw(
                            img_data.width as u32,
                            img_data.height as u32,
                            img_data.bytes,
                        ) {
                            Some(img) => {
                                if let Err(e) = img.save(&image_path) {
                                    eprintln!("\n⚠ Failed to save image {}: {}", image_filename, e);
                                    errors += 1;
                                } else {
                                    image_count += 1;
                                    print!(".");
                                    io::stdout().flush()?;
                                }
                            }
                            None => {
                                eprintln!(
                                    "\n⚠ Failed to create image from data for entry {}",
                                    entry.id
                                );
                                errors += 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "\n⚠ Failed to deserialize image data for entry {}: {}",
                            entry.id, e
                        );
                        errors += 1;
                    }
                }
            }
        }
    }

    csv_writer.flush()?;
    println!();
    println!();
    println!("✓ Dump complete!");
    println!();
    println!("📊 Summary:");
    println!(
        "  - Text entries: {} (saved to {})",
        text_count,
        csv_path.display()
    );
    println!("  - Images: {} (saved as PNG files)", image_count);

    if errors > 0 {
        println!("  ⚠ Errors: {}", errors);
    }

    Ok(())
}

/// Browse clipboard history with interactive TUI
async fn cmd_browse(db: ClipboardType, key: MasterKey) -> Result<()> {
    // Check if initialized
    // if !db.is_initialized().await? {
    //     anyhow::bail!("Database not initialized. Run 'clpd init' first.");
    // }

    // // Get password
    // let password = rpassword::prompt_password("Enter master password: ")?;

    // // Get salt and derive key
    // let salt = db.get_salt().await?;
    // let key = derive_key(&password, &salt)?;

    // // Verify password
    // if !db.verify_password(&key).await? {
    //     anyhow::bail!("❌ Incorrect password!");
    // }

    // Run TUI
    tui::run(db, key).await?;

    Ok(())
}

/// Install clpd binary to default location and add to PATH
fn cmd_install() -> Result<()> {
    println!("🔧 Installing clpd...");
    println!();

    // Get the current executable path
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    println!("📍 Current executable: {}", current_exe.display());

    // Get the default database directory
    let install_dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to determine local data directory"))?
        .join("clpd");

    // Create install directory if it doesn't exist
    fs::create_dir_all(&install_dir).context("Failed to create installation directory")?;

    // Target path for the binary
    let binary_name = if cfg!(windows) { "clpd.exe" } else { "clpd" };
    let target_path = install_dir.join(binary_name);

    println!("📂 Install directory: {}", install_dir.display());
    println!();

    // Copy the binary
    if target_path.exists() {
        print!(
            "⚠️  clpd is already installed at {}. Overwrite? (y/N): ",
            target_path.display()
        );
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Installation cancelled.");
            return Ok(());
        }
    }

    fs::copy(&current_exe, &target_path)
        .context("Failed to copy binary to installation directory")?;

    println!("✓ Binary copied to: {}", target_path.display());
    println!();

    // Add to PATH
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        println!("🔧 Adding to Windows PATH...");
        println!();

        let install_dir_str = install_dir.to_string_lossy().to_string();

        // Check if already in PATH
        let already_in_path = if let Ok(path_var) = std::env::var("PATH") {
            path_var.split(';').any(|p| p == install_dir_str.as_str())
        } else {
            false
        };

        if already_in_path {
            println!("✓ Directory already in PATH");
        } else {
            // Check if running as administrator
            let is_admin = Command::new("net")
                .args(&["session"])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);

            if is_admin {
                println!("🔓 Running as Administrator - adding to PATH automatically...");

                // Get current user PATH
                let output = Command::new("powershell")
                    .args(&[
                        "-NoProfile",
                        "-Command",
                        "[Environment]::GetEnvironmentVariable('Path', 'User')",
                    ])
                    .output()
                    .context("Failed to get current PATH")?;

                let current_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

                // Add install directory to PATH if not empty
                let new_path = if current_path.is_empty() {
                    install_dir_str.clone()
                } else {
                    format!("{};{}", current_path, install_dir_str)
                };

                // Set the new PATH
                let status = Command::new("powershell")
                    .args(&[
                        "-NoProfile",
                        "-Command",
                        &format!(
                            "[Environment]::SetEnvironmentVariable('Path', '{}', 'User')",
                            new_path.replace("'", "''")
                        ),
                    ])
                    .status()
                    .context("Failed to set PATH")?;

                if status.success() {
                    println!("✓ Successfully added to PATH!");
                    println!();
                    println!(
                        "⚠️  You may need to restart your terminal for the changes to take effect."
                    );
                } else {
                    anyhow::bail!("Failed to update PATH environment variable");
                }
            } else {
                println!("⚠️  Not running as Administrator!");
                println!();
                println!("To automatically add clpd to your PATH, please run:");
                println!();
                println!("  clpd install");
                println!();
                println!("in an Administrator PowerShell/Command Prompt.");
                println!();
                println!("Or manually run this command in PowerShell (as Administrator):");
                println!();
                println!(
                    "  [Environment]::SetEnvironmentVariable('Path', $env:Path + ';{}', [EnvironmentVariableTarget]::User)",
                    install_dir_str
                );
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("🔧 Adding to PATH...");
        println!();

        let install_dir_str = install_dir.to_string_lossy();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        let rc_file = if shell.contains("zsh") {
            "~/.zshrc"
        } else if shell.contains("fish") {
            "~/.config/fish/config.fish"
        } else {
            "~/.bashrc"
        };

        println!("Add this line to your {}:", rc_file);
        println!();
        println!("  export PATH=\"$PATH:{}\"", install_dir_str);
        println!();
        println!("Then run: source {}", rc_file);
    }

    println!();
    println!("✨ Installation complete!");
    println!("   Run 'clpd init' to set up your encrypted clipboard database.");

    Ok(())
}
