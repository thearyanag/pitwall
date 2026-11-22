# Installed agents and provider adapters

Pitwall is a client for installed coding agents. The native agent owns model
access, login, its tools and its conversation history. Board owns tasks and shared
conversation history. The local worker connects them; it never reads OAuth tokens
and turns them into API keys.

## Setup

Install and sign in to the agent through its own CLI. Click **Refresh** after
installing a new executable. On a new task, click **Agent: …** or press **Ctrl+G**
to cycle providers. The same picker is available on replies: each message chooses
its agent. Submitted/queued messages keep their choice even if you change the
picker for the next message. Comment-only replies do not start an agent.

| Agent | Local interface | Login/setup |
| --- | --- | --- |
| Codex | `codex app-server` over JSONL | `codex login` |
| Claude Code | Official Agent SDK controlling installed `claude` | `claude auth login` |
| Grok Build | `grok --no-auto-update agent --no-leader stdio` | Run `grok` and finish its login |
| Antigravity (Google) | `agy acp` | Run `agy` and finish its login |

Google transitioned consumer Gemini CLI access to Antigravity CLI on June 18,
2026. Existing enterprise and paid API-key access to Gemini CLI remain supported.
See [Google's migration announcement](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)
and [Antigravity ACP support](https://antigravity.google/docs/extensions).
Pitwall uses Antigravity for Google. Gemini CLI is no longer a built-in option.
Existing Gemini tasks are not silently rerouted to a different runtime or session store.

Detection only checks executable availability. It does not claim the login is
valid, start sessions in the background, launch a browser, or install/update agents.
The worker starts the selected agent on demand. It reports expired/missing login
in the task conversation. Permission requests appear in a local Pitwall dialog;
use the numbered choices or click, and scroll to read details. Esc declines.

List detected agents from a source checkout:

```sh
node board-runtime/agent-runner.mjs list
```

The installed launcher exposes `pitwall agents` in v0.3.0 and later.

## Local configuration

Defaults work with executables on PATH. Override them or add an ACP agent in
`worker/agents.json` (source checkout) or
`~/.local/share/pitwall/state/agents.json` (installed build). `PITWALL_AGENTS` can
select another file. This is trusted local configuration, excluded from Git.

```json
[
  {
    "id": "claude",
    "command": "/absolute/path/to/claude"
  },
  {
    "id": "claude-personal",
    "label": "Claude Personal",
    "driver": "claude",
    "command": "claude",
    "args": [],
    "env": { "CLAUDE_CONFIG_DIR": "/absolute/path/to/personal-claude-config" },
    "login": "CLAUDE_CONFIG_DIR=/absolute/path/to/personal-claude-config claude auth login"
  },
  {
    "id": "another-agent",
    "label": "Another Agent",
    "driver": "acp",
    "command": "/absolute/path/to/another-agent",
    "args": ["acp"]
  }
]
```

The last entry is a template: use that agent's documented ACP launch arguments.
Commands are spawned directly, without shell interpolation. Board carries only
the instance ID; it cannot supply executables, environment variables or arguments.
Use native credential stores rather than putting secrets in this file. Native
Instances can set `model` as their configured default; individual messages can
select another advertised model without changing the instance or native session.
Claude launch arguments are rejected because its settings are owned by the SDK.

## Model selection

Click **Model: …** beside the agent picker, or press **Ctrl+O**, for a live list
from the selected installed agent. Use arrow keys and Enter, or click a model.
**Refresh** queries it again; Escape closes the picker without changing the draft.
No model names are hardcoded. Discovery runs only when you open/refresh the picker,
and sends no generation prompt. ACP agents may need a brief native session to
advertise their choices. Failed login or discovery is shown in the picker.

The selected model is saved with the draft and frozen on submission. Replies
inherit the most recent selection; changing agents clears the model selection.
Changing the picker while another run is active affects the next message only.
Use the mode button beside Paste to toggle **Run agent** and **Comment only**.

Model choice is separate from native session identity. Codex selects its model
through app-server; Claude uses its SDK's supported-model catalog and `setModel`;
ACP uses the advertised model config option, with legacy `session/set_model` for
agents that expose only that interface. Model changes retain the native session.
Reference images are reattached when the model changes. Unsupported choices or
images fail visibly instead of silently switching to another model or dropping
attachments. A rejected preflight preserves the preceding verified session.

**Agent default** resets the selection: Codex/Claude use their native default;
ACP restores the default captured for that conversation's agent session (or its
configured instance default). Runtime-reported model IDs appear with worker
replies and in session/completion events. Context-window and reasoning-effort
controls are not part of this picker.

## The adapter boundary

The structure follows T3 Code's separation between provider drivers and
orchestration, with a shared ACP adapter and capability reporting:
[provider architecture](https://github.com/pingdotgg/t3code/blob/main/docs/internals/adding-a-provider.md).
We use maintained upstream SDKs instead of copying T3's private provider packages.

```text
Rust UI → Board task (agent instance ID) → local queue/coordinator
                                              ↓
                                        openAgent()
                                  /          |          \
                           Codex native  Claude SDK   ACP SDK
```

`providers/contract.d.ts` defines `open → session → prompt/cancel/close`.
A session reports `id`, whether it resumed, and its image/resume capabilities.
The host supplies event delivery, a cancellation signal, and permission choices.
Events are normalized as `text_delta`, `message`, `tool`, `session`, `approval`,
`completed` and `error`. Board receives assistant messages and permission notices;
raw tool parameters and approval choices remain local.

For another ACP agent, add a configuration entry. No UI, queue or Rust enum
changes are needed. For a different native protocol, implement the contract in
one module and register the driver in `providers/index.mjs` and the registry's
driver validation. Run the shared lifecycle tests and record a real smoke test.
Do not claim unsupported capabilities or silently drop attachments. Grok has a
small adapter over ACP to honor its native stream boundaries and replay markers;
those vendor extensions are kept out of the generic ACP behavior.

## Session and execution limits

One Board conversation can have multiple native agent sessions. Selecting a new
agent starts its own session with attributed Board history and the reference
images. Returning to an agent resumes its verified session and supplies messages
it has not seen, including intervening comments and other agents' results.
Reference images are sent again when history is handed off, including images
introduced by another agent. Ordinary same-agent continuations send only new
images. Native reasoning and in-flight tool state are not portable.

Each run records its selected history IDs, omitted IDs, image IDs and context
mode in `handoff.json`; `context.json` retains the complete prepared reference
history. History selection keeps up to 40 whole messages within 32,000 UTF-8
bytes, prioritizing recent messages. The original task and current request are
supplied separately for a fresh session. The current request is never shortened.
The reference archive is available to the agent if it needs omitted messages.
This budget is not a guarantee that every native model context has room.

The local session ledger advances only after the agent succeeds, verification
passes, and its final Board result is posted. Old sessions that predate coverage
tracking get a one-time fresh context bootstrap rather than a guessed history
position. Edits to prior Board messages are identified by content fingerprints.

One worker processes one job at a time. Processes close at the end of a job;
verified session IDs let later jobs resume where supported. This is persistent
conversation state, not an always-running agent process. Resume errors are visible;
only a provider that advertises no resume gets a fresh session with Board context.

The current worker still edits Pitwall's own Rust source/README in a source
snapshot and validates changes before applying them. Selecting arbitrary projects,
parallel agents, importing an existing terminal chat, free-form
agent questions, and steering an in-flight turn are separate features.

Codex retains workspace-write sandboxing. Other runtimes retain their own
permission enforcement; Pitwall does not pretend that a source copy is an OS
sandbox. It never enables an always-approve or bypass mode. ACP does not advertise
client-provided filesystem or terminal tools, because these adapters use the
agent's own tool implementations. An agent requiring those client capabilities
needs an additional integration before it can run here.

Provider documentation:
[Codex app-server](https://learn.chatgpt.com/docs/app-server),
[Claude Agent SDK](https://platform.claude.com/docs/en/agent-sdk/overview),
[Grok ACP](https://docs.x.ai/build/cli/headless-scripting),
[Antigravity ACP](https://antigravity.google/docs/extensions),
[ACP TypeScript SDK](https://agentclientprotocol.com/libraries/typescript).

## Verification

Use Node 22.22.3+:

```sh
npm ci --prefix board-runtime
npm run typecheck --prefix board-runtime
node --test board-runtime/*.test.mjs
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The ACP tests launch a peer using the official SDK and exercise framing, native
permission IDs, continuation replay suppression, unsupported capabilities,
cancellation and process failure. Native adapter tests cover their respective
contracts, and Board's in-memory backend covers task routing and follow-ups.
These deterministic tests do not substitute for a signed-in live test of each
installed agent version.
