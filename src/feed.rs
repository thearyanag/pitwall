//! One long-lived SDK feed process, independent of cancellable snapshot requests.
use crate::board::Config;
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedState {
    Connecting,
    Live,
    Retrying,
    Failed,
    Demo,
}
impl std::fmt::Display for FeedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Connecting => "connecting",
            Self::Live => "live",
            Self::Retrying => "reconnecting",
            Self::Failed => "disconnected",
            Self::Demo => "demo",
        })
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Notice {
    State {
        state: FeedState,
    },
    Changed {
        channel: String,
        thread: String,
        parent: Option<String>,
        root: bool,
    },
    Reset,
    Error {
        message: String,
    },
}
pub struct Feed {
    child: Child,
    reader: Option<thread::JoinHandle<()>>,
    pub rx: mpsc::Receiver<Notice>,
}
impl Feed {
    pub fn start(config: &Config) -> Result<Self, String> {
        let mut child = Command::new(&config.node)
            .arg(&config.adapter)
            .arg("-")
            .env("SPROUT_STATE_DIR", config.state_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot start Board feed: {e}"))?;
        let request = serde_json::json!({"op":"events","team":config.team,"board":config.board});
        if let Err(e) = child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                match line
                    .ok()
                    .and_then(|l| serde_json::from_str::<Notice>(&l).ok())
                {
                    Some(notice) => {
                        if tx.send(notice).is_err() {
                            return;
                        }
                    }
                    None => {
                        let _ = tx.send(Notice::Error {
                            message: "Board feed returned invalid data.".into(),
                        });
                        return;
                    }
                }
            }
            let _ = tx.send(Notice::Error {
                message: "Board feed closed. Press r to reconnect.".into(),
            });
        });
        Ok(Self {
            child,
            reader: Some(reader),
            rx,
        })
    }
}
impl Drop for Feed {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
#[derive(Default)]
pub struct Refresh {
    pub init: bool,
    pub list: bool,
    pub thread: bool,
}
