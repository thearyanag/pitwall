//! Typed UI navigation and worker status; strings are only presentation/wire values.
use crate::board::Config;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Channels,
    Tasks,
    Conversation,
    Composer,
}
impl Focus {
    pub fn next(self) -> Self {
        match self {
            Self::Channels => Self::Tasks,
            Self::Tasks => Self::Conversation,
            Self::Conversation => Self::Composer,
            Self::Composer => Self::Channels,
        }
    }
    pub fn previous(self) -> Self {
        match self {
            Self::Channels => Self::Composer,
            Self::Tasks => Self::Channels,
            Self::Conversation => Self::Tasks,
            Self::Composer => Self::Conversation,
        }
    }
    pub fn left(self) -> Self {
        match self {
            Self::Channels => Self::Channels,
            _ => self.previous(),
        }
    }
    pub fn right(self) -> Self {
        match self {
            Self::Channels => Self::Tasks,
            _ => Self::Conversation,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    Tasks,
    Threads,
}
impl ViewMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Tasks => Self::Threads,
            Self::Threads => Self::Tasks,
        }
    }
}
impl fmt::Display for ViewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tasks => "tasks",
            Self::Threads => "threads",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Waiting,
    Ready,
    Claimed,
    Completed,
    Blocked,
    Failed,
    Canceled,
    Thread,
    #[serde(other)]
    Unknown,
}
impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Waiting => "waiting",
            Self::Ready => "ready",
            Self::Claimed => "claimed",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Thread => "thread",
            Self::Unknown => "unknown",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerState {
    Waiting,
    Working,
    Stopping,
    Stopped,
    Error,
    #[serde(other)]
    Unknown,
}
#[derive(Deserialize)]
struct Heartbeat {
    state: WorkerState,
    team: String,
    board: String,
    updated_at: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStatus {
    Online(WorkerState),
    Offline,
    OtherBoard,
    Demo,
}
impl WorkerStatus {
    pub fn read(config: &Config) -> Self {
        if config.demo {
            return Self::Demo;
        }
        let Some(heartbeat) = std::fs::read(config.state_dir().join("status.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Heartbeat>(&bytes).ok())
        else {
            return Self::Offline;
        };
        if heartbeat.team != config.team
            || heartbeat.board
                != if config.board.is_empty() {
                    "General"
                } else {
                    &config.board
                }
        {
            return Self::OtherBoard;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now.saturating_sub(heartbeat.updated_at) > 15 {
            return Self::Offline;
        }
        Self::Online(heartbeat.state)
    }
}
impl fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Demo => "demo",
            Self::Offline => "offline",
            Self::OtherBoard => "offline for this Board",
            Self::Online(s) => match s {
                WorkerState::Waiting => "waiting",
                WorkerState::Working => "working",
                WorkerState::Stopping => "stopping",
                WorkerState::Stopped => "stopped",
                WorkerState::Error => "error",
                WorkerState::Unknown => "unknown",
            },
        };
        write!(f, "Worker: {label}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_heartbeat_is_scoped_fresh_and_typed() {
        let root = std::env::temp_dir().join(format!("sprout-status-{}", std::process::id()));
        let config = Config {
            team: "test".into(),
            board: "General".into(),
            demo: false,
            node: "node".into(),
            adapter: root.join("board-runtime/adapter.mjs"),
            data_dir: None,
        };
        std::fs::create_dir_all(config.state_dir()).unwrap();
        let path = config.state_dir().join("status.json");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut data =
            serde_json::json!({"state":"working","team":"test","board":"General","updated_at":now});
        std::fs::write(&path, data.to_string()).unwrap();
        assert_eq!(
            WorkerStatus::read(&config),
            WorkerStatus::Online(WorkerState::Working)
        );
        data["team"] = serde_json::json!("another-team");
        std::fs::write(&path, data.to_string()).unwrap();
        assert_eq!(WorkerStatus::read(&config), WorkerStatus::OtherBoard);
        data["team"] = serde_json::json!("test");
        data["updated_at"] = serde_json::json!(0);
        std::fs::write(&path, data.to_string()).unwrap();
        assert_eq!(WorkerStatus::read(&config), WorkerStatus::Offline);
        std::fs::remove_dir_all(root).unwrap();
    }
}
