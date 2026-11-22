//! Local worker permission requests. Credentials and decisions never enter Board.
use crate::board::Config;
use serde::Deserialize;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Deserialize)]
pub struct Approval {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub options: Vec<Choice>,
    pub provider: String,
    pub task: String,
    team: String,
    board: String,
    expires: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl Approval {
    pub fn read(config: &Config) -> Option<Self> {
        if config.demo {
            return None;
        }
        let directory = config.state_dir().join("approvals");
        let mut paths = fs::read_dir(directory)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with(".request.json"))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let Some(bytes) = fs::read(&path).ok() else {
                continue;
            };
            let Ok(mut request) = serde_json::from_slice::<Self>(&bytes) else {
                continue;
            };
            if request.id.len() != 36
                || !request
                    .id
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == '-')
            {
                continue;
            }
            if path.file_name()?.to_str()? != format!("{}.request.json", request.id) {
                continue;
            }
            if request.team != config.team
                || request.board
                    != if config.board.is_empty() {
                        "General"
                    } else {
                        &config.board
                    }
                || request.expires <= now()
                || path
                    .with_file_name(format!("{}.response.json", request.id))
                    .exists()
            {
                continue;
            }
            // Render untrusted tool text as text, never terminal control sequences.
            fn clean(text: &mut String) {
                text.retain(|c| !c.is_control() || c == '\n' || c == '\t');
            }
            clean(&mut request.title);
            clean(&mut request.detail);
            clean(&mut request.provider);
            clean(&mut request.task);
            for choice in &mut request.options {
                clean(&mut choice.label);
            }
            return Some(request);
        }
        None
    }
    pub fn respond(&self, config: &Config, choice: &str) -> Result<(), String> {
        if self.expires <= now() {
            return Err("This permission request expired.".into());
        }
        if !choice.is_empty() && !self.options.iter().any(|o| o.id == choice) {
            return Err("Unknown permission choice.".into());
        }
        let path = config
            .state_dir()
            .join("approvals")
            .join(format!("{}.response.json", self.id));
        let tmp = path.with_extension("tmp");
        fs::write(
            &tmp,
            serde_json::json!({"id":self.id, "choice":choice}).to_string(),
        )
        .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}
