# Pitwall

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/brand/pitwall-lockup-on-dark.svg">
  <img src="assets/brand/pitwall-lockup-on-light.svg" alt="Pitwall — WALL logo" width="244" height="64">
</picture>

**All your agent work, one view.**

Give your coding agent a task, follow its progress, and reply with changes—all from your
terminal. Pitwall keeps the request, screenshots, progress updates, results,
and feedback together in the same conversation.

Channels on the left. Tasks in the middle. The conversation and composer on the
right. See the work and take the next step from one screen.

## What Pitwall does today

- **Send work:** create a task and let the local agent worker pick it up.
- **Bring the context:** paste screenshots, preview them, and attach them to a task or reply.
- **Follow progress:** see task state, worker availability, progress messages, and results as Board events arrive.
- **Revise in place:** send feedback in the same conversation; the next run uses the original request and relevant prior discussion.
- **Choose when the agent runs:** send a request to your agent or leave a comment without starting work.
- **Pick up where you left off:** separate drafts survive switching tasks and restarting the app.
- **Choose the project:** open a local folder; agents edit that project directly. Pitwall itself is never the default workspace.

Pitwall supports **Codex, Claude Code, Grok Build, and Antigravity + Vercel Board**, with one connected Board per
app instance and one local worker processing one task at a time. The worker's
working directory is the folder explicitly selected for each conversation. Questions and
writing requests can finish with an answer and no file changes.

The terminal interface and worker run on your machine; Vercel Board stores the
task and conversation data. Pitwall runs inside Rex or a compatible terminal.

## Positioning and voice

Pitwall is a **terminal workspace for agent work**, for developers who want to
delegate to their installed coding agents while keeping the request, progress, and next action in view.
Its promise is continuity: a revision belongs with the work it changes.

The name suggests a place to oversee work and decide when to intervene. Keep
that metaphor in the identity; use familiar words in the interface: task,
conversation, reply, comment, run, and result. Write calmly and specifically.
Report what happened and what the user can do next.

| Surface | Copy |
| --- | --- |
| Main headline | All your agent work, one view. |
| Supporting line | Send tasks to your agent, follow progress, and give feedback in the same conversation. |
| Short description | A terminal workspace for tasks, agent progress, and feedback. |
| Empty task list | No tasks yet. Create your first task. |
| Task submitted | Task queued for the agent. |
| Feedback submitted | Feedback queued for the agent. |
| Comment submitted | Comment posted. Agent was not started. |
| Answer-only completion | Response complete. No files changed. |
| File changes | Describe the files changed and checks actually run. |

Use completion language that matches the result. A completed run is not
necessarily a deployed release, and checks should only be mentioned when they
actually ran. Parallel workers, a dedicated
attention/review queue, and an integrated diff viewer are future directions,
not current product claims.

The interface and launch name are now Pitwall. **Launch Pitwall.command** is the
primary launcher; **Launch Sprout.command** remains a compatible alias. The
executable is still `sprout`, and `SPROUT_*` settings, draft/retry keys,
`project=sprout`, `source=sprout-ui`, `sprout-runs`, and Rex's process-name match
are intentionally preserved so existing drafts, tasks and shortcuts keep working.
The original educational reader keeps its historical Sprout name.

## Local agents

Click **Agent: …** in a new task or reply, or press **Ctrl+G**, to choose the
agent for that message. You can switch agents within the same conversation. Pitwall starts the agent process when
the worker picks up the task; no separate ACP server command is needed.
Use native agent logins. Missing installations are shown in the composer.
Permission requests appear in a local dialog with the agent’s choices; Esc denies.

See [agent setup and adapter architecture](docs/agents.md) for configuration,
capability limits, session continuation, and adding another ACP agent.

**v0.4.0** adds explicit project folders and native agent configuration to
multi-agent conversations and model selection.
Run the installer below to install or upgrade; restart existing UI and worker tabs afterward.

## Install with one command (macOS)

Apple Silicon and Intel Macs on macOS 13.5 or newer:

```sh
curl -fsSL https://raw.githubusercontent.com/thearyanag/pitwall/master/install.sh | sh
```

The installer downloads a prebuilt app, bundled Node runtime, and pinned Board
dependencies, then verifies the release checksum. **No Rust, Node, npm, or source
build is needed to try the UI.** It installs for your user without sudo and does
not edit shell startup files, sign you in, or start a worker.

```sh
~/.local/bin/pitwall --demo                  # Try it without an account
~/.local/bin/pitwall login                   # Sign in to Vercel when ready
~/.local/bin/pitwall --team your-team        # Open your Board
```

If `~/.local/bin` is already on PATH, use `pitwall` directly. Otherwise add it to
your shell's PATH or keep using the full path above. The terminal must support
true colour; Rex supports the screenshot previews and WALL mark.

Settings live in `~/.local/share/pitwall/config.env`; drafts, images and run
history live in `~/.local/share/pitwall/state/`. Set `SPROUT_TEAM` in that trusted
shell config file to avoid passing `--team` each time. To run the worker, install
Python 3.11+ and a signed-in supported agent CLI, then run `pitwall worker` (or prefix it
with `SPROUT_TEAM=your-team`). Install the build tools required by the project you open.
The worker follows that project’s instructions and asks the agent to run relevant checks.

Run the installer again to update. It switches the `current` version atomically
and retains settings, drafts, and old release directories, including local worker
changes in them. Restart the UI and any workers afterward. Reinstalling the same
release preserves that release's local source changes. `PITWALL_VERSION=v0.4.0`
can pin a tag; `PITWALL_INSTALL_DIR` and `PITWALL_BIN_DIR` select absolute install
paths. Put these variables on the `sh` side of the pipe when overriding them.

Release archives use ad-hoc code signatures; this initial tarball distribution is
not Developer ID notarized. Install scripts and release checksums are public in
this repository and its [releases](https://github.com/thearyanag/pitwall/releases).

## Install from source

The current desktop workflow targets macOS and Rex (or a compatible terminal).
Use Rust 1.88+, Node 22.22.3+, and Python 3.11+. The worker also needs an
installed, signed-in supported agent CLI. Prebuilt binaries are published as release assets; the Git checkout contains source.

```sh
git clone https://github.com/thearyanag/pitwall.git
cd pitwall
npm ci --prefix board-runtime
cargo build --locked --bins
cp target/debug/sprout ./sprout
cp target/debug/reader ./reader
cp .pitwall.env.example .pitwall.env
# Edit .pitwall.env and replace your-team with your Vercel team slug.
./board-runtime/node_modules/.bin/vercel login
./'Launch Pitwall.command'
```

Run `./'Launch Pitwall.command' --demo` for fictional sample data; it cannot
submit tasks. `.pitwall.env` is an optional trusted shell configuration file,
read by the launchers and excluded from Git. Set executable paths there when
launching from Finder if Node/Python/your agent are not on its PATH. Environment
variables take precedence over the defaults in the supplied example.

## Run

Double-click **Launch Pitwall.command**, or run it from this directory:

```sh
./'Launch Pitwall.command'
./'Launch Pitwall.command' --team your-team --board General
./'Launch Pitwall.command' --demo
```

The launch script uses Node from PATH unless `SPROUT_NODE` selects an executable.
With Node on PATH and `SPROUT_TEAM` exported, `./sprout` also works.
Keep `board-runtime/` beside the executable.

Choose your team with `--team` or `SPROUT_TEAM`; live mode requires an explicit
team. The launchers load your local `.pitwall.env` settings. Choose a named board with `--board` or
`SPROUT_BOARD`; otherwise the team's default Board is used. Named boards are
looked up with `ensure: false`: a misspelling does not create a new board.

The client uses the official Board SDK and your existing Vercel login. Credentials
are handled by the SDK, not copied into Pitwall. To refresh an expired login:

```sh
./board-runtime/node_modules/.bin/vercel login
```

## Open a project

Click **Open project** in the composer or press **Ctrl+L**. Browse to a folder
and click **Open folder**, or paste its absolute path and press Enter. Opening a
folder starts a new task draft for that project. There is no default project;
Send stays disabled until you choose one. The composer shows its path.

Each task keeps its project when you switch agents or models or send feedback.
Opening another folder starts a separate task; it does not redirect an existing
conversation. Project paths stay in the local registry; Board stores an opaque
project ID. If a directory is moved, replaced, or unavailable, open it again and
start a new task. Older projectless tasks remain readable and accept comments,
but cannot start agents.

Agents edit files **directly in the selected folder**. Changes can remain after
an interrupted or failed run; use your project's version control to review them.
Pitwall does not copy changes back or run a fixed Rust validation pipeline.
Pitwall launches the selected agent in that folder and delegates configuration,
trust and execution permissions to the native runtime. Approval requests appear
in Pitwall. It does not add another sandbox or force full access/network access.
The selected folder is a work target, not a universal filesystem barrier: actual
access depends on your agent's native settings. See
[execution limits](docs/agents.md#session-and-execution-limits).

## Composer and clickable controls

The conversation has an always-visible composer. Click it and type your reply.
**Run agent** is the default; click the mode button beside Paste to switch to
**Comment only** and post a note without
creating a worker run. Click **Send** or press **Cmd+Enter**. Enter adds a new line.
After sending, the composer clears and retains the selected agent/model.
Click **Model: …** or press **Ctrl+O** to load the installed agent’s model choices.
Choose a different model for a reply to switch within the same conversation;
queued messages retain the selection they were submitted with.

**+ New task** uses the same composer to create a task in the selected channel.
Selecting a task switches back to its reply draft. Drafts are saved separately
per project/channel or conversation, including images and send mode, and survive restarts.
Esc leaves the editor without deleting the draft. The editor supports selection,
word wrapping and undo/redo (**Ctrl+Z**, **Ctrl+Shift+Z**); these use
`ratatui-textarea`. Existing drafts are migrated on read; older drafts default to Codex.

Click **Paste** or press **Cmd+V** for clipboard text or screenshots. **Attach**
opens a file chooser inside Pitwall: click folders/images, or type/paste a path
and press Enter. Click an image thumbnail to expand it; click its **×** to remove
it. Images upload only when you send. Channel/task rows, Refresh, All threads,
Load more and preview controls are clickable too.

This Mac loads `rex-shortcuts.lua` from `~/.config/rex/init.lua`. It routes Cmd+V
and Cmd+Enter to Pitwall while its current `sprout` process is foreground in the pane;
other programs keep Rex's normal paste and split actions. On another Mac, add
`dofile("/absolute/path/to/sprout/rex-shortcuts.lua")` to Rex's init.lua and run
`rex config reload`. Remove that include to restore the original bindings.
**Ctrl+V**, **Ctrl+Enter** and **Ctrl+S** are terminal fallbacks; clickable buttons
work without the Rex configuration.

## Appearance

The dark UI uses Carbon (`#0E0F12`), Radio Orange (`#FF5A1F`), Track Grey
(`#8A8F98`) and Paddock White (`#F4F4F1`). Neutral panel layers and focus borders
keep orange concentrated in the approved WALL mark and the primary action.
Status dots are grey for queued/waiting, green for running, yellow for blocked,
purple for completed, and red for failed. Labels remain visible alongside colour.

Press **t** outside the composer to switch to the matching light theme.
The approved transparent PNG mark uses Pitwall's existing direct Kitty renderer
in Rex; other terminals retain the plain-text PITWALL wordmark. The on-dark and
on-light SVG/PNG assets and outlined wordmarks live in `assets/brand/`, with the
Inter font license. No runtime SVG or font dependency is added.

## Controls

| Key | Action |
| --- | --- |
| Ctrl+L | Open a project folder for a new task |
| n | Compose a task in the selected channel |
| c | Focus the reply composer |
| Tab / Shift-Tab | Cycle columns and composer focus |
| Left / Right, h / l | Move between columns |
| Up / Down, j / k | Select a channel or task; scroll the conversation |
| Space / PageDown / PageUp | Scroll the conversation by a page |
| g / G, Home / End | Beginning / end of the conversation |
| r | Reload channels and the current list |
| v | Switch tasks / all threads |
| m | Load the next page of tasks or older threads |
| d | Switch between the live Board and clearly labeled fictional demo data |
| t | Toggle Pitwall's light/dark colors |
| ? | Open / close keyboard help; Esc also closes it |
| q / Ctrl-C | Quit and restore the terminal |

Help pauses keyboard navigation and other controls beneath the panel; `q` and
Ctrl-C still quit. Pitwall submits tasks; a separate queue worker runs the selected agent.

The UI subscribes to Board's event feed through one long-lived SDK process.
Relevant events trigger snapshots, coalesced across bursts and deferred while a
send is in flight. The SDK handles cursor resumption and reconnect backoff;
a fresh snapshot on connection closes startup/reconnect gaps. The header shows
**Feed: live**, **connecting**, **reconnecting**, or **disconnected**. Press **r**
to retry after a permanent feed error. Idle conversations are no longer polled
every five seconds. Task lease expiry is checked locally and refreshed once.

The conversation opens at the latest message and follows new replies.
Scrolling up pauses following; scrolling back to the bottom or pressing **End** /
**Shift+G** outside the composer resumes it. The title shows **following** or
**paused**. Sending a message also returns to the latest replies. **r** manually
refreshes channels and tasks. Tasks include
all states. Lists load 50 at a time; a `+` after the count and the footer indicate
more pages. Manual refresh reloads the first list page. Conversations come from
Board's whole-thread read. Times are shown in UTC. Attachments are listed by name;
Press p to preview PNG/JPEG attachments (Tab cycles images, Esc closes). Minimum size is 100 columns × 28 rows.

Errors remain visible until retry or navigation. Previously loaded content is
identified as stale when its refresh fails. The app never silently substitutes
demo content for a failed connection. The composer writes tasks and replies to Board. It
assigns new tasks to codex-worker with project=sprout and source=sprout-ui labels,
and disables external sync for the task. Demo mode cannot create real tasks.

## Create a task and run it automatically

1. Start `worker/Start Codex Queue.command` in its own terminal tab. It stays
   running until you press Ctrl+C there. The header shows Worker: waiting/working.
2. In Pitwall, select a live channel and **Open project** (**Ctrl+L**). For another
   task in that project, click **+ New task** (or press **n**).
3. Type the task: first line is the title, Enter adds details. Left/Right,
   Home/End, Backspace/Delete and multiline paste work in the composer.
4. Click **Create task** or press **Cmd+Enter** to submit. The task is selected immediately. The worker
   picks it up within a few seconds, and progress appears in the conversation.
5. Review the result and changes in your selected folder. The agent reports
   the checks it actually ran; a completed task does not deploy or publish code.

The worker processes one task at a time, across channels, and takes only your
Pitwall-created tasks assigned to codex-worker. It uses Board’s built-in wait API
while idle, waking for ready-task events or a known lease expiry. It doesn't consume other people's
or unrelated tasks, and ignores tasks without a locally registered project.
The agent follows the selected project’s instructions. It is instructed to preserve
unrelated edits and not commit, push, deploy or contact external services unless requested.

**Stopping:** Ctrl+C in the queue worker tab stops waiting and cancels any active
run. Board releases the claim, so a later worker can pick it up. Closing Pitwall
alone does not stop the separate queue worker. No login/background service is installed.

**Submission recovery:** an attempted submission is saved to
`worker/pending-task.json` before the network request. A retry uses the same key,
so it cannot create a duplicate. After an uncertain result, press n to resume,
then Ctrl+S to retry unchanged. Text and images are locked after the first attempt. The file
is deleted once the server confirms creation. Esc leaves the editor and retains the draft, including after an error.
Ctrl+C can quit even during submission; n on the next launch recovers it.

### Example: a queued task

Keep the **Agent Queue** tab running. In Pitwall, open a project with a README, select a live channel, press **n**,
and type: “Improve README.md by adding a short explanation of how to refresh the
task list. Change README.md only.” Press **Ctrl+S**, then watch the task become
claimed and completed (press **r** to refresh the list). Inspect the selected
task's conversation for progress, the summary and validation results. **Ctrl+C**
in the queue worker tab stops the worker.

Press **c** to open a reply on the selected task, then **Ctrl+S** to queue the worker follow-up in the same conversation.

## Reply to the worker

Select one of your Pitwall tasks. Write feedback, a question, or requested changes
in the visible composer, then click **Send** or press **Cmd+Enter**. You can attach screenshots using the
same image controls as the new-task composer. The reply and the agent's progress and
answer remain in the original conversation.

The worker processes requests in order. Feedback sent while it is working is
queued for another run after the current run; it does not interrupt the running
model. A successfully completed native session is resumed when supported, with the new
request and any shared history it missed. Switching agents preserves the Board
conversation and reattaches reference images. Otherwise the run receives the original request, bounded
Board history, and relevant images. Both paths use the same selected project. Questions can
finish with an answer and no file changes. The agent runs the checks appropriate
to that project; Pitwall does not independently certify its result.

Completed Board tasks cannot be reopened, so Pitwall creates an internal follow-up
job in the `sprout-runs` channel and displays its status on the original task.
That infrastructure channel is hidden from Pitwall's channel list. The original
Board task ID and conversation remain unchanged; the CLI can still inspect the
individual run records. Worker progress and results are posted to the original
thread, not a second conversation you need to follow.

Unconfirmed replies use `worker/pending-reply.json` and retain their original
thread ID and retry key. Select the original task to resume its reply, or
select the original channel and click **+ New task** to resume its task draft.
These do not overwrite each other. Retrying a reply cannot create
duplicate feedback or duplicate jobs. **Comment only** posts to the same thread
without creating a job or changing its task status. Ordinary unsent drafts live
under `worker/drafts/`; each target has its own file.

## Screenshots and image tasks

In either composer, click **Paste** or press **Cmd+V** for clipboard images or
text. **Attach** opens the terminal file chooser for PNG/JPEG files. Pasting a
local image path also works. Pitwall stages full-resolution PNGs without changing
the originals. Up to 10 images are supported, each at most 20 MB and 40 megapixels.

Thumbnails appear below the text. Click one for a larger preview, **×** to remove
it, or **Next images** when the strip has more attachments than fit on screen.
Keyboard alternatives are **Ctrl+P** to expand and **Ctrl+D** to remove the
selected image. Click **Send** to upload and post; pasting alone does not upload.

Draft images survive restarts and switching conversations. Once a submission is
attempted, its contents are locked; retries reuse the message key and upload
receipts. Confirmed submissions clean up staged images. Downloaded task images
are cached under `worker/downloads/`. The SDK handles authenticated transfers.

In a conversation, press **p** to view attached images, **Tab** to cycle, and
**Esc** to return. Rendering uses the terminal's negotiated graphics protocol
(direct Kitty placements in Rex, emitted after the text frame), with a colored-cell
fallback on terminals without image support. Pitwall avoids Unicode-placeholder
placements because those produced blank previews through Rex.
Preview copies may be scaled; the Board attachment keeps full resolution.

The queue worker downloads task images into its isolated run and passes each
file through the selected adapter’s image input. Unsupported image capabilities
or download failures stop the task rather than silently dropping screenshots. Each run records `images/manifest.json` and
`image-inputs.json` so the image handoff can be checked.

## Source map

- `install.sh`: checksum-verified, per-user binary installer.
- `scripts/package-release.py`: macOS archive builder with bundled Node/runtime dependencies.
- `scripts/pitwall-launcher.sh`: installed `pitwall` command, login and worker entry points.
- `src/main.rs`: configuration, terminal setup and restoration.
- `src/app.rs`: Board state, shared actions and the event loop.
- `src/app_tests.rs`: input, scrolling, draft and rendering regressions.
- `src/ui.rs`: three-column layout, inline composer and clickable hit regions.
- `src/file_picker.rs`: terminal file chooser and path navigation.
- `rex-shortcuts.lua`: Pitwall-scoped Cmd+V and Cmd+Enter bindings.
- `src/board.rs`: background worker, cancellation, timeouts and JSON bridge.
- `src/composer.rs`: ratatui-textarea integration, screenshot drafts and persistent retry state.
- `src/approvals.rs`: scoped local permission requests and replies.
- `board-runtime/providers/`: common contract, instance registry, and native/ACP adapters.
- `board-runtime/discover-models.mjs`: explicit native model discovery for the picker, without generation.
- `board-runtime/agent-runner.mjs`: automatic agent lifecycle and normalized events.
- `src/state.rs`: typed focus, list mode, task state and worker heartbeat.
- `src/feed.rs`: long-lived feed process and typed notifications.
- `board-runtime/feed.mjs`: official SDK subscription and view invalidations.
- `src/images.rs`: clipboard input, PNG staging and terminal image widgets.
- `board-runtime/images.mjs`: image-size metadata inspection, limits, authenticated transfers and retry receipts.
- `board-runtime/feedback.mjs`: follow-up jobs, original-thread routing and run context.
- `board-runtime/queue.mjs`: scoped task admission and official Board wait API.
- `worker/handoff.py`: bounded, attributed history and per-agent image/context coverage.
- `worker/listener.py`: local process supervisor, heartbeat and shutdown.
- `worker/run_codex.py`: provider-neutral runs in the selected project (legacy filename).
- `board-runtime/projects.mjs`: explicit local project registration and identity checks.
- `board-runtime/adapter.mjs`: shared SDK boundary for UI reads/writes, queue discovery and run preparation.
- `board-runtime/adapter.test.mjs`: integration checks against Board's in-memory backend.
- `src/bin/reader.rs`: the original educational document reader; run `./reader`.
- `READER.md`: original reader walkthrough.

The flow is: key event -> selected channel/task -> worker process -> official
Board SDK -> typed JSON -> Rust state -> Ratatui cells -> Rex pixels. Requests
carry a generation number; late responses cannot replace the current selection.
Changing selection cancels obsolete subprocesses. Quitting cancels pending reads.

## Build and test

```sh
npm ci --prefix board-runtime
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
node --test board-runtime/*.test.mjs
npm run typecheck --prefix board-runtime
python3 -m unittest discover -s worker -p 'test_*.py'
```

Use Node 22.22.3 or newer for the local-backend integration tests; older Node
22 releases can hit a SQLite parameter-binding error in Board's in-memory backend.
The SDK and CLI versions are pinned in `board-runtime/package-lock.json`.

After rebuilding, run `cargo run --locked -- --team YOUR_TEAM` or copy the newly
built binaries beside `board-runtime/`. Agents use the selected project’s build
tools, project configuration and native permission settings.

References: https://github.com/vercel/board and https://vercel.com/docs/board

## Build release archives

Maintainers can produce both Mac archives from a clean committed checkout:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
python3 scripts/package-release.py --work /absolute/path/to/release-work
python3 scripts/test-installer.py
```

The builder uses the current Cargo version, records the source commit, verifies
official Node download checksums, installs locked npm dependencies for each CPU,
and produces `dist/pitwall-darwin-arm64.tar.gz`, `dist/pitwall-darwin-x64.tar.gz`,
and `dist/SHA256SUMS`. Intel smoke checks on an Apple Silicon builder require
Rosetta. Upload all three assets to the matching GitHub release tag. Generated
packages, local configuration and runtime state are excluded from Git.
