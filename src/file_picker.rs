//! A file chooser inside the terminal; it never blocks the event loop.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

pub struct FilePicker {
    pub directory: PathBuf,
    pub directories_only: bool,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    pub input: String,
    pub error: Option<String>,
}
impl FilePicker {
    pub fn new(directory: PathBuf) -> Self {
        let mut picker = Self {
            directory,
            directories_only: false,
            entries: vec![],
            selected: 0,
            input: String::new(),
            error: None,
        };
        picker.reload();
        picker
    }
    pub fn folder(directory: PathBuf) -> Self {
        let mut picker = Self::new(directory);
        picker.directories_only = true;
        picker.reload();
        picker
    }
    fn reload(&mut self) {
        self.entries.clear();
        self.selected = 0;
        self.error = None;
        match std::fs::read_dir(&self.directory) {
            Ok(entries) => {
                self.entries = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_dir()
                            || (!self.directories_only
                                && p.extension().is_some_and(|e| {
                                    matches!(
                                        e.to_string_lossy().to_ascii_lowercase().as_str(),
                                        "png" | "jpg" | "jpeg"
                                    )
                                }))
                    })
                    .collect();
                self.entries.sort_by_key(|p| {
                    (
                        !p.is_dir(),
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase(),
                    )
                });
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub fn up(&mut self) {
        if let Some(parent) = self.directory.parent() {
            self.directory = parent.into();
            self.input.clear();
            self.reload();
        }
    }
    pub fn choose(&mut self, index: usize) -> Option<PathBuf> {
        let path = self.entries.get(index)?.clone();
        self.open(path)
    }
    fn open(&mut self, path: PathBuf) -> Option<PathBuf> {
        if path.is_dir() {
            self.directory = path;
            self.input.clear();
            self.reload();
            None
        } else if path.is_file() {
            Some(path)
        } else {
            self.error = Some("Choose an existing image or directory.".into());
            None
        }
    }
    pub fn accept(&mut self) -> Option<PathBuf> {
        if self.input.trim().is_empty() {
            return if self.directories_only {
                self.directory.canonicalize().ok()
            } else {
                self.choose(self.selected)
            };
        }
        let text = self.input.trim().trim_matches(['\'', '"']);
        let path = if let Some(tail) = text.strip_prefix("~/") {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(tail)
        } else {
            PathBuf::from(text)
        };
        let path = if path.is_absolute() {
            path
        } else {
            self.directory.join(path)
        };
        if self.directories_only {
            if path.is_dir() {
                return path.canonicalize().ok();
            }
            self.error = Some("Choose an existing project folder.".into());
            None
        } else {
            self.open(path)
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<PathBuf> {
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.entries.len().saturating_sub(1))
            }
            KeyCode::Left if self.input.is_empty() => self.up(),
            KeyCode::Enter => {
                if self.directories_only
                    && self.input.is_empty()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return self.choose(self.selected);
                }
                return self.accept();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char('a')
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
            {
                self.input.clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
            {
                self.input.push(c)
            }
            _ => {}
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_picker_selects_only_an_explicit_folder() {
        let root = std::env::temp_dir().join(format!("pitwall-folder-{}", std::process::id()));
        let project = root.join("project with spaces");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(root.join("file.png"), b"not a folder").unwrap();
        let mut picker = FilePicker::folder(root.clone());
        assert_eq!(picker.entries, vec![project.clone()]);
        assert!(picker.choose(0).is_none());
        assert_eq!(picker.accept(), Some(project.canonicalize().unwrap()));
        picker.input = root.join("file.png").to_string_lossy().into();
        assert!(picker.accept().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn browse_and_choose_a_unicode_path_with_spaces() {
        let dir = std::env::temp_dir().join(format!("sprout-picker-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pictures")).unwrap();
        let path = dir.join("pictures/测试 image.PNG");
        std::fs::write(&path, b"test").unwrap();
        std::fs::write(dir.join("ignore.txt"), b"test").unwrap();
        let mut picker = FilePicker::new(dir.clone());
        assert_eq!(picker.entries.len(), 1);
        assert!(picker.choose(0).is_none());
        assert_eq!(picker.choose(0), Some(path.clone()));
        picker.up();
        picker.input = path.to_string_lossy().into();
        assert_eq!(picker.accept(), Some(path));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
