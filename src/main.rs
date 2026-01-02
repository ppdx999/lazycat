use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::{
    env,
    fs::{self, DirEntry},
    io::{self, stdout},
    path::PathBuf,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{self, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

struct App {
    current_dir: PathBuf,
    entries: Vec<DirEntry>,
    selected: usize,
    preview_lines: Vec<Line<'static>>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl App {
    fn new() -> io::Result<Self> {
        let current_dir = env::current_dir()?;
        let mut app = Self {
            current_dir,
            entries: Vec::new(),
            selected: 0,
            preview_lines: Vec::new(),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        };
        app.refresh_entries()?;
        Ok(app)
    }

    fn refresh_entries(&mut self) -> io::Result<()> {
        self.entries = fs::read_dir(&self.current_dir)?
            .filter_map(|e| e.ok())
            .collect();
        self.entries.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
        self.update_preview();
        Ok(())
    }

    fn syntect_to_ratatui_color(color: highlighting::Color) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }

    fn highlight_content(&self, content: &str, path: &PathBuf) -> Vec<Line<'static>> {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for line in LinesWithEndings::from(content) {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let fg = Self::syntect_to_ratatui_color(style.foreground);
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            lines.push(Line::from(spans));
        }
        lines
    }

    fn update_preview(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            let path = entry.path();
            if path.is_dir() {
                match fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut items: Vec<(String, bool)> = entries
                            .filter_map(|e| e.ok())
                            .map(|e| {
                                let name = e.file_name().to_string_lossy().to_string();
                                let is_dir = e.path().is_dir();
                                (name, is_dir)
                            })
                            .collect();
                        items.sort_by(|a, b| a.0.cmp(&b.0));

                        self.preview_lines = items
                            .into_iter()
                            .map(|(name, is_dir)| {
                                let display = if is_dir {
                                    format!("{}/", name)
                                } else {
                                    name
                                };
                                let style = if is_dir {
                                    Style::default().fg(Color::Blue)
                                } else {
                                    Style::default()
                                };
                                Line::from(Span::styled(display, style))
                            })
                            .collect();
                    }
                    Err(e) => {
                        self.preview_lines =
                            vec![Line::from(format!("Cannot read directory: {}", e))];
                    }
                }
            } else {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let truncated: String = content.chars().take(50000).collect();
                        self.preview_lines = self.highlight_content(&truncated, &path);
                    }
                    Err(_) => {
                        self.preview_lines = vec![Line::from("[Binary file or cannot read]")];
                    }
                }
            }
        } else {
            self.preview_lines.clear();
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.update_preview();
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
            self.update_preview();
        }
    }

    fn enter_directory(&mut self) -> io::Result<()> {
        if let Some(entry) = self.entries.get(self.selected) {
            let path = entry.path();
            if path.is_dir() {
                self.current_dir = path;
                self.selected = 0;
                self.refresh_entries()?;
            }
        }
        Ok(())
    }

    fn go_parent(&mut self) -> io::Result<()> {
        if let Some(parent) = self.current_dir.parent() {
            let old_dir = self.current_dir.clone();
            self.current_dir = parent.to_path_buf();
            self.refresh_entries()?;
            if let Some(idx) = self.entries.iter().position(|e| e.path() == old_dir) {
                self.selected = idx;
                self.update_preview();
            }
        }
        Ok(())
    }

    fn get_list_items(&self) -> Vec<ListItem<'_>> {
        self.entries
            .iter()
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.path().is_dir();
                let display = if is_dir {
                    format!("{}/", name)
                } else {
                    name
                };
                let style = if is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                ListItem::new(display).style(style)
            })
            .collect()
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(frame.area());

            let items = app.get_list_items();
            let list = List::new(items)
                .block(
                    Block::default()
                        .title(app.current_dir.to_string_lossy().to_string())
                        .borders(Borders::ALL),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");

            list_state.select(Some(app.selected));
            frame.render_stateful_widget(list, chunks[0], &mut list_state);

            let preview_title = if let Some(entry) = app.entries.get(app.selected) {
                entry.file_name().to_string_lossy().to_string()
            } else {
                "Preview".to_string()
            };

            let preview = Paragraph::new(app.preview_lines.clone())
                .block(Block::default().title(preview_title).borders(Borders::ALL))
                .wrap(Wrap { trim: false });

            frame.render_widget(preview, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                            app.enter_directory()?;
                        }
                        KeyCode::Char('h') | KeyCode::Left => {
                            app.go_parent()?;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
