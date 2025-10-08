# clpd - Encrypted Clipboard History Manager

[![Build and Release](https://github.com/Reflexes-Consulting/clpd/actions/workflows/build.yml/badge.svg)](https://github.com/Reflexes-Consulting/clpd/actions/workflows/build.yml)

A secure, local clipboard history manager written in Rust that encrypts all clipboard entries with a password-derived key.

## 🔐 Security Features

- **Strong Encryption**: XChaCha20-Poly1305 authenticated encryption
- **Key Derivation**: Argon2id password hashing with random salt
- **Zero Knowledge**: Master password never stored on disk
- **Memory Safety**: Automatic zeroization of sensitive data
- **Deduplication**: SHA-256 hashing prevents duplicate entries
- **Local Only**: No network access, all data stored locally

## 📦 Installation

### Option 1: Download Pre-built Binary (Recommended)

Download the latest release for your platform from the [Releases page](https://github.com/Reflexes-Consulting/clpd/releases):

- **Windows**: `clpd-windows-x86_64.exe`
- **Linux**: `clpd-linux-x86_64`

Make the binary executable (Linux):

```bash
chmod +x clpd-linux-x86_64
```

### Option 2: Build from Source

```bash
cargo build --release
```

The binary will be available at `target/release/clpd`.

You can optionally install it system-wide:

```bash
cargo install --path .
```

## 🚀 Quick Start

### 1. Initialize the Database

First, initialize the encrypted database with a master password:

```bash
# Windows
.\clpd.exe init

# Linux/macOS (if installed)
clpd init
```

You'll be prompted to enter and confirm a master password. This password will be used to encrypt all clipboard entries.

⚠️ **Important**: Choose a strong password and remember it! There is no password recovery mechanism.

### 2. Start the Watcher

Start monitoring your clipboard:

```bash
# Windows
.\clpd.exe start

# Linux/macOS
clpd start
```

The daemon will:

- Monitor your clipboard for changes every 500ms
- Automatically encrypt and store new entries
- Deduplicate entries using content hashing
- Continue running until you press Ctrl+C

Optional: Limit the maximum number of stored entries:

```bash
# Windows
.\clpd.exe start --max-entries 1000

# Linux/macOS
clpd start --max-entries 1000
```

### 3. List Clipboard History

View your clipboard history:

```bash
clpd list
```

For more details:

```bash
clpd list --verbose
```

Limit the number of entries shown:

```bash
clpd list --limit 10
```

### 4. Show Entry Content

Decrypt and display a specific entry:

```bash
clpd show <entry-id>
```

### 5. Copy Entry to Clipboard

Restore an entry to your clipboard:

```bash
clpd copy <entry-id>
```

### 6. Delete Entries

Delete a specific entry:

```bash
clpd delete <entry-id>
```

Clear all entries:

```bash
clpd clear
```

Use `-y` or `--yes` to skip confirmation prompts.

### 7. View Statistics

Show database statistics:

```bash
clpd stats
```

### 8. Dump All Entries

Export all clipboard entries to a directory:

```bash
# Windows
.\clpd.exe dump <directory-path>

# Linux/macOS
clpd dump <directory-path>
```

This will:

- Create a CSV file (`clipboard_text_entries.csv`) containing all text entries with ID, timestamp, and content
- Save all images as PNG files with timestamped filenames (e.g., `image_20251008_143052_12345678.png`)
- Prompt for your master password to decrypt all entries

Example:

```bash
# Export to a folder called "clipboard_export"
.\clpd.exe dump clipboard_export

# Skip confirmation if directory exists
.\clpd.exe dump clipboard_export --yes
```

**Note**: This creates an unencrypted backup of your clipboard history. Store the exported directory securely!

## 📁 Database Location

By default, the database is stored at:

- **Windows**: `%LOCALAPPDATA%\clpd\db`
- **Linux/macOS**: `~/.local/share/clpd/db`

You can specify a custom database path with the `--database` flag:

```bash
clpd --database /path/to/custom/db list
```

## 🔧 Technical Details

### Encryption

- **Algorithm**: XChaCha20-Poly1305 (AEAD cipher)
- **Key Size**: 256 bits
- **Nonce**: Random 192-bit nonce per entry
- **Authentication**: Poly1305 MAC tag

### Key Derivation

- **Algorithm**: Argon2id (default parameters)
- **Salt**: Random 128-bit salt (generated during init)
- **Output**: 256-bit key

### Storage

- **Database**: Sled (embedded key-value store)
- **Format**: Two trees:
  - `meta`: Stores salt and payload
  - `clips`: Stores encrypted clipboard entries

### Entry Format

Each entry contains:

- Unique ID (timestamp + random suffix)
- Timestamp (UTC)
- Content type (Text or Image)
- Encrypted payload (nonce || ciphertext)
- SHA-256 hash (for deduplication)

## 🛡️ Security Considerations

### ✅ Good Security Practices

- Password never stored on disk
- All entries encrypted with authenticated encryption
- Random nonces prevent pattern analysis
- Key material zeroized from memory
- Local-only operation (no network)

### ⚠️ Important Notes

- **Database file is encrypted**, but anyone with filesystem access can see metadata (timestamps, entry count)
- **Master password cannot be recovered** - if you forget it, you lose access to all entries
- **System clipboard is not secured** - other applications can read the clipboard when you copy entries back
- The watcher daemon runs in the foreground - for background operation, use your OS's process management tools

## 🔍 Example Session

```bash
# Initialize (Windows - use .\clpd.exe, Linux/macOS - use clpd)
$ .\clpd.exe init
🔐 Initializing encrypted clipboard database

Enter master password: ****
Confirm master password: ****

⏳ Deriving encryption key...
✓ Database initialized successfully!

💡 Use 'clpd start' to begin watching your clipboard.

# Start watcher (in one terminal)
$ .\clpd.exe start
✓ Password verified

🔒 Clipboard watcher started. Press Ctrl+C to stop.
📋 Monitoring clipboard for changes...
✓ Stored encrypted entry #1
✓ Stored encrypted entry #2
✓ Stored encrypted entry #3

# List entries (in another terminal)
$ .\clpd.exe list
📋 Clipboard History (3 entries, showing 3)

[2025-10-08 14:23:45] 1728394425123-1234567890 - Text
[2025-10-08 14:23:30] 1728394410456-9876543210 - Text
[2025-10-08 14:23:15] 1728394395789-5555555555 - Text

# Show specific entry
$ .\clpd.exe show 1728394425123-1234567890
Enter master password: ****
📋 Entry: 1728394425123-1234567890
⏰ Timestamp: 2025-10-08 14:23:45 UTC
📝 Type: Text

Content:
─────────────────────────────────────
This is my clipboard content!
─────────────────────────────────────

# Copy back to clipboard
$ .\clpd.exe copy 1728394425123-1234567890
Enter master password: ****
✓ Text copied to clipboard

# View stats
$ .\clpd.exe stats
📊 Database Statistics

Total entries: 3
  - Text: 3
  - Images: 0

Total encrypted size: 384 bytes (0.38 KB)
Average size per entry: 128.00 bytes

Oldest entry: 2025-10-08 14:23:15
Newest entry: 2025-10-08 14:23:45
```

## 🚧 Limitations and Future Work

### Current Limitations

- Watcher runs in foreground only
- No GUI or TUI (CLI only)
- No search/filter functionality
- No sync between machines
- Export is unencrypted (dump command creates plaintext files)

### Potential Enhancements

- ✨ TUI with ratatui for better browsing
- ✨ Search and filter by content, date, or type
- ✨ Encrypted export/import for backups
- ✨ Auto-lock after inactivity
- ✨ Configurable Argon2 parameters
- ✨ Background daemon mode
- ✨ Favorite/pin entries
- ✨ Tags and categories

### ✅ Recently Implemented

- ✅ Full image clipboard support (capture and restore)
- ✅ Export functionality via `dump` command

## � Development

### Building

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Continuous Integration

The project uses GitHub Actions for CI/CD:

- **Automated builds** for Windows and Linux
- **Automated testing** on both platforms
- **Code quality checks** with clippy and rustfmt
- **Release automation** - creates GitHub releases with binaries when tags are pushed

To create a new release:

```bash
git tag v0.2.0
git push origin v0.2.0
```

Binaries and checksums will be automatically built and attached to the GitHub release.

## �📝 License

This project is provided as-is for educational and personal use, and uses the BSD 2 Clause license.

## 🤝 Contributing

This is a learning project. Feel free to fork and modify as needed!

## ⚖️ Disclaimer

This software is provided "as is" without warranty of any kind. Use at your own risk. The authors are not responsible for any data loss or security issues.

**Remember**: Good security starts with a strong, unique master password!
