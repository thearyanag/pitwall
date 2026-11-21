//! Board state, user actions and the responsive event loop.
use crate::state::{Focus, TaskState, ViewMode};
use crate::{
    board::{
        Attachment, Channel, Config, Conversation, CreatedData, DownloadedImage, InitData, Item,
        ItemsData, Request, Response, Work, Worker,
    },
    composer,
    feed::{Feed, FeedState, Notice, Refresh},
    file_picker, images, ui,
};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::{widgets::ListState, DefaultTerminal};
use ratatui_image::picker::{Picker, ProtocolType};
use std::{
    env, io,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Pick(usize),
    PickerAccept,
    PickerUp,
    PickerClose,
    NewTask,
    Refresh,
    View,
    More,
    Send,
    Paste,
    Attach,
    Mode(bool),
    Expand(usize),
    Remove(usize),
    NextImages,
    ClosePreview,
}
pub(crate) struct App {
    pub(crate) config: Config,
    pub(crate) worker: Worker,
    feed: Option<Feed>,
    pub(crate) feed_state: FeedState,
    pub(crate) feed_error: Option<String>,
    live_refresh: Refresh,
    last_live_refresh: Instant,
    pub(crate) channels: Vec<Channel>,
    pub(crate) items: Vec<Item>,
    pub(crate) thread: Option<Conversation>,
    pub(crate) channel: usize,
    pub(crate) item: usize,
    pub(crate) focus: Focus,
    pub(crate) scroll: u16,
    pub(crate) follow_latest: bool,
    pub(crate) max_scroll: u16,
    pub(crate) page_height: u16,
    pub(crate) channels_state: ListState,
    pub(crate) items_state: ListState,
    pub(crate) identity: String,
    pub(crate) mode: ViewMode,
    pub(crate) next: Option<String>,
    pub(crate) generation: u64,
    pub(crate) pending: Option<Request>,
    pub(crate) error: Option<String>,
    pub(crate) loaded: bool,
    pub(crate) refreshed: Instant,
    pub(crate) light: bool,
    pub(crate) help: bool,
    pub(crate) composer: Option<composer::Composer>,
    pub(crate) file_picker: Option<file_picker::FilePicker>,
    pub(crate) image_picker: Picker,
    pub(crate) graphics: images::KittyRenderer,
    pub(crate) image_open: bool,
    pub(crate) image_index: usize,
    pub(crate) preview: Option<images::Preview>,
    pub(crate) logo: Option<images::Preview>,
    logo_light: Option<bool>,
    pub(crate) new_task: bool,
    pub(crate) ui: ui::Map,
    pub(crate) toast: Option<(String, Instant)>,
}
impl App {
    pub(crate) fn new(config: Config) -> Self {
        let worker = Worker::new(config.clone());
        Self {
            config,
            worker,
            feed: None,
            feed_state: FeedState::Connecting,
            feed_error: None,
            live_refresh: Refresh::default(),
            last_live_refresh: Instant::now(),
            channels: vec![],
            items: vec![],
            thread: None,
            channel: 0,
            item: 0,
            focus: Focus::Channels,
            scroll: 0,
            follow_latest: true,
            max_scroll: 0,
            page_height: 1,
            channels_state: ListState::default(),
            items_state: ListState::default(),
            identity: "Connecting…".into(),
            mode: ViewMode::Tasks,
            next: None,
            generation: 0,
            pending: None,
            error: None,
            loaded: false,
            refreshed: Instant::now(),
            light: false,
            help: false,
            composer: None,
            file_picker: None,
            image_picker: Picker::halfblocks(),
            graphics: images::KittyRenderer::default(),
            image_open: false,
            image_index: 0,
            preview: None,
            logo: None,
            logo_light: None,
            new_task: false,
            ui: ui::Map::default(),
            toast: None,
        }
    }
    fn start_feed(&mut self) {
        self.feed = None;
        self.feed_error = None;
        self.live_refresh = Refresh::default();
        if self.config.demo {
            self.feed_state = FeedState::Demo;
            return;
        }
        self.feed_state = FeedState::Connecting;
        match Feed::start(&self.config) {
            Ok(feed) => self.feed = Some(feed),
            Err(message) => self.apply_notice(Notice::Error { message }),
        }
    }
    fn apply_notice(&mut self, notice: Notice) {
        match notice {
            Notice::State { state } => {
                self.feed_state = state;
                self.feed_error = None;
                if state == FeedState::Live {
                    self.live_refresh.init = true;
                }
            }
            Notice::Reset => self.live_refresh.init = true,
            Notice::Error { message } => {
                self.feed_state = FeedState::Failed;
                if self.feed_error.is_none() {
                    self.feed_error = Some(format!("{message} Press r to reconnect."));
                }
            }
            Notice::Changed {
                channel,
                thread,
                parent,
                root,
            } => {
                if channel != "sprout-runs" && !self.channels.iter().any(|c| c.path == channel) {
                    self.live_refresh.init = true;
                }
                let selected = self.item_id();
                if selected.as_deref() == Some(&thread) || (parent.is_some() && parent == selected)
                {
                    self.live_refresh.thread = true;
                    if root && parent.is_none() {
                        self.live_refresh.list = true;
                    }
                } else if self.channel_path().as_deref() == Some(&channel) {
                    self.live_refresh.list = true;
                }
            }
        }
    }
    fn expire_leases(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        for item in &mut self.items {
            if item.state == TaskState::Claimed
                && item.lease_until.is_some_and(|until| until <= now)
            {
                item.state = TaskState::Ready;
                item.lease_until = None;
            }
        }
        if let Some(thread) = self.thread.as_mut() {
            if thread.state == TaskState::Claimed
                && thread.lease_until.is_some_and(|until| until <= now)
            {
                thread.state = TaskState::Ready;
                thread.lease_until = None;
                self.live_refresh.thread = true;
            }
        }
    }
    fn refresh_from_feed(&mut self) {
        if self.pending.is_some()
            || self.composer.as_ref().is_some_and(|e| e.submitting)
            || self.image_open
        {
            return;
        }
        if self.live_refresh.init {
            self.live_refresh = Refresh::default();
            self.request(Request::Init);
        } else if self.live_refresh.list {
            self.live_refresh.list = false;
            self.live_refresh.thread = false;
            self.load_items(false);
        } else if self.live_refresh.thread {
            self.live_refresh.thread = false;
            self.load_thread();
        }
        self.last_live_refresh = Instant::now();
    }
    fn channel_path(&self) -> Option<String> {
        self.channels.get(self.channel).map(|c| c.path.clone())
    }
    fn item_id(&self) -> Option<String> {
        self.items.get(self.item).map(|i| i.id.clone())
    }
    fn request(&mut self, request: Request) {
        self.generation += 1;
        self.error = None;
        self.pending = Some(request.clone());
        if self
            .worker
            .send(Work {
                generation: self.generation,
                request,
            })
            .is_err()
        {
            self.pending = None;
            self.error = Some("Board worker stopped. Restart Pitwall.".into());
        }
    }
    fn load_items(&mut self, more: bool) {
        if let Some(channel) = self.channel_path() {
            self.request(Request::List {
                channel,
                mode: self.mode,
                cursor: if more { self.next.clone() } else { None },
                keep_selected: if more {
                    None
                } else {
                    self.items.get(self.item).cloned()
                },
            });
        }
    }
    fn load_thread(&mut self) {
        if let Some(id) = self.item_id() {
            if self.thread.as_ref().map(|t| &t.id) != Some(&id) {
                self.thread = None;
                self.scroll = 0;
                self.follow_latest = true;
            }
            self.request(Request::Thread { id });
        }
    }
    fn apply(&mut self, response: Response) {
        // A late response for a previous selection must never replace the visible thread.
        if response.generation != self.generation {
            return;
        }
        self.pending = None;
        let is_reply = matches!(response.request, Request::Reply { .. });
        let comment = matches!(
            response.request,
            Request::Reply {
                run_worker: false,
                ..
            }
        );
        let data = match response.data {
            Ok(data) => data,
            Err(error) => {
                if let Some(editor) = self.composer.as_mut() {
                    if matches!(
                        response.request,
                        Request::Create { .. } | Request::Reply { .. }
                    ) {
                        editor.submitting = false;
                        editor.error = Some(error.clone());
                    }
                }
                self.error = Some(error);
                return;
            }
        };
        let result: Result<(), String> = (|| {
            match response.request {
                Request::Image { .. } => {
                    let image: DownloadedImage =
                        serde_json::from_value(data).map_err(|e| format!("Image response: {e}"))?;
                    self.preview = Some(images::Preview::load(
                        &image.path,
                        image.name,
                        &self.image_picker,
                    )?);
                }
                Request::Create { key, .. } | Request::Reply { key, .. } => {
                    let created: CreatedData = serde_json::from_value(data)
                        .map_err(|e| format!("Task creation response: {e}"))?;
                    if let Err(error) = if is_reply {
                        composer::clear_reply(&self.config)
                    } else {
                        composer::clear_pending(&self.config)
                    } {
                        return Err(format!(
                            "Task created, but retry file cleanup failed: {error}"
                        ));
                    }
                    if let Some(editor) = self.composer.as_ref() {
                        editor.remove_saved(&self.config)?;
                    }
                    images::cleanup(&self.config, &key);
                    self.composer = None;
                    self.new_task = false;
                    self.toast = Some((
                        if comment {
                            "Comment posted. Codex was not started."
                        } else if is_reply {
                            "Feedback queued for Codex."
                        } else {
                            "Task queued for Codex."
                        }
                        .into(),
                        Instant::now(),
                    ));
                    if self.channel_path().as_deref() != Some(&created.channel)
                        || self.mode != ViewMode::Tasks
                    {
                        self.items.clear();
                        self.next = None;
                    }
                    self.mode = ViewMode::Tasks;
                    if let Some(index) =
                        self.channels.iter().position(|c| c.path == created.channel)
                    {
                        self.channel = index;
                    }
                    self.items.retain(|item| item.id != created.item.id);
                    self.items.insert(0, created.item);
                    self.item = 0;
                    self.focus = Focus::Composer;
                    self.scroll = 0;
                    self.follow_latest = true;
                    self.load_thread();
                }
                Request::Init => {
                    let data: InitData = serde_json::from_value(data)
                        .map_err(|e| format!("Board identity response: {e}"))?;
                    let previous = self.channel_path();
                    self.identity = data.identity;
                    self.channels = data.channels;
                    self.loaded = true;
                    self.channel = previous
                        .as_ref()
                        .and_then(|id| self.channels.iter().position(|c| &c.path == id))
                        .unwrap_or(0);
                    if previous != self.channel_path() {
                        self.save_draft();
                        self.composer = None;
                        self.items.clear();
                        self.thread = None;
                        self.item = 0;
                        self.scroll = 0;
                    }
                    if self.channels.is_empty() {
                        self.items.clear();
                        self.thread = None;
                    } else {
                        self.load_items(false);
                    }
                }
                Request::List {
                    cursor,
                    keep_selected,
                    ..
                } => {
                    let data: ItemsData = serde_json::from_value(data)
                        .map_err(|e| format!("Board list response: {e}"))?;
                    let selected = self.item_id();
                    if cursor.is_none() {
                        self.items.clear();
                    }
                    for item in data.items {
                        if let Some(old) = self.items.iter_mut().find(|i| i.id == item.id) {
                            *old = item;
                        } else {
                            self.items.push(item);
                        }
                    }
                    // A live refresh of page one must not evict the conversation
                    // being read from a later page. Its own snapshot refreshes next.
                    if let Some(item) = keep_selected {
                        if !self.items.iter().any(|i| i.id == item.id) {
                            self.items.push(item);
                        }
                    }
                    self.next = data.next.filter(|next| Some(next) != cursor.as_ref());
                    self.item = selected
                        .as_ref()
                        .and_then(|id| self.items.iter().position(|i| &i.id == id))
                        .unwrap_or(0);
                    if self.items.is_empty() {
                        self.thread = None;
                        self.scroll = 0;
                    } else {
                        self.load_thread();
                    }
                }
                Request::Thread { id } => {
                    let conversation: Conversation = serde_json::from_value(data)
                        .map_err(|e| format!("Board conversation response: {e}"))?;
                    if conversation.id != id {
                        return Err(
                            "Board returned a different conversation than requested.".into()
                        );
                    }
                    if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
                        item.state = conversation.state.clone();
                        item.lease_until = conversation.lease_until;
                        item.replies = conversation.reply_count;
                    }
                    self.thread = Some(conversation);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(editor) = self.composer.as_mut() {
                editor.submitting = false;
                editor.error = Some(error.clone());
            }
            self.error = Some(error);
        }
        self.refreshed = Instant::now();
        self.ensure_composer();
    }
    fn select(&mut self, down: bool) {
        match self.focus {
            Focus::Channels if !self.channels.is_empty() => {
                let next = if down {
                    (self.channel + 1).min(self.channels.len() - 1)
                } else {
                    self.channel.saturating_sub(1)
                };
                self.select_channel(next);
            }
            Focus::Tasks if !self.items.is_empty() => {
                let next = if down {
                    (self.item + 1).min(self.items.len() - 1)
                } else {
                    self.item.saturating_sub(1)
                };
                self.select_task(next);
            }
            Focus::Conversation => self.scroll_conversation(down, 1),
            _ => {}
        }
    }
    fn select_channel(&mut self, index: usize) {
        if index >= self.channels.len() {
            return;
        }
        self.focus = Focus::Channels;
        if index == self.channel {
            return;
        }
        self.save_draft();
        self.composer = None;
        self.new_task = false;
        self.channel = index;
        self.item = 0;
        self.items.clear();
        self.thread = None;
        self.next = None;
        self.scroll = 0;
        self.load_items(false);
    }
    fn select_task(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.focus = Focus::Tasks;
        if index == self.item && !self.new_task {
            return;
        }
        self.save_draft();
        self.item = index;
        self.new_task = false;
        self.load_thread();
        self.ensure_composer();
    }
    fn scroll_conversation(&mut self, down: bool, amount: u16) {
        self.scroll = if down {
            self.scroll.saturating_add(amount).min(self.max_scroll)
        } else {
            self.scroll.saturating_sub(amount)
        };
        // Only an explicit return to the bottom resumes following. Layout changes
        // or new messages must not undo a reader's decision to scroll up.
        self.follow_latest = down && self.scroll == self.max_scroll;
    }
    fn ensure_composer(&mut self) {
        if self.composer.as_ref().is_some_and(|e| e.submitting) {
            return;
        }
        let Some(channel) = self.channel_path() else {
            return;
        };
        if self.items.is_empty() && matches!(self.pending, Some(Request::List { .. })) {
            return;
        }
        if self.items.is_empty() {
            self.new_task = true;
        }
        let thread = if self.new_task { None } else { self.item_id() };
        if self
            .composer
            .as_ref()
            .is_some_and(|e| e.draft.channel == channel && e.draft.thread == thread)
        {
            return;
        }
        if let Some(editor) = self.composer.as_ref() {
            if let Err(error) = editor.save(&self.config) {
                self.error = Some(error);
                return;
            }
        }
        let title = if self.new_task {
            String::new()
        } else {
            self.items
                .get(self.item)
                .map(|i| i.title.clone())
                .unwrap_or_default()
        };
        match composer::Composer::load_for(&self.config, channel, thread, title) {
            Ok(mut editor) => {
                editor.refresh_preview(&self.image_picker);
                self.composer = Some(editor);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn save_draft(&mut self) {
        if let Some(editor) = self.composer.as_ref() {
            if let Err(error) = editor.save(&self.config) {
                self.error = Some(error);
            }
        }
    }
    fn open_composer(&mut self) {
        self.new_task = true;
        self.focus = Focus::Composer;
        self.ensure_composer();
    }
    fn open_reply(&mut self) {
        self.new_task = false;
        self.focus = Focus::Composer;
        self.ensure_composer();
    }
    fn attach_files(&mut self) {
        if self
            .composer
            .as_ref()
            .is_none_or(|e| e.draft.locked || e.submitting)
        {
            return;
        }
        let directory = env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join("Desktop"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| env::current_dir().unwrap_or_default());
        self.file_picker = Some(file_picker::FilePicker::new(directory));
        self.focus = Focus::Composer;
    }
    fn picked_file(&mut self, path: Option<PathBuf>) {
        if let Some(path) = path {
            self.file_picker = None;
            self.paste_content(&path.to_string_lossy());
            self.save_draft();
            self.focus = Focus::Composer;
        }
    }
    fn action(&mut self, action: Action) {
        if self.composer.as_ref().is_some_and(|e| e.submitting) {
            return;
        }
        match action {
            Action::Pick(index) => {
                let path = self.file_picker.as_mut().and_then(|p| p.choose(index));
                self.picked_file(path);
            }
            Action::PickerAccept => {
                let path = self.file_picker.as_mut().and_then(|p| p.accept());
                self.picked_file(path);
            }
            Action::PickerUp => {
                if let Some(p) = self.file_picker.as_mut() {
                    p.up();
                }
            }
            Action::PickerClose => self.file_picker = None,
            Action::NewTask => self.open_composer(),
            Action::Refresh => {
                if self.feed_state == FeedState::Failed {
                    self.start_feed();
                }
                self.request(Request::Init);
            }
            Action::View => {
                self.mode = self.mode.toggle();
                self.items.clear();
                self.thread = None;
                self.item = 0;
                self.scroll = 0;
                self.next = None;
                self.load_items(false);
            }
            Action::More => self.load_items(true),
            Action::Send => self.submit_draft(),
            Action::Paste => {
                self.focus = Focus::Composer;
                self.ensure_composer();
                self.paste_clipboard();
                self.save_draft();
            }
            Action::Attach => self.attach_files(),
            Action::Mode(run) => {
                self.focus = Focus::Composer;
                if let Some(editor) = self.composer.as_mut() {
                    if !editor.draft.locked {
                        editor.draft.send_to_codex = run;
                        self.save_draft();
                    }
                }
            }
            Action::Expand(index) => {
                self.focus = Focus::Composer;
                if let Some(editor) = self.composer.as_mut() {
                    editor.image_index = index;
                    editor.refresh_preview(&self.image_picker);
                    editor.expanded = true;
                }
            }
            Action::Remove(index) => {
                self.focus = Focus::Composer;
                if let Some(editor) = self.composer.as_mut() {
                    if !editor.draft.locked && index < editor.draft.images.len() {
                        let removed = editor.draft.images.remove(index);
                        editor
                            .thumbnails
                            .remove(&removed.path.to_string_lossy().to_string());
                        editor.refresh_preview(&self.image_picker);
                        self.save_draft();
                    }
                }
            }
            Action::NextImages => {
                if self.image_open {
                    self.open_image(true);
                } else if let Some(editor) = self.composer.as_mut() {
                    editor.image_index =
                        (editor.image_index + 1) % editor.draft.images.len().max(1);
                    editor.refresh_preview(&self.image_picker);
                }
            }
            Action::ClosePreview => {
                self.image_open = false;
                self.preview = None;
                if let Some(editor) = self.composer.as_mut() {
                    editor.expanded = false;
                }
            }
        }
    }
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.composer.as_ref().is_some_and(|e| e.submitting) {
            return;
        }
        let point = ratatui::layout::Position::new(mouse.column, mouse.row);
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            if let Some(action) = self.ui.action(mouse.column, mouse.row) {
                self.action(action);
                return;
            }
            if self.file_picker.is_some()
                || self.help
                || self.image_open
                || self.composer.as_ref().is_some_and(|e| e.expanded)
            {
                return;
            }
            if self.ui.editor.contains(point) {
                self.focus = Focus::Composer;
                if let Some(editor) = self.composer.as_mut() {
                    editor.cursor_at(
                        (mouse.row - self.ui.editor.y) as usize,
                        (mouse.column - self.ui.editor.x) as usize,
                    );
                }
                return;
            }
            if self.ui.channels.contains(point) {
                let i =
                    self.channels_state.offset() + ((mouse.row - self.ui.channels.y) / 2) as usize;
                self.select_channel(i);
                return;
            }
            if self.ui.tasks.contains(point) {
                let i = self.items_state.offset() + ((mouse.row - self.ui.tasks.y) / 4) as usize;
                self.select_task(i);
                return;
            }
            if self.ui.conversation.contains(point) {
                self.focus = Focus::Conversation;
            }
        }
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
        ) {
            if self.help || self.image_open || self.composer.as_ref().is_some_and(|e| e.expanded) {
                return;
            }
            let down = mouse.kind == MouseEventKind::ScrollDown;
            if let Some(picker) = self.file_picker.as_mut() {
                picker.key(KeyEvent::new(
                    if down { KeyCode::Down } else { KeyCode::Up },
                    KeyModifiers::NONE,
                ));
                return;
            }
            if self.ui.channels.contains(point) {
                self.focus = Focus::Channels;
                self.select(down);
                self.new_task = false;
            } else if self.ui.tasks.contains(point) {
                self.focus = Focus::Tasks;
                self.select(down);
                self.new_task = false;
            } else {
                self.scroll_conversation(down, 3);
            }
            self.ensure_composer();
        }
    }
    pub(crate) fn prepare_logo(&mut self) {
        // Use the tested direct Kitty path; every other terminal keeps the wordmark.
        if self.image_picker.protocol_type() != ProtocolType::Kitty {
            self.logo = None;
            self.logo_light = None;
            return;
        }
        if self.logo_light == Some(self.light) {
            return;
        }
        let name = if self.light {
            "pitwall-mark-on-light.png"
        } else {
            "pitwall-mark-on-dark.png"
        };
        let path = self.config.root_dir().join("assets/brand").join(name);
        self.logo = images::Preview::load(&path, String::new(), &self.image_picker).ok();
        self.logo_light = Some(self.light);
    }
    pub(crate) fn reset_previews(&mut self) {
        for preview in self.logo.iter_mut().chain(self.preview.iter_mut()).chain(
            self.composer.iter_mut().flat_map(|editor| {
                editor
                    .preview
                    .iter_mut()
                    .chain(editor.thumbnails.values_mut())
            }),
        ) {
            preview.frame_area = None;
        }
    }
    fn attached_images(&self) -> Vec<Attachment> {
        self.thread
            .as_ref()
            .map(|t| {
                t.messages
                    .iter()
                    .flat_map(|m| m.attachments.iter())
                    .filter(|a| matches!(a.mime.as_str(), "image/png" | "image/jpeg"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
    fn open_image(&mut self, next: bool) {
        let images = self.attached_images();
        if images.is_empty() {
            self.error = Some("This conversation has no image attachments.".into());
            return;
        }
        self.image_index = if next {
            (self.image_index + 1) % images.len()
        } else {
            0
        };
        self.image_open = true;
        self.preview = None;
        if let Some(thread) = self.item_id() {
            self.request(Request::Image {
                id: images[self.image_index].id.clone(),
                thread,
            });
        }
    }
    fn paste_content(&mut self, text: &str) {
        let Some(editor) = self.composer.as_mut() else {
            return;
        };
        if let Some(path) = images::pasted_image_path(text) {
            let result = images::decode(&path)
                .and_then(|image| editor.add_image(image, &self.config, &self.image_picker));
            if let Err(error) = result {
                editor.error = Some(error);
            }
        } else {
            editor.insert(text);
        }
    }
    fn paste_clipboard(&mut self) {
        match images::clipboard() {
            Ok(images::ClipboardContent::Text(text)) => self.paste_content(&text),
            Ok(images::ClipboardContent::Image(image)) => {
                if let Some(editor) = self.composer.as_mut() {
                    if let Err(error) = editor.add_image(image, &self.config, &self.image_picker) {
                        editor.error = Some(error);
                    }
                }
            }
            Err(error) => {
                if let Some(editor) = self.composer.as_mut() {
                    editor.error = Some(error);
                }
            }
        }
    }
    fn submit_draft(&mut self) {
        let Some(editor) = self.composer.as_mut() else {
            return;
        };
        if editor.submitting || editor.draft.text.trim().is_empty() || self.config.demo {
            return;
        }
        let was_attempted = editor.draft.locked;
        editor.draft.locked = true;
        if let Err(error) = editor.save(&self.config) {
            editor.draft.locked = was_attempted;
            editor.error = Some(format!("Cannot save retry information: {error}"));
            return;
        }
        editor.submitting = true;
        editor.error = None;
        let request = if let Some(thread) = &editor.draft.thread {
            Request::Reply {
                run_worker: editor.draft.send_to_codex,
                thread: thread.clone(),
                text: editor.draft.text.clone(),
                key: editor.draft.key.clone(),
                attachments: editor.draft.images.clone(),
            }
        } else {
            Request::Create {
                channel: editor.draft.channel.clone(),
                text: editor.draft.text.clone(),
                key: editor.draft.key.clone(),
                attachments: editor.draft.images.clone(),
            }
        };
        self.request(request);
    }
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_draft();
            return true;
        }
        if self.composer.as_ref().is_some_and(|e| e.submitting) {
            return false;
        }
        if let Some(picker) = self.file_picker.as_mut() {
            if key.code == KeyCode::Esc {
                self.file_picker = None;
            } else if key.code == KeyCode::Char('v')
                && key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
            {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        picker.input.push_str(&text);
                    }
                }
            } else {
                let path = picker.key(key);
                self.picked_file(path);
            }
            return false;
        }
        if self.help {
            match key.code {
                KeyCode::Char('q') => {
                    self.save_draft();
                    return true;
                }
                KeyCode::Esc | KeyCode::Char('?') => self.help = false,
                _ => {}
            }
            return false;
        }
        if key.code == KeyCode::Char('v')
            && key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
        {
            self.action(Action::Paste);
            return false;
        }
        if (key.code == KeyCode::Enter
            && key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER))
            || (key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            self.submit_draft();
            return false;
        }
        if self.image_open || self.composer.as_ref().is_some_and(|e| e.expanded) {
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('p') {
                self.action(Action::ClosePreview);
            } else if key.code == KeyCode::Tab {
                self.action(Action::NextImages);
            }
            return false;
        }
        if self.focus == Focus::Composer {
            if key.code == KeyCode::Tab {
                self.focus = Focus::Channels;
                return false;
            }
            if key.code == KeyCode::BackTab {
                self.focus = Focus::Conversation;
                return false;
            }
            if let Some(editor) = self.composer.as_mut() {
                match key.code {
                    KeyCode::Esc => self.focus = Focus::Conversation,
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if editor.preview.is_some() {
                            let index = editor.image_index;
                            self.action(Action::Expand(index));
                        }
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let index = editor.image_index;
                        self.action(Action::Remove(index));
                    }
                    _ => editor.key(key),
                }
                self.save_draft();
            }
            return false;
        }
        if key.code == KeyCode::Enter {
            self.focus = Focus::Composer;
            self.ensure_composer();
            return false;
        }
        match key.code {
            KeyCode::Char('q') => {
                self.save_draft();
                return true;
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('n') => self.action(Action::NewTask),
            KeyCode::Char('c') => self.open_reply(),
            KeyCode::Char('p') => self.open_image(false),
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::BackTab => self.focus = self.focus.previous(),
            KeyCode::Right | KeyCode::Char('l') => self.focus = self.focus.right(),
            KeyCode::Left | KeyCode::Char('h') => self.focus = self.focus.left(),
            KeyCode::Down | KeyCode::Char('j') => self.select(true),
            KeyCode::Up | KeyCode::Char('k') => self.select(false),
            KeyCode::Char('r') => self.action(Action::Refresh),
            KeyCode::Char('d') => {
                self.config.demo = !self.config.demo;
                self.start_feed();
                self.worker = Worker::new(self.config.clone());
                self.channels.clear();
                self.items.clear();
                self.thread = None;
                self.channel = 0;
                self.item = 0;
                self.scroll = 0;
                self.next = None;
                self.loaded = false;
                self.identity = "Connecting…".into();
                self.request(Request::Init);
            }
            KeyCode::Char('v') => self.action(Action::View),
            KeyCode::Char('m') if self.next.is_some() && self.pending.is_none() => {
                self.action(Action::More)
            }
            KeyCode::Char('t') => self.light = !self.light,
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.focus = Focus::Conversation;
                self.scroll_conversation(true, self.page_height);
            }
            KeyCode::PageUp => {
                self.focus = Focus::Conversation;
                self.scroll_conversation(false, self.page_height);
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.follow_latest = true;
                self.scroll = self.max_scroll;
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.follow_latest = false;
                self.scroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.follow_latest = true;
                self.scroll = self.max_scroll;
            }
            _ => {}
        }
        self.ensure_composer();
        false
    }
}

pub(crate) fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    app.start_feed();
    app.request(Request::Init);
    let mut last_paint = Instant::now();
    let mut dirty = true;
    loop {
        while let Ok(response) = app.worker.rx.try_recv() {
            app.apply(response);
            dirty = true;
        }
        while let Some(notice) = app.feed.as_ref().and_then(|feed| feed.rx.try_recv().ok()) {
            app.apply_notice(notice);
            dirty = true;
        }
        if app.last_live_refresh.elapsed() >= Duration::from_millis(250) {
            app.expire_leases();
            app.refresh_from_feed();
        }
        if dirty || last_paint.elapsed() >= Duration::from_secs(1) {
            app.reset_previews();
            terminal.draw(|f| ui::draw(f, app))?;
            let previews = app
                .logo
                .iter()
                .chain(app.preview.iter())
                .chain(
                    app.composer
                        .iter()
                        .flat_map(|editor| editor.preview.iter().chain(editor.thumbnails.values())),
                )
                .collect::<Vec<_>>();
            app.graphics.render_many(&previews)?;
            last_paint = Instant::now();
            dirty = false;
        }
        if event::poll(Duration::from_millis(75))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if app.handle_key(key) {
                        return Ok(());
                    }
                    dirty = true;
                }
                Event::Paste(text) => {
                    if let Some(picker) = app.file_picker.as_mut() {
                        picker.input.push_str(&text);
                        dirty = true;
                        continue;
                    }
                    app.focus = Focus::Composer;
                    app.ensure_composer();
                    app.paste_content(&text);
                    app.save_draft();
                    dirty = true;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                    dirty = true;
                }
                Event::Resize(_, _) | Event::FocusGained => {
                    // Rex may clear graphic placements even when their cell
                    // rectangle is unchanged. Retransmit on resize/tab focus.
                    app.graphics.render_many(&[])?;
                    dirty = true;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
