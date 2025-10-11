# clpd Usage Guide

## Getting Started

### First Time Setup

1. **Initialize the database:**

   ```bash
   .\clpd.exe init
   ```

   - You'll be prompted for a master password (min 8 characters)
   - Choose a strong, memorable password
   - **IMPORTANT**: There is no password recovery!

2. **Start watching your clipboard:**
   ```bash
   .\clpd.exe start
   ```
   - The daemon will run in the foreground
   - Press `Ctrl+C` to stop
   - All clipboard changes are automatically encrypted and stored

## Command Reference

### `.\clpd.exe init`

Initialize or reinitialize the database with a master password.

**Example:**

```bash
.\clpd.exe init
```

---

### `.\clpd.exe browse`

Launch the interactive Terminal User Interface (TUI) for browsing clipboard history.

**Features:**

- Split-pane interface with entry list and live preview
- Image preview using colored block characters
- Real-time decryption and display
- Keyboard-driven navigation

**Controls:**

| Key                | Action                            |
| ------------------ | --------------------------------- |
| `↑`/`↓` or `j`/`k` | Navigate up/down through entries  |
| `Enter` or `c`     | Copy selected entry to clipboard  |
| `o`                | Open entry in default application |
| `d`                | Delete selected entry             |
| `r`                | Refresh entry list                |
| `Home`/`End`       | Jump to first/last entry          |
| `PgUp`/`PgDn`      | Jump 10 entries                   |
| `q` or `Esc`       | Quit TUI                          |

**Example:**

```bash
.\clpd.exe browse
```

**Notes:**

- You'll be prompted for your master password on launch
- Images are displayed using colored blocks (requires RGB terminal support)
- Status messages appear at the bottom for 3 seconds
- The "Open" feature creates temporary files in your system temp directory

---

### `.\clpd.exe start [OPTIONS]`

Start the clipboard watcher daemon.

**Options:**

- `--max-entries <N>` - Limit maximum stored entries (oldest are pruned)

**Examples:**

```bash
# Unlimited storage
.\clpd.exe start

# Limit to 1000 entries
.\clpd.exe start --max-entries 1000
```

---

### `.\clpd.exe list [OPTIONS]`

List all stored clipboard entries.

**Options:**

- `-v, --verbose` - Show full details for each entry
- `-n, --limit <N>` - Show only N most recent entries

**Examples:**

```bash
# Basic list
.\clpd.exe list

# Verbose output
.\clpd.exe list --verbose

# Show only last 20 entries
.\clpd.exe list --limit 20
```

---

### `.\clpd.exe show <ENTRY_ID>`

Decrypt and display a specific entry.

**Example:**

```bash
.\clpd.exe show 1728394425123-1234567890
```

---

### `.\clpd.exe copy <ENTRY_ID>`

Copy a stored entry back to your clipboard.

**Example:**

```bash
.\clpd.exe copy 1728394425123-1234567890
```

---

### `.\clpd.exe delete <ENTRY_ID> [OPTIONS]`

Delete a specific entry from the database.

**Options:**

- `-y, --yes` - Skip confirmation prompt

**Examples:**

```bash
# With confirmation
.\clpd.exe delete 1728394425123-1234567890

# Skip confirmation
.\clpd.exe delete 1728394425123-1234567890 --yes
```

---

### `.\clpd.exe clear [OPTIONS]`

Delete all entries from the database.

**Options:**

- `-y, --yes` - Skip confirmation prompt

**Examples:**

```bash
# With confirmation
.\clpd.exe clear

# Skip confirmation
.\clpd.exe clear --yes
```

---

### `.\clpd.exe stats`

Display database statistics (entry counts, sizes, date range).

**Example:**

```bash
.\clpd.exe stats
```

---

## Global Options

### `--database <PATH>`

Use a custom database location (default: `%LOCALAPPDATA%\clpd\db`).

**Example:**

```bash
.\clpd.exe --database C:\my-custom-path\db list
```

---

## Tips and Tricks

### Interactive Browsing

The TUI is the easiest way to browse your clipboard history:

```bash
.\clpd.exe browse
```

**Workflow:**

1. Launch TUI with `browse` command
2. Use `j`/`k` or arrow keys to navigate
3. Preview appears automatically on the right
4. Press `Enter` to copy, `o` to open, or `d` to delete
5. Press `q` to exit

**Image Preview:**

- Images are rendered using colored block characters
- Requires a terminal with RGB color support (Windows Terminal, iTerm2, Alacritty, etc.)
- Large images are automatically downsampled to fit the preview pane
- Half-block characters (▀) provide 2x vertical resolution

### Running in Background (Windows)

Since the watcher runs in the foreground, you can use Windows Task Scheduler or run it in a separate terminal window.

**Using PowerShell Job:**

```powershell
Start-Job -ScriptBlock { .\clpd.exe start }
```

**View job output:**

```powershell
Get-Job | Receive-Job
```

### Workflow Example

1. **Morning Setup:**

   ```bash
   .\clpd.exe start --max-entries 500
   ```

   Leave this terminal running all day.

2. **During the Day:**

   - Copy text, links, code snippets, images normally
   - Everything is automatically saved and encrypted

3. **Browsing Your History:**

   **Option A - Interactive TUI (Recommended):**

   ```bash
   .\clpd.exe browse
   ```

   Navigate with keyboard, preview in real-time, copy or open with a keypress.

   **Option B - Command Line:**

   ```bash
   # See recent items
   .\clpd.exe list --limit 10

   # Get specific entry ID, then:
   .\clpd.exe show <entry-id>

   # Or copy it back:
   .\clpd.exe copy <entry-id>
   ```

4. **End of Day:**
   - Press `Ctrl+C` in the watcher terminal
   - Or keep it running overnight

### Database Maintenance

**View stats periodically:**

```bash
.\clpd.exe stats
```

**Clean up old entries:**

```bash
# List and manually delete old ones
.\clpd.exe list --verbose
.\clpd.exe delete <old-entry-id> --yes

# Or clear everything
.\clpd.exe clear
```

**Change password:**

```bash
# Reinitialize (entries remain, but use new password)
.\clpd.exe init
```

---

## Troubleshooting

### "Database not initialized"

Run `.\clpd.exe init` first.

### "Incorrect password"

The password you entered doesn't match the one used during initialization. Try again or reinitialize (you'll lose access to old entries).

### "Failed to access clipboard"

- Make sure no other application is blocking clipboard access
- Try running as administrator (usually not needed)
- Check if clipboard service is running on Windows

### Entries not being saved

- Ensure the watcher is running (`.\clpd.exe start`)
- Check terminal for error messages
- Verify disk space is available

### Can't find entry ID

- Use `.\clpd.exe list` to see all entry IDs
- Use `--verbose` flag for more details
- Entry IDs are the long number-hyphen-number strings

---

## Security Best Practices

1. ✅ Use a unique, strong master password
2. ✅ Keep your database file secure (it's encrypted)
3. ✅ Don't share your master password
4. ✅ Be aware that active clipboard content is readable by other apps
5. ✅ Consider clearing sensitive entries after use
6. ⚠️ Remember: No password = no recovery!

---

## Performance Notes

- **Polling interval**: 500ms (checks clipboard twice per second)
- **Storage**: Efficient key-value store (sled)
- **Memory**: Low overhead, keys zeroized after use
- **Deduplication**: Identical content not stored twice
- **CPU**: Minimal (only hashes on clipboard change)

---

## Need Help?

- Check `.\clpd.exe --help` for command overview
- Check `.\clpd.exe <command> --help` for specific command help
- Review README.md for architecture and technical details
