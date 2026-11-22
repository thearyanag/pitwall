# Pitwall agent queue worker

Run `Start Codex Queue.command` in a terminal. It waits for tasks created through
Pitwall's **+ New task → Create task** composer, claims one, runs the selected agent and returns to waiting.
Use Ctrl+C in that tab to stop it. Closing Pitwall itself leaves the worker running.
The worker is a foreground process, not an installed startup service.

The queue requires your Vercel team in `SPROUT_TEAM`; its default board is General. `SPROUT_TEAM` and
`SPROUT_BOARD` select another target, which must match Pitwall's target. It works
across channels but takes only the logged-in user's tasks assigned to codex-worker
with both `project=sprout` and `source=sprout-ui`. A workspace lock prevents two
local listeners from editing this project at the same time.

Idle waiting uses the official Board SDK's `wait()` API. The listener checks for
eligible work once, then holds a wait for matching ready-task events rather than
polling every three seconds. It captures the feed cursor before checking tasks,
so a task arriving between the check and wait is not missed. It rechecks when an
event arrives, at the earliest known claim expiry, or after a 30-minute idle wait.
Board owns the long-poll requests; local timer ticks only maintain the heartbeat
and supervise shutdown. Network failures use bounded retry backoff.

The queue deliberately keeps exact assignment filtering. Board's
`tasks run --ready --wait` also takes unassigned tasks and requires a channel or
subtree, so it cannot preserve Pitwall's board-wide, assigned-only queue semantics.
No foreign owners are opted in. The short admission check stays in
`board-runtime/queue.mjs`; all idle network waiting is delegated to Board.

The official Board CLI claims the selected task, renews its lease and records its
final status. The provider-neutral coordinator starts the selected adapter in a
source snapshot under `worker/sessions/<key>/workspace`. Codex uses its native
workspace-write sandbox with task network access disabled. Other agents use their
native permission enforcement, with requests routed to Pitwall; a source snapshot
is not an OS sandbox. Pitwall does not enable bypass/always-approve modes. Its progress messages
are forwarded by the wrapper; credentials and raw tool outputs are not forwarded.

The permitted result is Rust source under `src/` and `README.md`. Manifests,
lockfiles, lessons, worker code and external services are outside its scope.
The wrapper independently checks formatting, tests, lint and build. It then
checks the live source against the original snapshot before publishing. If you
edited the source during execution, the task fails rather than overwriting your
work; the proposed result stays in the run workspace. Successful runs update
the source and `sprout` executable. Restart Pitwall to load UI changes.

Each agent gets up to 20 minutes per task. A failed task stays failed for review and is
not automatically retried. SIGINT/SIGTERM stops active child processes; Board's
runner releases the claim on interruption. A lost lease also stops the run.
Logs, final summary and checks live in `worker/runs/`. Current proposed source is
in the session workspace; the next attempt preserves it in that run’s
`previous-workspace/` before refreshing source. Successful verified sessions can
resume; failed or interrupted attempts start fresh with Board context. These
paths live under `SPROUT_STATE_DIR` for installed builds.
`status.json` is a local heartbeat used by Pitwall's header; stale heartbeats are
shown as offline.

For one explicitly selected task, the existing launcher still works:

```sh
./worker/'Launch Codex Worker.command' TASK_ID
```

Messages and new tasks use `--no-sync` / `noSync`, so they are not mirrored to
Slack, GitHub or Linear. Local task records and run history are intentionally excluded from Git.

## Image inputs

Task PNG/JPEG attachments are downloaded through the authenticated Board SDK
before the agent starts, validated for type/size and passed as native image input.
ACP image support is negotiated; unsupported images fail explicitly. The originals
stay in the run’s images directory, with a manifest and an image-inputs record.
An attachment that cannot be retrieved or validated fails the task rather than
being silently omitted.

## Feedback and questions

Pitwall's visible reply composer defaults to Send to the task’s agent, which posts to the existing task thread and creates a separate
follow-up job because completed Board tasks are immutable. The listener picks
that job up after the current run. The Board adapter loads the original task,
recent conversation (last 40 messages, at most 40,000 characters), the exact
feedback message and reference images. Newer user feedback is left for its own
job, while later worker results are available as context. The original task and
the exact requested feedback are always included separately from the bounded
history. Progress and final results route back to the original conversation.

Answer-only runs finish successfully without changing files or claiming tests
ran. Runs that change files still execute independent checks and publish only
when the original source has not changed.

**Comment only** posts a reply without creating a worker job. Both the UI and
worker use `board-runtime/adapter.mjs`; there is one context/image preparation
path. The official Board CLI still owns claiming and lease renewal.


## Providers and local sessions

`board-runtime/providers/contract.d.ts` defines the shared adapter interface.
`agent-runner.mjs` owns process lifecycle and local permissions. The Python
coordinator knows only normalized messages, session IDs and completion/errors.
Codex uses app-server; Claude uses the official Agent SDK with the installed CLI;
Grok and Antigravity use the official ACP SDK client, with a small Grok adapter
for its stream boundaries and replay markers.

`pitwall.agent` records the agent for each submitted message and follow-up job.
A reply may select a different installed agent without changing the root task. Existing tasks without the label default to Codex. The historical Board
actor `codex-worker` and launcher/script filenames remain compatibility identifiers;
they no longer choose the execution provider. The queue still runs one job at a time.

Native session state is scoped by team, board, conversation and local instance
configuration. Changing executable, account environment, arguments or model creates
a separate session workspace. Resume errors fail explicitly, rather than silently
starting a replacement conversation. Agents that advertise no resume get a fresh
session with bounded Board context. Processes are released after each job; this
version does not keep idle agents warm.

Only a successfully verified result with its final Board reply posted becomes
resumable. `handoff.py` selects the unseen Board history for that native session;
`session.json` records content fingerprints and image IDs actually covered.
Fresh agents receive reference images and bounded history. Returning agents
receive the intervening context and reference images again; same-agent
continuations without missed context send only new images. Comment-only replies
are not separate native turns, but become context for the next requested run.
`handoff.json` records the input selection; the native completion and verified
session ledger confirm whether delivery succeeded.

Permission requests and choices stay in local `approvals/` files, scoped to the
same team and board. The UI displays the requesting agent and task. Requests expire
after five minutes and cancellation denies them. Unknown native interaction methods
fail closed. Free-form agent questions and plan-mode controls are not yet exposed;
answer questions through the ordinary task conversation.

See [agent configuration](../docs/agents.md). No provider credentials are copied
into Board or the repository. Agent sign-in and subscription eligibility remain
owned by the native agent runtime.


## Per-message models

`pitwall.model` on the root/request/job records the requested model (`default`
means reset to the agent default). A model change does not change `provider_key`
or the native session directory. The runtime confirms its model through native
RPC/SDK metadata, and worker replies are labeled with that model. Session state
keeps `last_model` and `default_model`; changes reattach the reference images.
Preflight rejection restores the prior verified session, while uncertain failures
after prompting keep the existing fresh-context recovery behavior.
Model discovery is an explicit picker operation, separate from Board and the queue.
