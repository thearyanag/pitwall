//! Visible controls and their terminal-cell hit regions share one layout.
use crate::state::{Focus, TaskState, ViewMode, WorkerState, WorkerStatus};
use crate::{
    app::{Action, App},
    board::Request,
    images,
};
use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

#[derive(Default)]
pub struct Map {
    pub hits: Vec<(Rect, Action)>,
    pub channels: Rect,
    pub tasks: Rect,
    pub conversation: Rect,
    pub editor: Rect,
}
impl Map {
    pub fn action(&self, x: u16, y: u16) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(Position::new(x, y)))
            .map(|(_, a)| a.clone())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonTone {
    Normal,
    Selected,
    Primary,
}
impl ButtonTone {
    fn selected(selected: bool) -> Self {
        if selected {
            Self::Selected
        } else {
            Self::Normal
        }
    }
}
fn button(
    f: &mut Frame,
    rect: Rect,
    label: &str,
    p: &Palette,
    hits: &mut Vec<(Rect, Action)>,
    action: Option<Action>,
    tone: ButtonTone,
) {
    if rect.width < 3 || rect.height < 2 {
        return;
    }
    let enabled = action.is_some();
    let primary = enabled && tone == ButtonTone::Primary;
    let selected = tone == ButtonTone::Selected;
    let style = Style::default()
        .fg(if primary {
            p.on_accent
        } else if enabled {
            p.text
        } else {
            p.disabled
        })
        .bg(if primary {
            p.accent
        } else if selected {
            p.selected
        } else {
            p.raised
        })
        .add_modifier(if primary || selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    f.render_widget(
        Paragraph::new(label)
            .alignment(ratatui::layout::Alignment::Center)
            .style(style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(if primary {
                        p.accent
                    } else if selected {
                        p.focus
                    } else {
                        p.border
                    })),
            ),
        rect,
    );
    if let Some(action) = action {
        hits.push((rect, action));
    }
}
fn enabled(app: &App) -> bool {
    !app.composer.as_ref().is_some_and(|e| e.submitting)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    app.ui = Map::default();
    let area = f.area();
    let p = Palette::new(app.light);
    f.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.text)),
        area,
    );
    if area.width < 100 || area.height < 28 {
        f.render_widget(Paragraph::new("PITWALL\nResize to at least 100 columns × 28 rows.\nYour drafts are saved. Ctrl+C quits.").style(Style::default().fg(p.text)).wrap(Wrap{trim:false}),area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(20),
        Constraint::Length(2),
    ])
    .split(area);
    app.prepare_logo();
    if let Some(logo) = app.logo.as_mut() {
        logo.draw(f, Rect::new(area.x + 1, area.y + 1, 4, 2));
    }
    let header_x = area.x + if app.logo.is_some() { 7 } else { 1 };
    let header_text = Rect::new(
        header_x,
        area.y + 1,
        area.right().saturating_sub(header_x),
        2,
    );
    f.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(p.border)),
        rows[0],
    );
    let worker = WorkerStatus::read(&app.config);
    let live = if app.config.demo {
        "DEMO"
    } else {
        "VERCEL BOARD"
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "PITWALL ",
                    Style::default().fg(p.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" / {live}  ·  All your agent work, one view."),
                    Style::default().fg(p.muted),
                ),
            ]),
            Line::styled(
                format!(
                    "{} / {}{}  ·  {}  ·  Feed: {}",
                    app.config.team,
                    if app.config.board.is_empty() {
                        "General"
                    } else {
                        &app.config.board
                    },
                    if area.width >= 130 {
                        format!("  ·  {}", app.identity)
                    } else {
                        String::new()
                    },
                    worker,
                    app.feed_state
                ),
                Style::default().fg(p.muted),
            ),
        ]),
        header_text,
    );
    let columns = Layout::horizontal([
        Constraint::Length((area.width / 7).clamp(17, 24)),
        Constraint::Length(1),
        Constraint::Percentage(28),
        Constraint::Length(1),
        Constraint::Min(44),
    ])
    .split(rows[1]);
    let cols = [columns[0], columns[2], columns[4]];
    let channel_block = panel(" Channels ".into(), app.focus == Focus::Channels, &p);
    app.ui.channels = channel_block.inner(cols[0]);
    f.render_widget(channel_block, cols[0]);
    app.channels_state
        .select((!app.channels.is_empty()).then_some(app.channel));
    f.render_stateful_widget(
        List::new(
            app.channels
                .iter()
                .map(|c| ListItem::new(vec![Line::raw(format!("# {}", c.path)), Line::raw("")]))
                .collect::<Vec<_>>(),
        )
        .highlight_style(Style::default().bg(p.selected).fg(p.text))
        .highlight_symbol("› "),
        app.ui.channels,
        &mut app.channels_state,
    );
    let tasks_block = panel(
        format!(
            " {} · {}{} ",
            if app.mode == ViewMode::Tasks {
                "Tasks"
            } else {
                "Threads"
            },
            app.items.len(),
            if app.next.is_some() { "+" } else { "" }
        ),
        app.focus == Focus::Tasks,
        &p,
    );
    let ti = tasks_block.inner(cols[1]);
    f.render_widget(tasks_block, cols[1]);
    let interactive = enabled(app);
    button(
        f,
        Rect::new(ti.x, ti.y, ti.width.saturating_sub(10).min(18), 3),
        "+ New task",
        &p,
        &mut app.ui.hits,
        (interactive && !app.channels.is_empty()).then_some(Action::NewTask),
        if app.new_task {
            ButtonTone::Selected
        } else if app
            .composer
            .as_ref()
            .is_none_or(|e| e.draft.text.trim().is_empty())
        {
            ButtonTone::Primary
        } else {
            ButtonTone::Normal
        },
    );
    button(
        f,
        Rect::new(ti.right().saturating_sub(9), ti.y, 9, 3),
        "Refresh",
        &p,
        &mut app.ui.hits,
        interactive.then_some(Action::Refresh),
        ButtonTone::Normal,
    );
    app.ui.tasks = Rect::new(ti.x, ti.y + 3, ti.width, ti.height.saturating_sub(6));
    app.items_state
        .select((!app.new_task && !app.items.is_empty()).then_some(app.item));
    let items = app
        .items
        .iter()
        .map(|item| {
            ListItem::new(vec![
                Line::styled(
                    item.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(state_color(&item.state, &p))),
                    Span::styled(
                        format!("{} ", item.state.to_string().to_uppercase()),
                        Style::default().fg(p.muted),
                    ),
                    Span::styled(
                        format!("· {} replies", item.replies),
                        Style::default().fg(p.muted),
                    ),
                ]),
                Line::styled(
                    format!(
                        "{}{}",
                        item.author,
                        item.labels
                            .iter()
                            .find(|l| l.starts_with("kind="))
                            .map(|l| format!(" · {}", l.trim_start_matches("kind=")))
                            .unwrap_or_default()
                    ),
                    Style::default().fg(p.muted),
                ),
                Line::raw(""),
            ])
        })
        .collect::<Vec<_>>();
    f.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().bg(p.selected))
            .highlight_symbol("› "),
        app.ui.tasks,
        &mut app.items_state,
    );
    if app.loaded
        && !app.channels.is_empty()
        && app.items.is_empty()
        && app.pending.is_none()
        && app.error.is_none()
    {
        f.render_widget(
            Paragraph::new(if app.mode == ViewMode::Tasks {
                "No tasks yet. Create your first task."
            } else {
                "No conversations yet."
            })
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(p.muted)),
            app.ui.tasks,
        );
    }
    button(
        f,
        Rect::new(ti.x, ti.bottom().saturating_sub(3), ti.width / 2, 3),
        if app.mode == ViewMode::Tasks {
            "All threads"
        } else {
            "Tasks"
        },
        &p,
        &mut app.ui.hits,
        interactive.then_some(Action::View),
        ButtonTone::Normal,
    );
    button(
        f,
        Rect::new(
            ti.x + ti.width / 2,
            ti.bottom().saturating_sub(3),
            ti.width - ti.width / 2,
            3,
        ),
        "Load more",
        &p,
        &mut app.ui.hits,
        (interactive && app.next.is_some()).then_some(Action::More),
        ButtonTone::Normal,
    );
    let has_images = app
        .composer
        .as_ref()
        .is_some_and(|e| !e.draft.images.is_empty());
    let composer_height = (if has_images { 23 } else { 15 }).min(cols[2].height.saturating_sub(4));
    let right =
        Layout::vertical([Constraint::Min(5), Constraint::Length(composer_height)]).split(cols[2]);
    let title = if app.new_task {
        " New task ".into()
    } else if app.follow_latest {
        " Conversation · following ".into()
    } else {
        " Conversation · paused ".into()
    };
    let conversation = panel(title, app.focus == Focus::Conversation, &p);
    app.ui.conversation = conversation.inner(right[0]);
    f.render_widget(conversation, right[0]);
    let body = if app.new_task {
        vec![
            Line::styled("What should Codex work on?", Style::default().fg(p.text)),
            Line::raw(""),
            Line::raw("Describe the task below. Paste or attach screenshots for context."),
        ]
    } else {
        conversation_lines(app, &p)
    };
    let paragraph = Paragraph::new(body).wrap(Wrap { trim: false });
    let inner = app.ui.conversation;
    app.max_scroll = paragraph
        .line_count(inner.width)
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    app.page_height = inner.height.max(1);
    app.scroll = if app.follow_latest && !app.new_task && app.thread.is_some() {
        app.max_scroll
    } else {
        app.scroll.min(app.max_scroll)
    };
    f.render_widget(paragraph.scroll((app.scroll, 0)), inner);
    draw_composer(f, app, right[1], &p, &worker);
    let status = if let Some((message, at)) = &app.toast {
        if at.elapsed().as_secs() < 8 {
            message.clone()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let status = if !status.is_empty() {
        status
    } else if let Some(error) = app.error.as_ref().or(app.feed_error.as_ref()) {
        error.clone()
    } else {
        format!(
            "Updated {}s ago · drafts save automatically",
            app.refreshed.elapsed().as_secs()
        )
    };
    f.render_widget(Paragraph::new(vec![Line::styled(if app.focus==Focus::Composer{" Enter: new line  ·  Cmd+Enter: send  ·  Cmd+V: paste  ·  Esc: conversation"}else{" Click to navigate  ·  Tab: focus  ·  n: new task  ·  c: reply  ·  ? help  ·  q: quit"},Style::default().fg(p.muted)),Line::styled(status,Style::default().fg(if app.error.is_some(){p.bad}else{p.muted}))]),rows[2]);
    if app.file_picker.is_some() {
        app.reset_previews();
        app.ui.hits.clear();
        draw_file_picker(f, app, &p);
        return;
    }
    if app.help {
        app.reset_previews();
        draw_help(f, &p);
        app.ui.hits.clear();
        return;
    }
    if app.image_open {
        app.reset_previews();
        app.ui.hits.clear();
        if let Some(preview) = app.preview.as_mut() {
            draw_preview(f, preview, &p);
        } else {
            f.render_widget(Clear, area);
            f.render_widget(
                Paragraph::new(app.error.clone().unwrap_or_else(|| "Loading image…".into())),
                area,
            );
        }
        overlay_buttons(f, app, area, &p);
        return;
    }
    if app.composer.as_ref().is_some_and(|e| e.expanded) {
        app.reset_previews();
        app.ui.hits.clear();
        if let Some(preview) = app.composer.as_mut().and_then(|e| e.preview.as_mut()) {
            draw_preview(f, preview, &p);
        }
        overlay_buttons(f, app, area, &p);
    }
}
fn overlay_buttons(f: &mut Frame, app: &mut App, area: Rect, p: &Palette) {
    button(
        f,
        Rect::new(area.x + 2, area.bottom().saturating_sub(5), 14, 3),
        "Close · Esc",
        p,
        &mut app.ui.hits,
        Some(Action::ClosePreview),
        ButtonTone::Primary,
    );
    button(
        f,
        Rect::new(area.x + 18, area.bottom().saturating_sub(5), 14, 3),
        "Next image",
        p,
        &mut app.ui.hits,
        Some(Action::NextImages),
        ButtonTone::Normal,
    );
}
fn draw_composer(f: &mut Frame, app: &mut App, area: Rect, p: &Palette, worker: &WorkerStatus) {
    let Some(editor) = app.composer.as_mut() else {
        f.render_widget(
            Paragraph::new("Choose a channel to start a task.").block(panel(
                " Message ".into(),
                false,
                p,
            )),
            area,
        );
        return;
    };
    let is_reply = editor.draft.thread.is_some();
    let title = if is_reply { " Reply " } else { " New task " };
    let block = panel(title.into(), app.focus == Focus::Composer, p);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let with_images = !editor.draft.images.is_empty();
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(if with_images {
            if area.height >= 22 {
                8
            } else {
                6
            }
        } else {
            0
        }),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(inner);
    let editable = !editor.draft.locked && !editor.submitting;
    if is_reply {
        let width = parts[0].width.min(18);
        button(
            f,
            Rect::new(parts[0].x, parts[0].y, width, 3),
            "Send to Codex",
            p,
            &mut app.ui.hits,
            editable.then_some(Action::Mode(true)),
            ButtonTone::selected(editor.draft.send_to_codex),
        );
        button(
            f,
            Rect::new(
                parts[0].x + width + 1,
                parts[0].y,
                parts[0].width.saturating_sub(width + 1).min(17),
                3,
            ),
            "Comment only",
            p,
            &mut app.ui.hits,
            editable.then_some(Action::Mode(false)),
            ButtonTone::selected(!editor.draft.send_to_codex),
        );
    } else {
        f.render_widget(
            Paragraph::new(format!("Create a task in #{}", editor.draft.channel))
                .style(Style::default().fg(p.muted)),
            parts[0],
        );
    }
    app.ui.editor = parts[1];
    editor
        .textarea
        .set_style(Style::default().fg(p.text).bg(p.raised));
    editor.textarea.set_cursor_line_style(Style::default());
    editor
        .textarea
        .set_cursor_style(if app.focus == Focus::Composer {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
    editor
        .textarea
        .set_selection_style(Style::default().bg(p.selected));
    editor.textarea.set_placeholder_text(if is_reply {
        "Reply to Codex…"
    } else {
        "What should Codex work on?"
    });
    editor
        .textarea
        .set_placeholder_style(Style::default().fg(p.muted));
    f.render_widget(&editor.textarea, parts[1]);
    if with_images {
        let fits = (parts[2].width / 17).max(1) as usize;
        let capacity = if editor.draft.images.len() > fits {
            (parts[2].width.saturating_sub(10) / 17).max(1) as usize
        } else {
            fits
        };
        let start = editor.image_index / capacity * capacity;
        for index in start..(start + capacity).min(editor.draft.images.len()) {
            let image = editor.draft.images[index].clone();
            let x = parts[2].x + (index - start) as u16 * 17;
            let tile = Rect::new(x, parts[2].y, 16, parts[2].height);
            let block = Block::default()
                .title(format!(" {} ", index + 1))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(p.border));
            f.render_widget(block, tile);
            let photo = Rect::new(x + 1, tile.y + 3, 14, tile.height.saturating_sub(4));
            let path = image.path.to_string_lossy().to_string();
            if index == editor.image_index {
                if let Some(preview) = editor.preview.as_mut() {
                    preview.draw(f, photo);
                }
            } else {
                if !editor.thumbnails.contains_key(&path) {
                    if let Ok(preview) =
                        images::Preview::load(&image.path, image.name.clone(), &app.image_picker)
                    {
                        editor.thumbnails.insert(path.clone(), preview);
                    }
                }
                if let Some(preview) = editor.thumbnails.get_mut(&path) {
                    preview.draw(f, photo);
                }
            }
            app.ui.hits.push((photo, Action::Expand(index)));
            button(
                f,
                Rect::new(x + 11, tile.y, 5, 3),
                "×",
                p,
                &mut app.ui.hits,
                editable.then_some(Action::Remove(index)),
                ButtonTone::Normal,
            );
        }
        if editor.draft.images.len() > capacity {
            let more = Rect::new(parts[2].right().saturating_sub(9), parts[2].y, 9, 3);
            button(
                f,
                more,
                "Next →",
                p,
                &mut app.ui.hits,
                Some(Action::NextImages),
                ButtonTone::Normal,
            );
        }
    }
    let actions = parts[3];
    let send_width = if is_reply { 14 } else { 15 };
    button(
        f,
        Rect::new(actions.x, actions.y, 10, 3),
        "Attach",
        p,
        &mut app.ui.hits,
        editable.then_some(Action::Attach),
        ButtonTone::Normal,
    );
    button(
        f,
        Rect::new(actions.x + 11, actions.y, 9, 3),
        "Paste",
        p,
        &mut app.ui.hits,
        editable.then_some(Action::Paste),
        ButtonTone::Normal,
    );
    let can_send = !editor.submitting && !editor.draft.text.trim().is_empty() && !app.config.demo;
    let label = if editor.submitting {
        "Sending…"
    } else if editor.draft.locked {
        "Retry send"
    } else if is_reply {
        "Send"
    } else {
        "Create task"
    };
    button(
        f,
        Rect::new(
            actions.right().saturating_sub(send_width),
            actions.y,
            send_width,
            3,
        ),
        label,
        p,
        &mut app.ui.hits,
        can_send.then_some(Action::Send),
        ButtonTone::Primary,
    );
    let hint = if let Some(error) = &editor.error {
        error.clone()
    } else if editor.submitting {
        "Sending your message…".into()
    } else if is_reply && !editor.draft.send_to_codex {
        "Comment only · Codex will not run".into()
    } else if *worker == WorkerStatus::Online(WorkerState::Working) {
        "Codex is working · your message will run next".into()
    } else if *worker == WorkerStatus::Online(WorkerState::Waiting) {
        "Worker ready · sending starts Codex".into()
    } else {
        "Worker offline · your task will wait in the queue".into()
    };
    f.render_widget(
        Paragraph::new(hint)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(if editor.error.is_some() {
                p.bad
            } else {
                p.muted
            })),
        parts[4],
    );
}

fn draw_file_picker(f: &mut Frame, app: &mut App, p: &Palette) {
    let area = f.area();
    let width = area.width.saturating_sub(8).min(100);
    let height = area.height.saturating_sub(6).min(30);
    let rect = Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, rect);
    let block =
        panel(" Attach image ".into(), true, p).style(Style::default().bg(p.raised).fg(p.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let picker = app.file_picker.as_ref().unwrap();
    f.render_widget(
        Paragraph::new(picker.directory.to_string_lossy().to_string()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    f.render_widget(
        Paragraph::new(format!("Path: {}", picker.input)).style(Style::default().fg(p.text)),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
    let count = inner.height.saturating_sub(9) as usize;
    let offset = picker.selected.saturating_sub(count.saturating_sub(1));
    for (n, path) in picker.entries.iter().enumerate().skip(offset).take(count) {
        let row = Rect::new(inner.x, inner.y + 4 + (n - offset) as u16, inner.width, 1);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let label = format!(
            "{} {}{}",
            if n == picker.selected { "›" } else { " " },
            name,
            if path.is_dir() { "/" } else { "" }
        );
        f.render_widget(
            Paragraph::new(label).style(Style::default().bg(if n == picker.selected {
                p.selected
            } else {
                p.raised
            })),
            row,
        );
        app.ui.hits.push((row, Action::Pick(n)));
    }
    let hint = picker
        .error
        .as_deref()
        .unwrap_or("Click a folder or image · ↑↓ select · Enter opens · Type or paste a path");
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(p.muted)),
        Rect::new(inner.x, inner.bottom() - 5, inner.width, 1),
    );
    for (x, label, action) in [
        (inner.x, "Up", Action::PickerUp),
        (inner.x + 12, "Cancel", Action::PickerClose),
        (inner.right() - 16, "Open / attach", Action::PickerAccept),
    ] {
        button(
            f,
            Rect::new(
                x,
                inner.bottom() - 3,
                if label == "Open / attach" { 16 } else { 10 },
                3,
            ),
            label,
            p,
            &mut app.ui.hits,
            Some(action),
            if label == "Open / attach" {
                ButtonTone::Primary
            } else {
                ButtonTone::Normal
            },
        );
    }
    f.set_cursor_position((
        inner.x + (6 + picker.input.chars().count() as u16).min(inner.width - 1),
        inner.y + 2,
    ));
}

pub(crate) struct Palette {
    pub(crate) bg: Color,
    pub(crate) text: Color,
    muted: Color,
    accent: Color,
    on_accent: Color,
    surface: Color,
    pub(crate) raised: Color,
    border: Color,
    pub(crate) focus: Color,
    selected: Color,
    disabled: Color,
    running: Color,
    warning: Color,
    done: Color,
    bad: Color,
}
impl Palette {
    pub(crate) fn new(light: bool) -> Self {
        // Brand: Carbon #0E0F12, Radio Orange #FF5A1F,
        // Track Grey #8A8F98, Paddock White #F4F4F1.
        let carbon = Color::Rgb(14, 15, 18);
        let white = Color::Rgb(244, 244, 241);
        let orange = Color::Rgb(255, 90, 31);
        if light {
            Self {
                bg: white,
                text: carbon,
                muted: Color::Rgb(98, 104, 115),
                accent: orange,
                on_accent: carbon,
                surface: Color::Rgb(255, 255, 255),
                raised: Color::Rgb(237, 238, 240),
                border: Color::Rgb(201, 205, 212),
                focus: Color::Rgb(98, 104, 115),
                selected: Color::Rgb(221, 224, 230),
                disabled: Color::Rgb(128, 133, 143),
                running: Color::Rgb(21, 128, 61),
                warning: Color::Rgb(147, 96, 0),
                done: Color::Rgb(109, 40, 217),
                bad: Color::Rgb(185, 28, 28),
            }
        } else {
            Self {
                bg: carbon,
                text: white,
                muted: Color::Rgb(138, 143, 152),
                accent: orange,
                on_accent: carbon,
                surface: Color::Rgb(20, 22, 27),
                raised: Color::Rgb(28, 31, 37),
                border: Color::Rgb(48, 52, 61),
                focus: Color::Rgb(138, 143, 152),
                selected: Color::Rgb(34, 37, 44),
                disabled: Color::Rgb(104, 110, 121),
                running: Color::Rgb(74, 222, 128),
                warning: Color::Rgb(250, 204, 21),
                done: Color::Rgb(167, 139, 250),
                bad: Color::Rgb(248, 113, 113),
            }
        }
    }
}
fn panel(title: String, focused: bool, p: &Palette) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(Style::default().fg(if focused { p.text } else { p.muted }))
        .style(Style::default().bg(p.surface).fg(p.text))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { p.focus } else { p.border }))
}
fn state_color(state: &TaskState, p: &Palette) -> Color {
    match state {
        TaskState::Completed => p.done,
        TaskState::Claimed => p.running,
        TaskState::Blocked => p.warning,
        TaskState::Failed => p.bad,
        _ => p.muted,
    }
}
fn content(text: &str, p: &Palette) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            if let Some(title) = line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")) {
                Line::styled(
                    title.to_owned(),
                    Style::default().fg(p.text).add_modifier(Modifier::BOLD),
                )
            } else {
                Line::raw(line.to_owned())
            }
        })
        .collect()
}
pub(crate) fn conversation_lines(app: &App, p: &Palette) -> Vec<Line<'static>> {
    let Some(thread) = &app.thread else {
        let explanation = if matches!(app.pending, Some(Request::Thread { .. })) {
            "Loading conversation…"
        } else if let Some(error) = &app.error {
            return content(&format!("Could not load Board data\n\n{error}\n\nPress r to retry, or d to explore fictional demo data."), p);
        } else if app.loaded && app.channels.is_empty() {
            "This Board has no visible channels yet.\n\nPitwall is connected. Channels appear here when someone creates one or posts to Board.\n\nPress r to refresh or d to explore fictional demo data."
        } else if app.loaded && app.items.is_empty() && app.pending.is_none() {
            return content(
                if app.mode == ViewMode::Tasks {
                    "No tasks yet. Create your first task."
                } else {
                    "No conversations yet. Select Tasks to create your first task."
                },
                p,
            );
        } else {
            "Select a task to read its conversation.\n\nTab moves between columns.\n↑↓ selects; in this column it scrolls.\nr refreshes; v switches tasks / threads.\n\nPress n to create a task for the Codex queue worker."
        };
        return content(explanation, p);
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("● ", Style::default().fg(state_color(&thread.state, p))),
            Span::styled(
                thread.state.to_string().to_uppercase(),
                Style::default().fg(p.text),
            ),
            Span::styled(
                format!("  ·  {} messages", thread.messages.len()),
                Style::default().fg(p.muted),
            ),
        ]),
        Line::raw(""),
    ];
    if let Some(error) = &app.error {
        lines.insert(
            0,
            Line::styled(
                format!("Refresh failed; showing previous data: {error}"),
                Style::default().fg(p.bad),
            ),
        );
    }
    for detail in &thread.details {
        lines.extend(content(detail, p));
    }
    if !thread.details.is_empty() {
        lines.push(Line::raw(""));
    }
    for message in &thread.messages {
        lines.push(Line::from(vec![
            Span::styled(
                message.author.clone(),
                Style::default().fg(p.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", message.time.replace('T', " ").trim_end_matches('Z')),
                Style::default().fg(p.muted),
            ),
        ]));
        lines.push(Line::styled(
            format!("#{}", message.id),
            Style::default().fg(p.muted),
        ));
        lines.push(Line::raw(""));
        lines.extend(content(&message.text, p));
        for file in &message.attachments {
            lines.push(Line::styled(
                format!(
                    "Image: {} · {} KB · p previews",
                    file.name,
                    file.size / 1024
                ),
                Style::default().fg(p.muted),
            ));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "────────────────────",
            Style::default().fg(p.border),
        ));
        lines.push(Line::raw(""));
    }
    lines
}

fn draw_help(f: &mut Frame, p: &Palette) {
    let area = f.area();
    let width = area.width.min(76);
    let height = area.height.min(18);
    let overlay = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let lines = [
        " Tab / Shift-Tab          Cycle column focus",
        " ← / →, h / l, Enter      Move between columns",
        " ↑ / ↓, j / k             Select channel/task; scroll conversation",
        " Space / PgDn / PgUp      Scroll conversation by a page",
        " g / G, Home / End        Conversation beginning / end",
        " r                       Refresh channels and current list",
        " v                       Switch tasks / all threads",
        " m                       Next task page / older threads",
        " t                       Toggle light/dark theme",
        " d                       Switch demo/live Board",
        " q / Ctrl-C              Quit and restore the terminal",
        " ? / Esc                 Close help",
        "",
        " n / c / p               New task / reply to worker / images",
        " Cmd+Enter sends · Enter adds a line · Comment only skips Codex.",
    ];
    f.render_widget(Clear, overlay);
    f.render_widget(
        Paragraph::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>())
            .style(Style::default().bg(p.raised).fg(p.text))
            .block(
                panel(" Pitwall · Keyboard help ".into(), true, p)
                    .style(Style::default().bg(p.raised)),
            ),
        overlay,
    );
}

fn draw_preview(f: &mut Frame, preview: &mut images::Preview, p: &Palette) {
    let area = f.area();
    let rect = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    f.render_widget(Clear, rect);
    let block = Block::default()
        .title(format!(" {} ", preview.title))
        .borders(Borders::ALL)
        .style(Style::default().bg(p.bg).fg(p.text))
        .border_style(Style::default().fg(p.focus));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    preview.draw(
        f,
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(4),
        ),
    );
}
