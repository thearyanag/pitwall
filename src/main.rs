//! Pitwall's terminal entry point. State and rendering live in app and ui.
mod app;
mod board;
mod composer;
mod feed;
mod file_picker;
mod images;
mod state;
mod ui;
use app::App;
use board::Config;
use crossterm::event;
use ratatui_image::picker::Picker;
use std::{
    env,
    io::{self, IsTerminal},
    path::PathBuf,
};

fn config() -> Result<Option<Config>, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let base = exe.parent().unwrap();
    let beside = base.join("board-runtime/adapter.mjs");
    let mut config = Config {
        team: env::var("SPROUT_TEAM").unwrap_or_default(),
        board: env::var("SPROUT_BOARD").unwrap_or_default(),
        demo: false,
        node: env::var("SPROUT_NODE").unwrap_or_else(|_| "node".into()),
        adapter: if beside.exists() {
            beside
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("board-runtime/adapter.mjs")
        },
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => config.demo = true,
            "--team" => config.team = args.next().ok_or("--team needs a slug or ID")?,
            "--board" => config.board = args.next().ok_or("--board needs a name")?,
            "--help" | "-h" => {
                println!("Pitwall — a terminal workspace for agent work\nAll your agent work, one view.\n\nSend tasks to Codex, follow progress, and give feedback in the same conversation.\n\nUsage: sprout [--team SLUG] [--board NAME] [--demo]\n\nn: new task | Tab/arrows: navigate | r: refresh | v: tasks/threads | m: more\nt: theme | q/Ctrl-C: quit\n\nUses your existing Vercel CLI login. --demo is explicitly fictional.\nEnvironment: SPROUT_TEAM, SPROUT_BOARD, SPROUT_NODE.\nOriginal lesson reader: ./reader");
                return Ok(None);
            }
            _ => return Err(format!("Unknown argument {arg}; use --help")),
        }
    }
    if config.team.is_empty() {
        if config.demo {
            config.team = "demo".into();
        } else {
            return Err("Choose a Vercel team with --team SLUG or SPROUT_TEAM (the launcher also reads .pitwall.env).".into());
        }
    }
    Ok(Some(config))
}
fn main() -> io::Result<()> {
    let Some(config) = config().map_err(io::Error::other)? else {
        return Ok(());
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "Run Pitwall inside Rex or another interactive terminal.",
        ));
    }
    let mut app = App::new(config);
    let mut terminal = ratatui::init();
    app.image_picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let result = crossterm::execute!(
        io::stdout(),
        event::EnableBracketedPaste,
        event::EnableMouseCapture,
        event::EnableFocusChange,
        event::PushKeyboardEnhancementFlags(
            event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .and_then(|_| app::run(&mut terminal, &mut app));
    let _ = app.graphics.render_many(&[]);
    let _ = crossterm::execute!(
        io::stdout(),
        event::DisableBracketedPaste,
        event::DisableMouseCapture,
        event::DisableFocusChange,
        event::PopKeyboardEnhancementFlags
    );
    ratatui::restore();
    result
}
