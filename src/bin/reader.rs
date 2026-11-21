//! Read in this order: App -> handle_key -> draw_ui -> run -> main.
//! The whole demo is one terminal process. Its panels are Ratatui rectangles.
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use std::{
    env, fs,
    io::{self, IsTerminal},
    path::PathBuf,
};

struct Document {
    title: String,
    body: String,
}

// 1. State: plain data. None of these fields are terminal or Rex settings.
struct App {
    docs: Vec<Document>,
    selected: usize,
    scroll: u16,
    reader_focus: bool,
    inspector: bool,
    light: bool,
    frame: usize,
    last_event: String,
    transition: String,
    max_scroll: u16,
    page_height: u16,
    list: ListState,
}

impl App {
    fn new(docs: Vec<Document>) -> Self {
        Self {
            docs,
            selected: 0,
            scroll: 0,
            reader_focus: false,
            inspector: true,
            light: false,
            frame: 0,
            last_event: "Waiting for your first key".into(),
            transition: "App created -> first frame".into(),
            max_scroll: 0,
            page_height: 1,
            list: ListState::default(),
        }
    }

    // 2. Input: update state, then let the next draw reflect that state.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.last_event = format!("{:?}  modifiers={:?}", key.code, key.modifiers);
        let before = self.state();
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Tab | KeyCode::BackTab => self.reader_focus = !self.reader_focus,
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => self.reader_focus = true,
            KeyCode::Left | KeyCode::Char('h') => self.reader_focus = false,
            KeyCode::Char('i') => self.inspector = !self.inspector,
            KeyCode::Char('t') => self.light = !self.light,
            KeyCode::Down | KeyCode::Char('j') => {
                if self.reader_focus {
                    self.scroll = self.scroll.saturating_add(1).min(self.max_scroll);
                } else if self.selected + 1 < self.docs.len() {
                    self.selected += 1;
                    self.scroll = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.reader_focus {
                    self.scroll = self.scroll.saturating_sub(1);
                } else if self.selected > 0 {
                    self.selected -= 1;
                    self.scroll = 0;
                }
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.reader_focus = true;
                self.scroll = self
                    .scroll
                    .saturating_add(self.page_height)
                    .min(self.max_scroll);
            }
            KeyCode::PageUp => {
                self.reader_focus = true;
                self.scroll = self.scroll.saturating_sub(self.page_height);
            }
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = self.max_scroll,
            _ => {}
        }
        let after = self.state();
        self.transition = if before == after {
            "No state change (unbound key or boundary)".into()
        } else {
            before
                .split_whitespace()
                .zip(after.split_whitespace())
                .filter(|(old, new)| old != new)
                .map(|(old, new)| format!("{old} -> {new}"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        false
    }
    fn state(&self) -> String {
        format!(
            "doc={} scroll={} focus={} theme={} inspector={}",
            self.selected + 1,
            self.scroll,
            if self.reader_focus {
                "reader"
            } else {
                "library"
            },
            if self.light { "light" } else { "dark" },
            self.inspector
        )
    }
}

struct Palette {
    bg: Color,
    text: Color,
    muted: Color,
    accent: Color,
    border: Color,
    selection: Color,
}
impl Palette {
    fn new(light: bool) -> Self {
        if light {
            Self {
                bg: Color::Rgb(246, 245, 237),
                text: Color::Rgb(35, 49, 42),
                muted: Color::Rgb(90, 109, 98),
                accent: Color::Rgb(24, 112, 76),
                border: Color::Rgb(190, 200, 186),
                selection: Color::Rgb(215, 232, 213),
            }
        } else {
            Self {
                bg: Color::Rgb(18, 25, 24),
                text: Color::Rgb(223, 231, 213),
                muted: Color::Rgb(137, 159, 146),
                accent: Color::Rgb(174, 215, 128),
                border: Color::Rgb(57, 77, 66),
                selection: Color::Rgb(40, 61, 45),
            }
        }
    }
}

fn panel<'a>(title: &'a str, focused: bool, p: &Palette) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { p.accent } else { p.border }))
}

// Small Markdown presentation, deliberately not a complete Markdown parser.
fn document_lines(body: &str, p: &Palette) -> Vec<Line<'static>> {
    let mut code = false;
    body.lines()
        .map(|line| {
            if line.starts_with("```") {
                code = !code;
                return Line::styled("  -----", Style::default().fg(p.muted));
            }
            if code {
                return Line::styled(format!("  {line}"), Style::default().fg(p.accent));
            }
            if let Some(text) = line.strip_prefix("# ") {
                Line::styled(
                    text.to_string(),
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                )
            } else if let Some(text) = line.strip_prefix("## ") {
                Line::styled(
                    text.to_string(),
                    Style::default().fg(p.text).add_modifier(Modifier::BOLD),
                )
            } else if let Some(text) = line.strip_prefix("- ") {
                Line::from(vec![
                    Span::styled("• ", Style::default().fg(p.accent)),
                    Span::raw(text.to_string()),
                ])
            } else {
                Line::raw(line.to_string())
            }
        })
        .collect()
}

// 3. Rendering: describe the entire desired frame, as widgets in rectangles.
// Terminal::draw compares that buffer with the previous one and sends updates.
fn draw_ui(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let p = Palette::new(app.light);
    f.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.text)),
        area,
    );
    if area.width < 64 || area.height < 18 {
        f.render_widget(Paragraph::new("SPROUT\nMake the terminal at least 64 columns × 18 rows.\nq quits; resizing redraws automatically.")
            .style(Style::default().fg(p.accent)).wrap(Wrap { trim: false }), area);
        return;
    }
    let inspector_height = if app.inspector && area.height >= 26 {
        8
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(inspector_height),
        Constraint::Length(2),
    ])
    .split(area);
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " SPROUT ",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " / a terminal reader, with the mechanism visible",
            Style::default().fg(p.muted),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(p.border)),
    );
    f.render_widget(header, rows[0]);
    let cols = Layout::horizontal([Constraint::Length(25), Constraint::Min(20)]).split(rows[1]);
    let items: Vec<ListItem> = app
        .docs
        .iter()
        .enumerate()
        .map(|(n, doc)| ListItem::new(format!("{:02}  {}", n + 1, doc.title)))
        .collect();
    app.list.select(Some(app.selected));
    f.render_stateful_widget(
        List::new(items)
            .block(panel(" Library ", !app.reader_focus, &p))
            .highlight_style(
                Style::default()
                    .bg(p.selection)
                    .fg(p.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        cols[0],
        &mut app.list,
    );
    let reader_block = panel(" Reader ", app.reader_focus, &p);
    let inner = reader_block.inner(cols[1]);
    let content_area = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let paragraph = Paragraph::new(document_lines(&app.docs[app.selected].body, &p))
        .style(Style::default().fg(p.text))
        .wrap(Wrap { trim: false });
    app.page_height = content_area.height.max(1);
    app.max_scroll = paragraph
        .line_count(content_area.width)
        .saturating_sub(content_area.height as usize)
        .min(u16::MAX as usize) as u16;
    app.scroll = app.scroll.min(app.max_scroll);
    f.render_widget(reader_block, cols[1]);
    f.render_widget(paragraph.scroll((app.scroll, 0)), content_area);
    if inspector_height > 0 {
        let text = vec![
            Line::styled(
                " INPUT → STATE → WIDGETS → TERMINAL CELLS",
                Style::default().fg(p.accent),
            ),
            Line::raw(format!(" Event: {}", app.last_event)),
            Line::raw(format!(" State: {}", app.state())),
            Line::styled(
                format!(" Change: {}", app.transition),
                Style::default().fg(p.muted),
            ),
            Line::raw(format!(
                " Draw #{}  |  canvas {}×{} cells  |  reader {}×{}  |  scroll {}/{}",
                app.frame,
                area.width,
                area.height,
                content_area.width,
                content_area.height,
                app.scroll,
                app.max_scroll
            )),
            Line::styled(
                " Ratatui builds a cell buffer; Crossterm emits cursor/color/text commands.",
                Style::default().fg(p.muted),
            ),
        ];
        f.render_widget(
            Paragraph::new(text).block(panel(" Inspector · i to toggle ", false, &p)),
            rows[2],
        );
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(
                " Tab focus   ↑↓ / jk move   Space page   t theme   i inspector   q quit",
                Style::default().fg(p.accent),
            ),
            Line::styled(
                if app.inspector && inspector_height == 0 {
                    " Enlarge to 26 rows to see the inspector."
                } else {
                    " All panels above are drawn by Sprout inside a single terminal pane."
                },
                Style::default().fg(p.muted),
            ),
        ]),
        rows[3],
    );
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        app.frame += 1;
        terminal.draw(|frame| draw_ui(frame, app))?;
        // No timer or busy loop: wait for input, update state, draw again.
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if app.handle_key(key) {
                    return Ok(());
                }
            }
            Event::Resize(w, h) => {
                app.last_event = format!("Resize({w}, {h})");
                app.transition = "Terminal resized -> recompute layout and text wrapping".into();
            }
            _ => {}
        }
    }
}

// Do not let control characters in a supplied text file become terminal commands.
fn sanitize(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c == '\t' {
                "    ".chars().collect::<Vec<_>>()
            } else if c == '\n' || !c.is_control() {
                vec![c]
            } else {
                vec![]
            }
        })
        .collect()
}

fn load_documents() -> io::Result<Vec<Document>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        return Ok(vec![
            Document {
                title: "The canvas".into(),
                body: include_str!("../../lessons/01-the-canvas.md").into(),
            },
            Document {
                title: "Input to pixels".into(),
                body: include_str!("../../lessons/02-input-to-pixels.md").into(),
            },
            Document {
                title: "Leaf & Rex".into(),
                body: include_str!("../../lessons/03-leaf-and-rex.md").into(),
            },
        ]);
    }
    paths
        .iter()
        .map(|path| {
            let body = fs::read_to_string(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
            Ok(Document {
                title: sanitize(&path.file_name().unwrap_or_default().to_string_lossy()),
                body: sanitize(&body),
            })
        })
        .collect()
}

fn main() -> io::Result<()> {
    if env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!("Sprout — a terminal reader you can inspect\n\nUsage: sprout [file.md file.txt ...]\nNo arguments opens the built-in lessons.\n\nTab: focus | arrows/jk: move | Space: page | t: theme | i: inspector | q/Ctrl-C: quit");
        return Ok(());
    }
    let mut app = App::new(load_documents()?);
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "Run Sprout inside an interactive terminal (Rex, Terminal, etc.).",
        ));
    }
    // init enters raw mode + alternate screen and installs panic restoration.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore(); // Always restore, including when run returns an I/O error.
    result
}
