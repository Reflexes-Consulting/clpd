use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io;
use std::time::Instant;

use crate::crypto::{MasterKey, decrypt};
use crate::database::ClipboardDatabase;
use crate::models::{ClipboardContentType, ClipboardEntry, ImageData};

/// TUI Application State
pub struct App {
    entries: Vec<ClipboardEntry>,
    list_state: ListState,
    should_quit: bool,
    db: ClipboardDatabase,
    key: MasterKey,
    message: Option<String>,
    message_time: Option<Instant>,
}

impl App {
    pub fn new(db: ClipboardDatabase, key: MasterKey) -> Result<Self> {
        let entries = db.list_entries()?;
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            entries,
            list_state,
            should_quit: false,
            db,
            key,
            message: None,
            message_time: None,
        })
    }

    /// Handle key events
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.previous();
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                self.delete_selected()?;
            }
            KeyCode::Char('c') | KeyCode::Enter => {
                self.copy_selected()?;
            }
            KeyCode::Char('o') => {
                self.open_selected()?;
            }
            KeyCode::Char('r') => {
                self.refresh()?;
            }
            KeyCode::Home => {
                self.select_first();
            }
            KeyCode::End => {
                self.select_last();
            }
            KeyCode::PageDown => {
                self.page_down();
            }
            KeyCode::PageUp => {
                self.page_up();
            }
            _ => {}
        }

        Ok(())
    }

    fn next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.entries.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.entries.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_first(&mut self) {
        if !self.entries.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn select_last(&mut self) {
        if !self.entries.is_empty() {
            self.list_state.select(Some(self.entries.len() - 1));
        }
    }

    fn page_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 10).min(self.entries.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn page_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(10),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn delete_selected(&mut self) -> Result<()> {
        if let Some(index) = self.list_state.selected() {
            if index < self.entries.len() {
                let entry = &self.entries[index];
                self.db.delete_entry(&entry.id)?;
                self.entries.remove(index);

                // Adjust selection
                if self.entries.is_empty() {
                    self.list_state.select(None);
                } else if index >= self.entries.len() {
                    self.list_state.select(Some(self.entries.len() - 1));
                }

                self.set_message("Entry deleted".to_string());
            }
        }
        Ok(())
    }

    fn copy_selected(&mut self) -> Result<()> {
        if let Some(index) = self.list_state.selected() {
            if index < self.entries.len() {
                let entry = &self.entries[index];

                // Decrypt entry
                let plaintext =
                    decrypt(&self.key, &entry.payload).context("Failed to decrypt entry")?;

                // Copy to clipboard
                let mut clipboard =
                    arboard::Clipboard::new().context("Failed to access clipboard")?;

                match entry.content_type {
                    ClipboardContentType::Text => {
                        let text =
                            String::from_utf8(plaintext).context("Entry contains invalid UTF-8")?;
                        clipboard
                            .set_text(text)
                            .context("Failed to set clipboard text")?;
                        self.set_message("Text copied to clipboard".to_string());
                    }
                    ClipboardContentType::Image => {
                        let img_data: ImageData = bincode::deserialize(&plaintext)
                            .context("Failed to deserialize image data")?;

                        let arboard_img = arboard::ImageData {
                            width: img_data.width,
                            height: img_data.height,
                            bytes: img_data.bytes.into(),
                        };

                        clipboard
                            .set_image(arboard_img)
                            .context("Failed to set clipboard image")?;

                        self.set_message(format!(
                            "Image copied to clipboard ({}x{})",
                            img_data.width, img_data.height
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn open_selected(&mut self) -> Result<()> {
        if let Some(index) = self.list_state.selected() {
            if index < self.entries.len() {
                let entry = &self.entries[index];

                // Decrypt entry
                let plaintext =
                    decrypt(&self.key, &entry.payload).context("Failed to decrypt entry")?;

                match entry.content_type {
                    ClipboardContentType::Text => {
                        let text =
                            String::from_utf8(plaintext).context("Entry contains invalid UTF-8")?;

                        // Create temporary file with .txt extension
                        let temp_dir = std::env::temp_dir().join("clpd_temp");
                        std::fs::create_dir_all(&temp_dir)
                            .context("Failed to create temporary directory")?;
                        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                        let file_name = format!("clpd_text_{}.txt", timestamp);
                        let temp_path = temp_dir.join(file_name);

                        // Write text to file
                        std::fs::write(&temp_path, text)
                            .context("Failed to write temporary file")?;

                        // Open with default application
                        #[cfg(target_os = "windows")]
                        std::process::Command::new("cmd")
                            .args(["/C", "start", "", temp_path.to_str().unwrap()])
                            .spawn()
                            .context("Failed to open file")?;

                        #[cfg(target_os = "macos")]
                        std::process::Command::new("open")
                            .arg(&temp_path)
                            .spawn()
                            .context("Failed to open file")?;

                        #[cfg(target_os = "linux")]
                        std::process::Command::new("xdg-open")
                            .arg(&temp_path)
                            .spawn()
                            .context("Failed to open file")?;

                        self.set_message(format!("Opened: {}", temp_path.display()));
                    }
                    ClipboardContentType::Image => {
                        let img_data: ImageData = bincode::deserialize(&plaintext)
                            .context("Failed to deserialize image data")?;

                        // Create temporary file with .png extension
                        let temp_dir = std::env::temp_dir().join("clpd_temp");
                        std::fs::create_dir_all(&temp_dir)
                            .context("Failed to create temporary directory")?;
                        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                        let file_name = format!("clpd_image_{}.png", timestamp);
                        let temp_path = temp_dir.join(file_name);

                        // Convert to PNG and save
                        let img = image::RgbaImage::from_raw(
                            img_data.width as u32,
                            img_data.height as u32,
                            img_data.bytes,
                        )
                        .ok_or_else(|| anyhow::anyhow!("Failed to create image from data"))?;

                        img.save(&temp_path).context("Failed to save image file")?;

                        // Open with default application
                        #[cfg(target_os = "windows")]
                        std::process::Command::new("cmd")
                            .args(["/C", "start", "", temp_path.to_str().unwrap()])
                            .spawn()
                            .context("Failed to open file")?;

                        #[cfg(target_os = "macos")]
                        std::process::Command::new("open")
                            .arg(&temp_path)
                            .spawn()
                            .context("Failed to open file")?;

                        #[cfg(target_os = "linux")]
                        std::process::Command::new("xdg-open")
                            .arg(&temp_path)
                            .spawn()
                            .context("Failed to open file")?;

                        self.set_message(format!(
                            "Opened: {} ({}x{})",
                            temp_path.display(),
                            img_data.width,
                            img_data.height
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        self.entries = self.db.list_entries()?;

        // Adjust selection if needed
        if self.entries.is_empty() {
            self.list_state.select(None);
        } else if let Some(index) = self.list_state.selected() {
            if index >= self.entries.len() {
                self.list_state.select(Some(self.entries.len() - 1));
            }
        } else {
            self.list_state.select(Some(0));
        }

        self.set_message("Entries refreshed".to_string());
        Ok(())
    }

    fn get_selected_entry(&self) -> Option<&ClipboardEntry> {
        self.list_state.selected().and_then(|i| self.entries.get(i))
    }

    fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
        self.message_time = Some(Instant::now());
    }

    fn clear_old_message(&mut self) {
        // Clear message after 10 seconds
        if let Some(time) = self.message_time {
            if time.elapsed() > std::time::Duration::from_secs(10) {
                self.message = None;
                self.message_time = None;
            }
        }
    }

    fn render_preview_text(&self) -> Result<Text<'static>> {
        if let Some(entry) = self.get_selected_entry() {
            // Decrypt entry
            let plaintext =
                decrypt(&self.key, &entry.payload).context("Failed to decrypt entry")?;

            match entry.content_type {
                ClipboardContentType::Text => {
                    let text = String::from_utf8_lossy(&plaintext);
                    Ok(Text::from(text.to_string()))
                }
                ClipboardContentType::Image => {
                    match bincode::deserialize::<ImageData>(&plaintext) {
                        Ok(img_data) => {
                            let preview_text = format!(
                                "Image Preview\n\nDimensions: {} x {} pixels\nSize: {} bytes",
                                img_data.width,
                                img_data.height,
                                img_data.bytes.len()
                            );
                            Ok(Text::from(preview_text))
                        }
                        Err(_) => Ok(Text::from("Failed to deserialize image data")),
                    }
                }
            }
        } else {
            Ok(Text::from("No entry selected"))
        }
    }

    fn get_image_data(&self) -> Result<Option<ImageData>> {
        if let Some(entry) = self.get_selected_entry() {
            if entry.content_type == ClipboardContentType::Image {
                let plaintext =
                    decrypt(&self.key, &entry.payload).context("Failed to decrypt entry")?;
                let img_data: ImageData =
                    bincode::deserialize(&plaintext).context("Failed to deserialize image data")?;
                return Ok(Some(img_data));
            }
        }
        Ok(None)
    }
}

/// Run the TUI
pub fn run(db: ClipboardDatabase, key: MasterKey) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(db, key)?;

    // Main loop
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        // Clear old messages
        app.clear_old_message();

        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key)?;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main content area
            Constraint::Length(3), // Bottom bar (status + controls)
        ])
        .split(f.area());

    // Main area split into left (list) and right (preview)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(15), Constraint::Percentage(85)])
        .split(chunks[0]);

    // Bottom bar split into status (left) and controls (right)
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(15), Constraint::Percentage(85)])
        .split(chunks[1]);

    // Render entry list
    render_entry_list(f, app, main_chunks[0]);

    // Render preview
    render_preview(f, app, main_chunks[1]);

    // Render status bar
    render_status_bar(f, app, bottom_chunks[0]);

    // Render controls bar
    render_controls_bar(f, bottom_chunks[1]);
}

fn render_entry_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let type_icon = match entry.content_type {
                ClipboardContentType::Text => "📝",
                ClipboardContentType::Image => "🖼️",
            };

            let time_str = entry.timestamp.format("%H:%M:%S").to_string();
            let content = format!(
                "{} {} | {}",
                type_icon,
                time_str,
                &entry.id[..entry.id.len()]
            );

            let style = if Some(i) == app.list_state.selected() {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let title = format!(" Clipboard History ({}) ", app.entries.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_preview(f: &mut Frame, app: &mut App, area: Rect) {
    // Check if we have an image to display
    if let Ok(Some(img_data)) = app.get_image_data() {
        // For images, create a visual representation using ASCII/block characters
        let preview_text = create_image_preview(
            &img_data,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        let paragraph = Paragraph::new(preview_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " Image Preview ({}x{}) ",
                        img_data.width, img_data.height
                    ))
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
        return;
    }

    // Fallback to text preview
    let preview_text = app
        .render_preview_text()
        .unwrap_or_else(|e| Text::from(format!("Error decrypting entry: {}", e)));

    let paragraph = Paragraph::new(preview_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Preview ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn create_image_preview(img_data: &ImageData, max_width: u16, max_height: u16) -> Text<'static> {
    // Calculate downsampling ratio
    // With half-block chars, each line represents 2 vertical pixels
    let width_ratio = img_data.width as f32 / max_width.max(1) as f32;
    let height_ratio = img_data.height as f32 / (max_height.max(1) * 2) as f32; // *2 for half-blocks
    let ratio = width_ratio.max(height_ratio).max(1.0);

    let display_width = (img_data.width as f32 / ratio) as usize;
    let display_height = (img_data.height as f32 / ratio) as usize;

    let mut lines = Vec::new();

    // Add header
    lines.push(Line::from(vec![Span::styled(
        format!("Dimensions: {}x{} pixels", img_data.width, img_data.height),
        Style::default().add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::raw(format!(
        "Display: {}x{} chars ({}x{} pixels)",
        display_width,
        display_height / 2,
        display_width,
        display_height
    ))]));
    lines.push(Line::from(""));

    // Create image using half-block characters for double vertical resolution
    // Process two rows at a time: top pixel uses foreground color with ▀, bottom uses background
    let num_char_rows = (display_height / 2).min(max_height.saturating_sub(3) as usize);

    for char_row in 0..num_char_rows {
        let mut line_spans = Vec::new();

        for col in 0..display_width.min(max_width as usize) {
            // Get top pixel (upper half of the character cell)
            let top_row = char_row * 2;
            let top_src_x = ((col as f32 * ratio) as usize).min(img_data.width - 1);
            let top_src_y = ((top_row as f32 * ratio) as usize).min(img_data.height - 1);
            let top_pixel_index = (top_src_y * img_data.width + top_src_x) * 4;

            // Get bottom pixel (lower half of the character cell)
            let bottom_row = char_row * 2 + 1;
            let bottom_src_x = top_src_x;
            let bottom_src_y = ((bottom_row as f32 * ratio) as usize).min(img_data.height - 1);
            let bottom_pixel_index = (bottom_src_y * img_data.width + bottom_src_x) * 4;

            // Extract colors
            let top_color = if top_pixel_index + 2 < img_data.bytes.len() {
                Color::Rgb(
                    img_data.bytes[top_pixel_index],
                    img_data.bytes[top_pixel_index + 1],
                    img_data.bytes[top_pixel_index + 2],
                )
            } else {
                Color::Reset
            };

            let bottom_color = if bottom_pixel_index + 2 < img_data.bytes.len() {
                Color::Rgb(
                    img_data.bytes[bottom_pixel_index],
                    img_data.bytes[bottom_pixel_index + 1],
                    img_data.bytes[bottom_pixel_index + 2],
                )
            } else {
                Color::Reset
            };

            // Use upper half-block (▀) with foreground = top color, background = bottom color
            line_spans.push(Span::styled(
                "▀",
                Style::default().fg(top_color).bg(bottom_color),
            ));
        }

        lines.push(Line::from(line_spans));
    }

    Text::from(lines)
}

fn render_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    // Display message if present, otherwise show empty space
    let status_text = if let Some(msg) = &app.message {
        vec![Line::from(vec![Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Green),
        )])]
    } else {
        vec![Line::from(vec![Span::raw("")])]
    };

    let status = Paragraph::new(status_text).block(
        Block::default()
            .title("Status")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(status, area);
}

fn render_controls_bar(f: &mut Frame, area: Rect) {
    let controls_text = vec![Line::from(vec![
        // Span::styled("Controls: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("Navigate: ↑↓/j/k || "),
        Span::raw("Copy: Enter/c || "),
        Span::raw("Open: o || "),
        Span::raw("Delete: d || "),
        Span::raw("Refresh: r || "),
        Span::raw("Quit: q/Esc"),
    ])];

    let controls = Paragraph::new(controls_text).block(
        Block::default()
            .title("Controls")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(controls, area);
}
