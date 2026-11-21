//! A task draft survives an uncertain submission, with the same idempotency key.
use crate::board::Config;
use crate::images::{self, LocalImage, Preview};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_image::picker::Picker;
use ratatui_textarea::{CursorMove, Input, TextArea, WrapMode};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Draft {
    pub channel: String,
    pub text: String,
    pub key: String,
    pub team: String,
    pub board: String,
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub thread_title: String,
    #[serde(default = "default_true")]
    pub send_to_codex: bool,
    #[serde(default)]
    pub images: Vec<LocalImage>,
    #[serde(default = "default_true")]
    pub locked: bool,
}
fn default_true() -> bool {
    true
}
pub struct Composer {
    pub draft: Draft,
    pub textarea: TextArea<'static>,
    pub submitting: bool,
    pub error: Option<String>,
    pub preview: Option<Preview>,
    pub image_index: usize,
    pub expanded: bool,
    pub thumbnails: std::collections::HashMap<String, Preview>,
}
const LIMIT: usize = 8000;
impl Composer {
    pub fn new(channel: String, config: &Config) -> Self {
        Self::from_draft(Draft {
            channel,
            text: String::new(),
            key: format!(
                "sprout-{:x}-{:x}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            team: config.team.clone(),
            board: config.board.clone(),
            thread: None,
            thread_title: String::new(),
            send_to_codex: true,
            images: vec![],
            locked: false,
        })
    }
    fn from_draft(draft: Draft) -> Self {
        let mut textarea = TextArea::from(
            draft
                .text
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        );
        textarea.set_wrap_mode(WrapMode::WordOrGlyph);
        textarea.move_cursor(CursorMove::Bottom);
        textarea.move_cursor(CursorMove::End);
        Self {
            textarea,
            draft,
            submitting: false,
            error: None,
            preview: None,
            image_index: 0,
            expanded: false,
            thumbnails: std::collections::HashMap::new(),
        }
    }
    pub fn reply(channel: String, thread: String, title: String, config: &Config) -> Self {
        let mut editor = Self::new(channel, config);
        editor.draft.thread = Some(thread);
        editor.draft.thread_title = title;
        editor
    }
    pub fn insert(&mut self, text: &str) {
        if self.draft.locked || self.submitting {
            return;
        }
        let selected = if self.textarea.is_selecting() {
            let mut selection = self.textarea.clone();
            selection.copy();
            selection.yank_text().chars().count()
        } else {
            0
        };
        let available =
            LIMIT.saturating_sub(self.draft.text.chars().count().saturating_sub(selected));
        let clean: String = text
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .take(available)
            .map(|c| if c == '\t' { ' ' } else { c })
            .collect();
        self.textarea.insert_str(clean);
        self.sync_text();
    }
    /// Editing, wrapping, selection and history belong to ratatui-textarea.
    pub fn key(&mut self, key: KeyEvent) {
        if self.draft.locked || self.submitting {
            return;
        }
        let mut input = Input::from(key);
        input.ctrl |= key.modifiers.contains(KeyModifiers::SUPER);
        if key.code == KeyCode::Char('a') && input.ctrl {
            self.textarea.select_all();
            return;
        }
        let before = self.textarea.clone();
        if matches!(key.code, KeyCode::Char('z' | 'Z')) && input.ctrl {
            if input.shift || key.code == KeyCode::Char('Z') {
                self.textarea.redo();
            } else {
                self.textarea.undo();
            }
        } else {
            self.textarea.input(input);
        }
        if self.textarea.lines().join("\n").chars().count() > LIMIT {
            self.textarea = before;
            self.error = Some("Messages are limited to 8,000 characters.".into());
        }
        self.sync_text();
    }
    fn sync_text(&mut self) {
        self.draft.text = self.textarea.lines().join("\n");
    }
    pub fn cursor_at(&mut self, row: usize, col: usize) {
        let (top, left) = self.textarea.scroll_offset();
        let cursor = self
            .textarea
            .screen_to_data(row + usize::from(top), col + usize::from(left));
        self.textarea.cancel_selection();
        self.textarea
            .move_cursor(CursorMove::Jump(cursor.0 as u16, cursor.1 as u16));
    }
    pub fn refresh_preview(&mut self, picker: &Picker) {
        self.preview = None;
        if self.draft.images.is_empty() {
            self.image_index = 0;
            self.expanded = false;
            return;
        }
        self.image_index = self.image_index.min(self.draft.images.len() - 1);
        let image = &self.draft.images[self.image_index];
        match Preview::load(
            &image.path,
            format!("{} · {}×{}", image.name, image.width, image.height),
            picker,
        ) {
            Ok(preview) => self.preview = Some(preview),
            Err(error) => self.error = Some(error),
        }
    }
    pub fn add_image(
        &mut self,
        image: image::DynamicImage,
        config: &Config,
        picker: &Picker,
    ) -> Result<(), String> {
        if self.draft.locked {
            return Err("A submitted task cannot change. Retry the same task.".into());
        }
        if self.draft.images.len() >= images::MAX_IMAGES {
            return Err("Attach at most 10 screenshots.".into());
        }
        let staged = images::stage(
            config,
            &self.draft.key,
            image,
            format!("screenshot-{}.png", self.draft.images.len() + 1),
        )?;
        self.draft.images.push(staged);
        self.image_index = self.draft.images.len() - 1;
        self.refresh_preview(picker);
        self.save(config)
    }
    pub fn save(&self, config: &Config) -> Result<(), String> {
        let pending = if self.draft.thread.is_some() {
            reply_path(config)
        } else {
            pending_path(config)
        };
        if self.draft.locked && pending.exists() {
            let old: Draft =
                serde_json::from_slice(&fs::read(&pending).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if old.key != self.draft.key {
                return Err(format!(
                    "Resolve the unconfirmed send for {} before sending another message.",
                    if old.thread_title.is_empty() {
                        "the new task"
                    } else {
                        &old.thread_title
                    }
                ));
            }
        }
        let path = if self.draft.locked {
            pending
        } else {
            draft_path(config, &self.draft.channel, self.draft.thread.as_deref())
        };
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(
            &tmp,
            serde_json::to_vec(&self.draft).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
    pub fn load_for(
        config: &Config,
        channel: String,
        thread: Option<String>,
        title: String,
    ) -> Result<Self, String> {
        let pending = if thread.is_some() {
            reply_path(config)
        } else {
            pending_path(config)
        };
        if let Some(editor) = Self::resume_file(config, pending.clone())? {
            if editor.draft.thread == thread && editor.draft.channel == channel {
                if !editor.draft.locked {
                    editor.save(config)?;
                    clear_file(pending)?;
                }
                return Ok(editor);
            }
        }
        if let Some(editor) =
            Self::resume_file(config, draft_path(config, &channel, thread.as_deref()))?
        {
            if editor.draft.channel != channel || editor.draft.thread != thread {
                return Err("Saved draft does not match this conversation.".into());
            }
            return Ok(editor);
        }
        Ok(match thread {
            Some(id) => Self::reply(channel, id, title, config),
            None => Self::new(channel, config),
        })
    }
    pub fn remove_saved(&self, config: &Config) -> Result<(), String> {
        clear_file(draft_path(
            config,
            &self.draft.channel,
            self.draft.thread.as_deref(),
        ))
    }

    fn resume_file(config: &Config, path: PathBuf) -> Result<Option<Self>, String> {
        if !path.exists() {
            return Ok(None);
        }
        let draft: Draft = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cannot read pending task: {e}"))?;
        if draft.team != config.team || draft.board != config.board {
            return Err("A pending task belongs to a different team/board. Reopen Pitwall with that target to resolve it first.".into());
        }
        Ok(Some(Self::from_draft(draft)))
    }
}
pub fn draft_path(config: &Config, channel: &str, thread: Option<&str>) -> PathBuf {
    let target = format!(
        "{}|{}|{}|{}",
        config.team,
        config.board,
        channel,
        thread.unwrap_or("new")
    );
    let hash = target.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    config
        .state_dir()
        .join("drafts")
        .join(format!("{hash:016x}.json"))
}
pub fn pending_path(config: &Config) -> PathBuf {
    config.state_dir().join("pending-task.json")
}
pub fn reply_path(config: &Config) -> PathBuf {
    pending_path(config).with_file_name("pending-reply.json")
}
pub fn clear_reply(config: &Config) -> Result<(), String> {
    clear_file(reply_path(config))
}
pub fn clear_pending(config: &Config) -> Result<(), String> {
    clear_file(pending_path(config))
}
fn clear_file(path: PathBuf) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            team: "test".into(),
            board: "General".into(),
            demo: false,
            node: "node".into(),
            adapter: PathBuf::from("unused"),
            data_dir: None,
        }
    }
    #[test]
    fn installed_draft_and_image_survive_a_release_directory_change() {
        let dir = std::env::temp_dir().join(format!("pitwall-upgrade-{}", std::process::id()));
        let mut cfg = config();
        cfg.data_dir = Some(dir.join("state"));
        cfg.adapter = dir.join("versions/one/board-runtime/adapter.mjs");
        let mut draft = Composer::new("general".into(), &cfg);
        draft.insert("Keep this across upgrades");
        draft
            .add_image(
                image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    8,
                    8,
                    image::Rgba([255, 90, 31, 255]),
                )),
                &cfg,
                &Picker::halfblocks(),
            )
            .unwrap();
        cfg.adapter = dir.join("versions/two/board-runtime/adapter.mjs");
        let loaded = Composer::load_for(&cfg, "general".into(), None, String::new()).unwrap();
        assert_eq!(loaded.draft.key, draft.draft.key);
        assert_eq!(loaded.draft.text, "Keep this across upgrades");
        assert!(loaded.draft.images[0]
            .path
            .starts_with(dir.join("state/images")));
        assert!(loaded.draft.images[0].path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn textarea_selection_undo_and_restore_keep_the_persisted_text_in_sync() {
        let mut editor = Composer::new("general".into(), &config());
        editor.insert("Hello 🌱\nSecond line");
        editor.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        editor.insert("Replacement");
        assert_eq!(editor.draft.text, "Replacement");
        // The library records deleting a selection and inserting its replacement
        // as separate edits. Both undo steps must keep the saved draft in sync.
        editor.key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert_eq!(editor.draft.text, "");
        editor.key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert_eq!(editor.draft.text, "Hello 🌱\nSecond line");
        editor.key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        editor.key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(editor.draft.text, "Replacement");
        let restored = Composer::from_draft(editor.draft.clone());
        assert_eq!(restored.textarea.lines().join("\n"), editor.draft.text);
    }
    #[test]
    fn textarea_clicks_account_for_scrolling_and_wrapping() {
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};
        let mut editor = Composer::new("general".into(), &config());
        editor.insert(
            &(0..20)
                .map(|i| format!("line {i:02}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        terminal
            .draw(|f| f.render_widget(&editor.textarea, Rect::new(3, 2, 20, 3)))
            .unwrap();
        assert_eq!(editor.textarea.scroll_offset().0, 17);
        editor.cursor_at(0, 0);
        editor.insert("X");
        assert!(editor
            .draft
            .text
            .lines()
            .nth(17)
            .unwrap()
            .starts_with("Xline"));
        let mut editor = Composer::new("general".into(), &config());
        editor.insert("abcdefghijklmnopqrstuvwxyz");
        terminal
            .draw(|f| f.render_widget(&editor.textarea, Rect::new(3, 2, 8, 6)))
            .unwrap();
        editor.cursor_at(1, 0);
        editor.insert("X");
        assert_eq!(editor.draft.text, "abcdefghXijklmnopqrstuvwxyz");
    }
    #[test]
    fn reply_draft_keeps_its_thread_and_does_not_replace_new_task_draft() {
        let mut cfg = config();
        let dir = std::env::temp_dir().join(format!("sprout-reply-draft-{}", std::process::id()));
        cfg.adapter = dir.join("board-runtime/adapter.mjs");
        let mut task = Composer::new("general".into(), &cfg);
        task.insert("Unsent task");
        task.draft.locked = true;
        task.save(&cfg).unwrap();
        let mut reply = Composer::reply(
            "general".into(),
            "original-thread".into(),
            "Original title".into(),
            &cfg,
        );
        reply.insert("Please change the wording.");
        reply.draft.locked = true;
        reply.save(&cfg).unwrap();
        let recovered = Composer::load_for(
            &cfg,
            "general".into(),
            Some("original-thread".into()),
            "Original title".into(),
        )
        .unwrap();
        assert_eq!(recovered.draft.thread.as_deref(), Some("original-thread"));
        assert!(recovered.draft.locked);
        assert_eq!(recovered.draft.key, reply.draft.key);
        assert_eq!(
            Composer::load_for(&cfg, "general".into(), None, String::new())
                .unwrap()
                .draft
                .text,
            "Unsent task"
        );
        clear_reply(&cfg).unwrap();
        assert!(!reply_path(&cfg).exists());
        assert!(pending_path(&cfg).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn unicode_editing_and_normal_text_shortcuts() {
        let mut c = Composer::new("general".into(), &config());
        c.insert("A🦀B");
        c.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        c.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(c.draft.text, "AB");
        c.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(c.draft.text, "AqB");
        c.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(c.draft.text, "AqB");
        c.draft.locked = true;
        let key = c.draft.key.clone();
        c.insert("changed");
        assert_eq!(c.draft.text, "AqB");
        assert_eq!(c.draft.key, key);
    }
    #[test]
    fn text_controls_and_paste_limit() {
        let mut c = Composer::new("general".into(), &config());
        c.insert(" \n");
        c.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(c.draft.text, " \n");
        c.insert(&"x".repeat(9000));
        assert_eq!(c.draft.text.chars().count(), 8000);
    }
    #[test]
    fn pending_draft_keeps_target_and_key_across_restart() {
        let mut cfg = config();
        let dir = std::env::temp_dir().join(format!("sprout-draft-test-{}", std::process::id()));
        cfg.adapter = dir.join("board-runtime/adapter.mjs");
        let mut c = Composer::new("general".into(), &cfg);
        c.insert("Test task");
        c.draft.locked = true;
        c.save(&cfg).unwrap();
        let recovered = Composer::load_for(&cfg, "general".into(), None, String::new()).unwrap();
        assert_eq!(recovered.draft.key, c.draft.key);
        assert_eq!(recovered.draft.text, c.draft.text);
        cfg.team = "other".into();
        assert!(Composer::load_for(&cfg, "general".into(), None, String::new()).is_err());
        cfg.team = "test".into();
        clear_pending(&cfg).unwrap();
        assert!(!pending_path(&cfg).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
