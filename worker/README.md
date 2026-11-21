# Pitwall Codex queue worker

Run `Start Codex Queue.command` in a terminal. It waits for tasks created through
Pitwall's **+ New task → Create task** composer, claims one, runs Codex and returns to waiting.
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
final status. Codex runs in a per-task source snapshot under `worker/runs/`, with
workspace-write sandboxing and network access disabled. Its progress messages
are forwarded by the wrapper; credentials and raw tool outputs are not forwarded.

The permitted result is Rust source under `src/` and `README.md`. Manifests,
lockfiles, lessons, worker code and external services are outside its scope.
The wrapper independently checks formatting, tests, lint and build. It then
checks the live source against the original snapshot before publishing. If you
edited the source during execution, the task fails rather than overwriting your
work; the proposed result stays in the run workspace. Successful runs update
the source and `sprout` executable. Restart Pitwall to load UI changes.

Codex gets up to 20 minutes per task. A failed task stays failed for review and is
not automatically retried. SIGINT/SIGTERM stops active child processes; Board's
runner releases the claim on interruption. A lost lease also stops the run.
Logs, proposed source, final summary and checks live in `worker/runs/`.
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
before Codex starts, validated for type/size and passed with --image. The originals
stay in the run’s images directory, with a manifest and an image-inputs record.
An attachment that cannot be retrieved or validated fails the task rather than
being silently omitted.

## Feedback and questions

Pitwall's visible reply composer defaults to Send to Codex, which posts to the existing task thread and creates a separate
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
