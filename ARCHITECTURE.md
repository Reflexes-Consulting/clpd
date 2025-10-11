# clpd - Project Structure

## Overview

`clpd` is a secure, encrypted clipboard history manager written in Rust. This document describes the architecture and organization of the codebase.

## Directory Structure

```
clpd/
├── Cargo.toml          # Project dependencies and metadata
├── Cargo.lock          # Locked dependency versions
├── README.md           # Main documentation
├── USAGE.md            # User guide and command reference
├── RELEASE.md          # Release creation guide
├── .gitignore          # Git ignore patterns
├── clpd.exe            # Compiled executable (Windows)
└── src/
    ├── main.rs         # Application entry point and CLI command handlers
    ├── cli.rs          # Command-line argument parsing (clap)
    ├── crypto.rs       # Cryptographic operations (encryption, key derivation)
    ├── database.rs     # Database operations (sled wrapper)
    ├── models.rs       # Data structures (ClipboardEntry, etc.)
    ├── tui.rs          # Terminal User Interface (ratatui)
    └── watcher.rs      # Clipboard monitoring daemon
```

## Module Descriptions

### `main.rs`

**Purpose**: Application entry point and command implementation

**Key Functions**:

- `main()` - Parse CLI args and route to command handlers
- `cmd_init()` - Initialize database with master password
- `cmd_start()` - Start clipboard watcher daemon
- `cmd_browse()` - Launch interactive TUI browser
- `cmd_list()` - List stored entries
- `cmd_show()` - Decrypt and display entry
- `cmd_copy()` - Copy entry back to clipboard
- `cmd_delete()` - Delete specific entry
- `cmd_clear()` - Delete all entries
- `cmd_stats()` - Show database statistics
- `cmd_dump()` - Export all entries to files

**Dependencies**: All other modules

---

### `cli.rs`

**Purpose**: Command-line interface definition

**Structures**:

- `Cli` - Main CLI struct with global options
- `Commands` - Enum of all available commands
  - `Init` - Initialize database
  - `Start` - Start watcher daemon
  - `Browse` - Launch TUI browser
  - `List` - List entries
  - `Show` - Display entry
  - `Copy` - Copy to clipboard
  - `Delete` - Remove entry
  - `Clear` - Remove all entries
  - `Stats` - Show statistics
  - `Dump` - Export entries

**Technology**: Uses `clap` with derive macros for argument parsing

---

### `tui.rs`

**Purpose**: Terminal User Interface for browsing clipboard history

**Structure**: `App`

**Key Components**:

- `App` - TUI application state
  - `entries`: Cached list of clipboard entries
  - `list_state`: Current selection in the list
  - `message`: Status message display
  - `message_time`: Timestamp for auto-clearing messages
  - `db`: Database connection
  - `key`: Master encryption key

**Key Methods**:

- `new()` - Create new TUI app instance
- `handle_key()` - Process keyboard input
- `next()/previous()` - Navigate entry list
- `delete_selected()` - Delete current entry
- `copy_selected()` - Copy entry to clipboard
- `open_selected()` - Open entry in external app
- `refresh()` - Reload entries from database
- `render_preview_text()` - Generate text preview
- `get_image_data()` - Extract image data for display

**Rendering Functions**:

- `ui()` - Main layout (list, preview, status, controls)
- `render_entry_list()` - Render scrollable entry list
- `render_preview()` - Render entry preview with images
- `render_status_bar()` - Show status messages
- `render_controls_bar()` - Display keyboard shortcuts
- `create_image_preview()` - Generate colored block image preview

**Features**:

- Split-pane interface (15% list, 85% preview)
- Live decryption and preview
- Image display using half-block characters (▀) with RGB colors
- Keyboard navigation (vim-style + arrow keys)
- Status messages with 3-second auto-clear
- Temporary file creation for opening entries

**Controls**:

- `↑`/`↓` or `j`/`k` - Navigate
- `Enter` or `c` - Copy to clipboard
- `o` - Open in default app
- `d` - Delete entry
- `r` - Refresh list
- `Home`/`End` - Jump to first/last
- `PgUp`/`PgDn` - Jump 10 entries
- `q` or `Esc` - Quit

**Technology**:

- Uses `ratatui` for TUI rendering
- Uses `crossterm` for terminal control
- Colored block image rendering with RGB support

---

### `crypto.rs`

**Purpose**: Cryptographic operations

**Key Components**:

- `MasterKey` - Secure wrapper for encryption key (zeroized on drop)
- `generate_salt()` - Generate random 16-byte salt
- `derive_key()` - Derive key from password using Argon2id
- `encrypt()` - Encrypt data with XChaCha20-Poly1305
- `decrypt()` - Decrypt data

**Algorithms**:

- **Key Derivation**: Argon2id (default params)
- **Encryption**: XChaCha20-Poly1305 (AEAD)
- **Nonce**: 192-bit random nonce per encryption

**Security Features**:

- Keys zeroized from memory on drop
- Random nonce for every encryption
- Authenticated encryption (AEAD)

---

### `database.rs`

**Purpose**: Database abstraction layer

**Structure**: `ClipboardDatabase`

**Key Methods**:

- `open()` - Open or create database
- `default_path()` - Get OS-specific default path
- `is_initialized()` - Check if database has been set up
- `initialize()` - Store salt and payload
- `get_salt()` - Retrieve stored salt
- `verify_password()` - Check if password is correct
- `insert_entry()` - Store encrypted entry
- `get_entry()` - Retrieve entry by ID
- `list_entries()` - Get all entries (sorted)
- `hash_exists()` - Check for duplicate content
- `delete_entry()` - Remove entry
- `prune_to_limit()` - Maintain maximum entry count

**Storage**:

- **Engine**: sled (embedded key-value store)
- **Trees**:
  - `meta` - Stores salt, version, payload
  - `clips` - Stores encrypted clipboard entries

---

### `models.rs`

**Purpose**: Data structures and serialization

**Structures**:

- `ClipboardContentType` - Enum: Text or Image
- `ClipboardEntry` - Main entry structure
  - `id`: Unique identifier (timestamp + random)
  - `timestamp`: When entry was captured
  - `content_type`: Text or Image
  - `payload`: Encrypted data (nonce || ciphertext)
  - `hash`: SHA-256 hash for deduplication
- `DatabaseMetadata` - Metadata stored in DB
  - `version`: Schema version
  - `salt`: Key derivation salt
  - `payload`: For password verification

**Serialization**: Uses `serde` + `bincode`

---

### `watcher.rs`

**Purpose**: Clipboard monitoring daemon

**Structure**: `ClipboardWatcher`

**Key Methods**:

- `new()` - Create watcher instance
- `check_clipboard()` - Check for clipboard changes (once)
- `watch()` - Main loop (calls check_clipboard repeatedly)
- `process_text()` - Handle text clipboard content
- `process_image()` - Handle image clipboard content
- `hash_data()` - Calculate SHA-256 hash

**Features**:

- Polls clipboard every 500ms
- Deduplicates using SHA-256 hash
- Encrypts before storing
- Optional entry limit (prunes oldest)

**Technology**: Uses `arboard` for cross-platform clipboard access

---

## Data Flow

### Initialization Flow

```
User runs: .\clpd.exe init
    ↓
main.rs::cmd_init()
    ↓
Prompt for password
    ↓
crypto::generate_salt() → random 16 bytes
    ↓
crypto::derive_key(password, salt) → MasterKey
    ↓
crypto::encrypt(key, "clpd_test") → payload
    ↓
database::initialize(salt, payload)
    ↓
Database ready!
```

### Watcher Flow

```
User runs: .\clpd.exe start
    ↓
main.rs::cmd_start()
    ↓
Load salt from database
    ↓
Derive key from password
    ↓
Verify password
    ↓
watcher::start_watcher(db, key, max_entries)
    ↓
Loop every 500ms:
    - Check clipboard
    - If changed:
        * Calculate hash
        * Check if duplicate
        * Encrypt content
        * Store in database
        * Prune if over limit
```

### Retrieval Flow

```
User runs: .\clpd.exe show <id>
    ↓
main.rs::cmd_show()
    ↓
Prompt for password
    ↓
Derive key from password
    ↓
Verify password
    ↓
database::get_entry(id)
    ↓
crypto::decrypt(key, entry.payload)
    ↓
Display plaintext to user
```

---

## Security Architecture

### Defense in Depth

1. **Password Never Stored**

   - Only salt is stored
   - Password required for every operation

2. **Strong Key Derivation**

   - Argon2id (memory-hard, GPU-resistant)
   - Random salt per database

3. **Authenticated Encryption**

   - XChaCha20-Poly1305 (AEAD cipher)
   - Detects tampering automatically

4. **Memory Safety**

   - Keys zeroized when dropped
   - Rust's memory safety prevents leaks

5. **Deduplication Privacy**
   - Hash stored, but hash is also encrypted metadata
   - Original content never stored unencrypted

### Threat Model

**Protected Against**:

- ✅ Filesystem access without password
- ✅ Database file theft (encrypted)
- ✅ Memory dumps (keys zeroized)
- ✅ Weak passwords (Argon2id resistance)
- ✅ Replay attacks (random nonces)
- ✅ Tampering (authenticated encryption)

**NOT Protected Against**:

- ❌ Keyloggers (password capture)
- ❌ Memory scraping during operation (keys in RAM)
- ❌ Clipboard sniffing by other apps
- ❌ Compromised OS or hypervisor
- ❌ Physical access + running process

---

## Dependencies

### Core Dependencies

| Crate              | Version | Purpose                  |
| ------------------ | ------- | ------------------------ |
| `sled`             | 0.34    | Embedded database        |
| `argon2`           | 0.5     | Key derivation           |
| `chacha20poly1305` | 0.10    | Encryption               |
| `arboard`          | 3.4     | Clipboard access         |
| `clap`             | 4.5     | CLI parsing              |
| `serde`            | 1.0     | Serialization            |
| `bincode`          | 1.3     | Binary encoding          |
| `chrono`           | 0.4     | Timestamps               |
| `anyhow`           | 1.0     | Error handling           |
| `zeroize`          | 1.8     | Secure memory wiping     |
| `rand`             | 0.8     | Random number generation |
| `sha2`             | 0.10    | Hashing (deduplication)  |
| `rpassword`        | 7.3     | Secure password input    |
| `hex`              | 0.4     | Hex encoding             |
| `dirs`             | 5.0     | OS directory paths       |
| `csv`              | 1.3     | CSV export               |
| `image`            | 0.25    | Image processing         |
| `ratatui`          | 0.29    | Terminal UI framework    |
| `crossterm`        | 0.28    | Terminal manipulation    |
| `termimage`        | 1.2     | Terminal image display   |

### Dev Dependencies

| Crate      | Version | Purpose |
| ---------- | ------- | ------- |
| `tempfile` | 3.8     | Testing |

---

## Testing

Each module includes unit tests:

- `crypto.rs`: Encryption/decryption round-trip, wrong password detection, nonce uniqueness
- `database.rs`: Database creation, initialization, CRUD operations
- `models.rs`: Entry creation and serialization
- `watcher.rs`: Hash calculation consistency

**Run tests:**

```bash
cargo test
```

**Run tests with output:**

```bash
cargo test -- --nocapture
```

---

## Build Profiles

### Debug Build

```bash
cargo build
```

- Fast compilation
- Includes debug symbols
- No optimizations
- Binary: `target/debug/clpd.exe`

### Release Build

```bash
cargo build --release
```

- Optimized for performance
- Smaller binary size
- No debug symbols
- Binary: `target/release/clpd.exe`

---

## Extension Points

### Adding New Commands

1. Add variant to `Commands` enum in `cli.rs`
2. Add match arm in `main.rs::main()`
3. Implement `cmd_<name>()` function in `main.rs`

### Adding New Content Types

1. Add variant to `ClipboardContentType` in `models.rs`
2. Update `watcher.rs` to detect and process new type
3. Update `main.rs::cmd_show()` to display new type
4. Update `main.rs::cmd_copy()` to copy new type

### Improving Encryption

All encryption logic is isolated in `crypto.rs`. To change algorithms:

1. Update imports and algorithm constants
2. Modify `encrypt()` and `decrypt()` functions
3. Update tests
4. Consider migration path for existing databases

---

## Performance Characteristics

- **Startup**: O(1) - Opens database, no full scan
- **Insert**: O(1) - Direct key-value insert
- **List**: O(n) - Scans all entries, sorts in memory
- **Get**: O(log n) - B-tree lookup in sled
- **Delete**: O(log n) - B-tree delete
- **Prune**: O(n) - Must list all to find oldest

**Memory Usage**:

- Base: ~10-20 MB (Rust binary + sled)
- Per entry: ~100-500 bytes (metadata + encrypted content)
- List operation: Loads all entries into memory

**Disk Usage**:

- Database grows with entries
- sled uses log-structured storage
- Periodically compacted automatically

---

## Future Enhancements

See README.md for detailed list of planned features.

### ✅ Recently Implemented

- ✅ Full image support (capture, display, restore)
- ✅ Interactive TUI with ratatui
- ✅ Image preview using colored block characters
- ✅ Export functionality via `dump` command
- ✅ Open entries in external applications
- ✅ Half-block character rendering for 2x vertical resolution

### 🚧 Potential Future Enhancements

- Search and filtering within TUI
- Configurable TUI color themes
- Clipboard format detection improvements
- Background daemon mode
- And more...

---

## License and Disclaimer

Provided as-is for educational and personal use. See README.md for full disclaimer.
