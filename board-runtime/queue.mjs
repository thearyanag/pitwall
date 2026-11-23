import { PROJECT_LABEL, knownProject } from "./projects.mjs";
// Admission stays Pitwall-specific; Board's wait() owns the idle network wait.
// claim --ready also accepts unassigned tasks and requires a channel/subtree,
// so it cannot replace this exact-assignment, board-wide admission rule.
const LABELS = { project: "sprout", source: "sprout-ui" };
const READY = { ...LABELS, "board.task": "ready" };

export async function scanQueue(team, identity) {
  const me = identity ?? (await team.me());
  const actor = `${me.principal}+codex-worker`;
  let after;
  let expires_at = null;
  const seen = new Set();
  do {
    const page = await team.tasks.list({
      state: "open",
      owner: me.principal,
      for: actor,
      label: LABELS,
      limit: 100,
      ...(after ? { after } : {}),
    });
    for (const { message: m } of page.tasks) {
      const task = m.task;
      if (
        m.deleted ||
        task?.owner !== me.principal ||
        task?.for !== actor ||
        m.labels?.project !== "sprout" ||
        m.labels?.source !== "sprout-ui" ||
        !knownProject(m.labels?.[PROJECT_LABEL])
      )
        continue;
      const until =
        task.state === "claimed" ? Date.parse(task.claim?.until) : NaN;
      if (task.state === "ready" || until <= Date.now())
        return { task_id: m.id, expires_at: null };
      if (Number.isFinite(until))
        expires_at = Math.min(expires_at ?? Infinity, until);
    }
    after = page.tasks.length === 100 ? page.cursor : null;
    if (after && seen.has(after))
      throw new Error("Board returned a repeated task cursor.");
    if (after) seen.add(after);
  } while (after);
  return { task_id: null, expires_at };
}

export async function waitForTask(team, signal) {
  try {
    const me = await team.me();
    while (!signal.aborted) {
      // Capture the cursor BEFORE checking the queue: a task arriving between
      // the snapshot and wait must wake us, rather than wait for another event.
      const { cursor } = await team.poll({ label: READY, wait: 0 }, signal);
      const ready = await scanQueue(team, me);
      if (signal.aborted) return null;
      if (ready.task_id) return ready.task_id;
      // Some backends expire leases without emitting an event. Wake once at
      // the earliest known expiry; otherwise let Board wait for up to 30 minutes.
      const timeout =
        ready.expires_at === null
          ? 30 * 60_000
          : Math.max(1, Math.min(30 * 60_000, ready.expires_at - Date.now()));
      await team.wait({ label: READY, after: cursor }, { signal, timeout });
    }
    return null;
  } catch (error) {
    if (signal.aborted) return null;
    throw error;
  }
}
