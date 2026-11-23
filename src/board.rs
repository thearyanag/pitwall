//! SDK bridge. A worker thread runs Node; keyboard input stays responsive.
use crate::state::{TaskState, ViewMode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Config {
    pub team: String,
    pub board: String,
    pub demo: bool,
    pub node: String,
    pub adapter: PathBuf,
    pub data_dir: Option<PathBuf>,
}
impl Config {
    pub fn root_dir(&self) -> PathBuf {
        self.adapter
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| std::path::Path::new("."))
            .into()
    }
    pub fn state_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(|| self.root_dir().join("worker"))
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    pub path: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    #[serde(default)]
    pub lease_until: Option<u64>,
    pub author: String,
    pub replies: usize,
    pub labels: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub id: String,
    pub author: String,
    pub time: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
}
#[derive(Deserialize)]
pub struct DownloadedImage {
    pub path: PathBuf,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub messages: Vec<Message>,
    pub state: TaskState,
    #[serde(default)]
    pub lease_until: Option<u64>,
    pub details: Vec<String>,
    #[serde(default)]
    pub reply_count: usize,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub label: String,
    pub installed: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ModelChoice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Deserialize)]
pub struct ModelCatalog {
    pub provider: String,
    pub models: Vec<ModelChoice>,
}
#[derive(Deserialize)]
pub struct InitData {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub providers: Vec<AgentInfo>,
    pub identity: String,
    pub channels: Vec<Channel>,
}
#[derive(Deserialize)]
pub struct ItemsData {
    pub items: Vec<Item>,
    pub next: Option<String>,
}
#[derive(Deserialize)]
pub struct CreatedData {
    pub item: Item,
    pub channel: String,
}

#[derive(Clone, Debug)]
pub enum Request {
    Init,
    OpenProject {
        path: PathBuf,
    },
    Models {
        provider: String,
        project: Option<String>,
    },
    List {
        channel: String,
        mode: ViewMode,
        cursor: Option<String>,
        keep_selected: Option<Item>,
    },
    Thread {
        id: String,
    },
    Create {
        project: Option<String>,
        provider: String,
        model: Option<String>,
        channel: String,
        text: String,
        key: String,
        attachments: Vec<crate::images::LocalImage>,
    },
    Reply {
        project: Option<String>,
        provider: String,
        model: Option<String>,
        run_worker: bool,
        thread: String,
        text: String,
        key: String,
        attachments: Vec<crate::images::LocalImage>,
    },
    Image {
        id: String,
        thread: String,
    },
}
pub struct Work {
    pub generation: u64,
    pub request: Request,
}
pub struct Response {
    pub generation: u64,
    pub request: Request,
    pub data: Result<Value, String>,
}
pub struct Worker {
    tx: mpsc::Sender<Work>,
    pub rx: mpsc::Receiver<Response>,
    stopped: Arc<AtomicBool>,
    latest: Arc<AtomicU64>,
    handle: Option<thread::JoinHandle<()>>,
}

fn fetch(
    config: &Config,
    req: &Request,
    generation: u64,
    stopped: &AtomicBool,
    latest: &AtomicU64,
) -> Result<Value, String> {
    let mut input = match req {
        Request::Init => json!({"op":"init"}),
        Request::OpenProject { path } => json!({"path":path}),
        Request::Models { provider, project } => {
            json!({"op":"models", "provider":provider, "project":project})
        }
        Request::List {
            channel,
            mode,
            cursor,
            ..
        } => json!({"op":"list","channel":channel,"mode":mode,"cursor":cursor}),
        Request::Thread { id } => json!({"op":"thread","id":id}),
        Request::Create {
            project,
            provider,
            model,
            channel,
            text,
            key,
            attachments,
        } => {
            json!({"op":"create","project":project,"provider":provider,"model":model,"channel":channel,"text":text,"key":key,"attachments":attachments})
        }
        Request::Image { id, thread } => json!({"op":"image","id":id,"thread":thread}),
        Request::Reply {
            project,
            provider,
            model,
            run_worker,
            thread,
            text,
            key,
            attachments,
        } => {
            json!({"op":"reply","project":project,"provider":provider,"model":model,"thread":thread,"text":text,"key":key,"attachments":attachments,"run_worker":run_worker})
        }
    };
    input["team"] = json!(config.team);
    input["board"] = json!(config.board);
    input["demo"] = json!(config.demo);
    let mut child = Command::new(&config.node)
        .arg(match req {
            Request::Models { .. } => config.adapter.with_file_name("discover-models.mjs"),
            Request::OpenProject { .. } => config.adapter.with_file_name("open-project.mjs"),
            _ => config.adapter.clone(),
        })
        .arg("-")
        .env("SPROUT_STATE_DIR", config.state_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!("Cannot start Board adapter: {e}. Check Node.js and npm dependencies.")
        })?;
    if let Err(error) = child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("Cannot send Board request: {error}"));
    }
    let mut stdout = child.stdout.take().unwrap();
    // Drain concurrently: large conversations must not fill the child's output pipe.
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    loop {
        if stopped.load(Ordering::Relaxed) || latest.load(Ordering::Relaxed) != generation {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("Request canceled".into());
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None)
                if started.elapsed()
                    < Duration::from_secs(
                        if matches!(req, Request::Create { .. } | Request::Reply { .. }) {
                            125
                        } else {
                            23
                        },
                    ) =>
            {
                thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("Board adapter timed out. Press r to retry.".into());
            }
        }
    }
    let bytes = reader
        .join()
        .map_err(|_| "Board adapter output failed")?
        .map_err(|e| e.to_string())?;
    let response: Value = serde_json::from_slice(&bytes).map_err(|_| {
        "Board adapter returned no valid JSON. Check Node.js and board-runtime dependencies."
            .to_string()
    })?;
    if response["ok"] != true {
        return Err(response["error"]
            .as_str()
            .unwrap_or("Board request failed")
            .to_string());
    }
    Ok(response["data"].clone())
}

impl Worker {
    pub fn new(config: Config) -> Self {
        let (tx, requests) = mpsc::channel::<Work>();
        let (results, rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(AtomicU64::new(0));
        let stop = stopped.clone();
        let newest = latest.clone();
        let handle = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let mut work = match requests.recv_timeout(Duration::from_millis(50)) {
                    Ok(work) => work,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                // Navigation can queue several requests. Execute only the most recent selection.
                while let Ok(newer) = requests.try_recv() {
                    work = newer;
                }
                let data = fetch(&config, &work.request, work.generation, &stop, &newest);
                if results
                    .send(Response {
                        generation: work.generation,
                        request: work.request,
                        data,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            tx,
            rx,
            stopped,
            latest,
            handle: Some(handle),
        }
    }
    pub fn send(&self, work: Work) -> Result<(), &'static str> {
        self.latest.store(work.generation, Ordering::Relaxed);
        self.tx.send(work).map_err(|_| "Board worker stopped")
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
