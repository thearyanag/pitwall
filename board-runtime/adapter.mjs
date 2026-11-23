import { PROJECT_LABEL, listProjects, resolveProject } from "./projects.mjs";
import { MODEL_LABEL, modelLabel } from "./providers/models.mjs";
import { catalog, provider, PROVIDER_LABEL } from "./providers/registry.mjs";
import { waitForTask } from "./queue.mjs";
import { follow } from "./feed.mjs";
// The only network boundary. Uses Vercel's official SDK and existing CLI login.
// Credentials stay in the SDK; draft images and worker context are stored locally.
import { board } from "@vercel/board";
import { pathToFileURL } from "node:url";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { uploadImages, downloadImage } from "./images.mjs";
import {
  sendFeedback,
  effectiveTask,
  prepareRun,
  RUN_CHANNEL,
} from "./feedback.mjs";

export const clean = (value) =>
  String(value ?? "").replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, "");
const author = (m) => {
  const agent = m.labels?.[PROVIDER_LABEL];
  if (
    agent &&
    m.labels?.["pitwall.kind"] &&
    m.by?.actor?.endsWith("+codex-worker")
  )
    return clean(
      `${catalog().find((p) => p.id === agent)?.label || agent} · ${m.labels?.[MODEL_LABEL] && m.labels[MODEL_LABEL] !== "default" ? m.labels[MODEL_LABEL] : "worker"}`,
    );
  return clean(m.author?.name || m.author?.handle || m.by?.actor || "Unknown");
};
const taskState = (task) =>
  task?.state === "claimed" &&
  task.claim &&
  Date.parse(task.claim.until) <= Date.now()
    ? "ready"
    : (task?.state ?? "thread");

const leaseUntil = (task) =>
  task?.state === "claimed" && task.claim ? Date.parse(task.claim.until) : null;

export function message(m) {
  return {
    id: String(m.id),
    author: author(m),
    time: clean(m.createdAt),
    text: m.deleted ? "[Message deleted]" : clean(m.text),
    attachments: (m.attachments ?? []).map((a) => ({
      id: String(a.id),
      name: clean(a.name || a.id),
      type: clean(a.type),
      size: a.size,
    })),
  };
}

export function summary(t) {
  const m = t.message;
  return {
    id: String(m.id),
    title:
      clean(m.deleted ? "[Deleted task]" : m.text)
        .split("\n")
        .find((s) => s.trim()) || "(Untitled)",
    state: taskState(m.task),
    lease_until: leaseUntil(m.task),
    author: author(m),
    replies: t.replies ?? 0,
    labels: Object.entries(m.labels ?? {})
      .filter(([key]) => !key.startsWith("board."))
      .map(([k, v]) => clean(`${k}=${v}`)),
  };
}

export async function execute(team, req, imageRoot) {
  if (req.op === "prepare") {
    if (!req.task || !req.dir)
      throw new Error("A task and image directory are required.");
    const prepared = await prepareRun(team, req.task);
    try {
      const images = [];
      for (const attachment of prepared.images) {
        images.push({
          ...attachment,
          path: await downloadImage(team, attachment, req.dir),
        });
      }
      mkdirSync(req.dir, { recursive: true });
      writeFileSync(
        join(req.dir, "manifest.json"),
        JSON.stringify(images, null, 2),
      );
      return { ...prepared, images };
    } catch (error) {
      // Only a validated conversation may receive preparation-failure feedback.
      error.conversation = prepared.conversation;
      throw error;
    }
  }
  if (req.op === "create") {
    const text = clean(req.text).trim();
    if (!text || [...text].length > 8000)
      throw new Error("Write a task between 1 and 8,000 characters.");
    if (!req.channel || !req.key)
      throw new Error("Task channel and submission key are required.");
    const project = resolveProject(req.project);
    const agent = provider(req.provider);
    const model = modelLabel(req.model);
    const attachments = await uploadImages(team, req, imageRoot);
    const me = await team.me();
    const { task } = await team.tasks.create(req.channel, {
      text,
      id: req.key,
      for: `${me.principal}+codex-worker`,
      noSync: true,
      labels: {
        project: "sprout",
        source: "sprout-ui",
        [PROJECT_LABEL]: project.id,
        [PROVIDER_LABEL]: agent.id,
        [MODEL_LABEL]: model,
      },
      attachments,
    });
    return {
      item: summary({ message: task, replies: 0 }),
      channel: clean(task.channel),
    };
  }
  if (req.op === "reply") {
    const result = await sendFeedback(
      team,
      { ...req, text: clean(req.text) },
      imageRoot,
    );
    const item = summary({
      message: {
        ...result.page.message,
        task: result.effective.task,
        labels: {
          ...result.page.message.labels,
          [PROVIDER_LABEL]: result.effective.provider,
          [MODEL_LABEL]: modelLabel(result.effective.model),
        },
      },
      replies: result.page.replies.filter((m) => !m.deleted).length,
    });
    return {
      item,
      channel: result.page.message.channel,
      reply_id: result.reply_id,
      run_id: result.run_id,
    };
  }
  if (req.op === "image") {
    const page = await team.thread(req.thread);
    const attachment = [page.message, ...page.replies]
      .flatMap((m) => m.attachments ?? [])
      .find((a) => a.id === req.id);
    if (!attachment)
      throw new Error("Image is no longer attached to this conversation.");
    return {
      path: await downloadImage(team, attachment),
      name: clean(attachment.name),
    };
  }
  if (req.op === "init") {
    const me = await team.me();
    const channels = [];
    const seen = new Set();
    let after;
    do {
      const page = await team.channels({
        limit: 100,
        ...(after ? { after } : {}),
      });
      channels.push(
        ...page.channels
          .filter((c) => c.path !== RUN_CHANNEL)
          .map((c) => ({ path: clean(c.path) })),
      );
      after = page.next;
      if (after && seen.has(after))
        throw new Error("Board returned a repeated channel cursor.");
      seen.add(after);
    } while (after);
    return {
      identity: clean(me.name || me.handle || me.actor),
      providers: catalog(),
      projects: listProjects(),
      channels: channels.sort((a, b) => a.path.localeCompare(b.path)),
    };
  }
  if (req.op === "list") {
    const tasks = req.mode !== "threads";
    const args = { channel: req.channel, limit: 50 };
    if (tasks) {
      args.state = "all";
      if (req.cursor) args.after = req.cursor;
    } else if (req.cursor) args.before = req.cursor;
    const page = tasks ? await team.tasks.list(args) : await team.threads(args);
    const entries = tasks ? page.tasks : [...page.threads].reverse();
    const items = [];
    for (let i = 0; i < entries.length; i += 4) {
      items.push(
        ...(await Promise.all(
          entries.slice(i, i + 4).map(async (entry) => {
            const effective = await effectiveTask(team, entry.message);
            return summary({
              ...entry,
              message: {
                ...entry.message,
                task: effective.task,
                labels: {
                  ...entry.message.labels,
                  [PROVIDER_LABEL]: effective.provider,
                  [MODEL_LABEL]: modelLabel(effective.model),
                },
              },
            });
          }),
        )),
      );
    }
    return {
      items,
      next: tasks
        ? entries.length === 50
          ? page.cursor
          : null
        : (page.older ?? null),
    };
  }
  if (req.op === "thread") {
    const page = await team.thread(req.id);
    const effective = await effectiveTask(team, page.message);
    const task = effective.task;
    return {
      id: String(page.message.id),
      messages: [page.message, ...page.replies].map(message),
      reply_count: page.replies.filter((m) => !m.deleted).length,
      state: taskState(task),
      lease_until: leaseUntil(task),
      details: task
        ? [
            effective.run ? "Follow-up request: " + taskState(task) : "",
            task.claim && taskState(task) === "claimed"
              ? `Claimed by ${task.claim.actor} until ${task.claim.until}`
              : "",
            task.needs?.length ? `Depends on: ${task.needs.join(", ")}` : "",
            task.blocked ? `Blocked: ${task.blocked.reason}` : "",
            task.result ? `Result: ${task.result}` : "",
            task.reason ? `Reason: ${task.reason}` : "",
          ]
            .filter(Boolean)
            .map(clean)
        : [],
    };
  }
  throw new Error(`Unsupported Board operation: ${req.op}`);
}

// Deliberately fictional fixtures, available only with --demo. Never a fallback.
export async function demo(req) {
  if (req.op === "create" || req.op === "reply")
    throw new Error(
      "Sending tasks and replies is disabled in demo mode. Press d to connect to your live Board.",
    );
  const channels = ["development", "design", "incidents"];
  const all = {
    development: [
      {
        id: "demo-search",
        title: "Add search to Pitwall",
        state: "claimed",
        author: "Aryan",
        replies: 2,
        labels: ["kind=feature"],
      },
      {
        id: "demo-tests",
        title: "Verify terminal restoration",
        state: "completed",
        author: "Codex",
        replies: 1,
        labels: ["kind=test"],
      },
      {
        id: "demo-layout",
        title: "Review the three-column layout",
        state: "blocked",
        author: "Claude",
        replies: 1,
        labels: ["kind=review"],
      },
    ],
    design: [
      {
        id: "demo-color",
        title: "Choose status colors",
        state: "ready",
        author: "Designer",
        replies: 0,
        labels: [],
      },
    ],
    incidents: [],
  };
  if (req.op === "init")
    return {
      identity: "Demo data · not connected",
      providers: catalog(),
      projects: listProjects(),
      channels: channels.map((path) => ({ path })),
    };
  if (req.op === "list") return { items: all[req.channel] ?? [], next: null };
  const item = Object.values(all)
    .flat()
    .find((i) => i.id === req.id);
  if (!item) throw new Error("Unknown demo thread");
  return {
    id: item.id,
    state: item.state,
    reply_count: item.replies,
    details: ["Fictional sample conversation. No Board messages were created."],
    messages: [
      {
        id: item.id,
        author: item.author,
        time: "2026-11-21T10:00:00Z",
        text:
          item.title +
          "\n\nThis is sample content for exploring Pitwall’s terminal interface.\n\nUse Tab to move between columns. Arrow keys select channels and tasks; in the conversation they scroll.\n\nPress v to switch between tasks and all threads. Press r to refresh, m for another page, and t for the light theme.",
        attachments: [],
      },
      ...Array.from({ length: item.replies }, (_, n) => ({
        id: `${item.id}-${n}`,
        author: n ? "Reviewer" : "Codex",
        time: `2026-11-21T10:0${n + 1}:00Z`,
        text: n
          ? "The thread keeps the request, progress and review together.\n\nThis view is read-only; it never claims tasks or posts replies."
          : "The interface is drawn with Ratatui.\n\nA background worker asks the official Board SDK for JSON, then the Rust application turns those records into lists and paragraphs.\n\nNetwork work never blocks the keyboard loop.",
        attachments: [],
      })),
    ],
  };
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  let request;
  try {
    request = JSON.parse(
      process.argv[2] === "-" ? readFileSync(0, "utf8") : process.argv[2],
    );
  } catch {
    console.log(JSON.stringify({ ok: false, error: "Invalid Board request" }));
    process.exit(1);
  }
  if (request.op === "queue_wait") {
    const abort = new AbortController();
    const stop = () => {
      abort.abort();
      process.exit(0);
    };
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
    try {
      const team = board({
        team: request.team,
        name: request.board || undefined,
        ensure: false,
      }).as(null);
      const task_id = await waitForTask(team, abort.signal);
      console.log(JSON.stringify({ ok: true, data: { task_id } }));
      process.exit(0);
    } catch (error) {
      console.log(
        JSON.stringify({
          ok: false,
          error: clean(error?.message || "Board task wait failed").slice(
            0,
            600,
          ),
        }),
      );
      process.exit(1);
    }
  }
  if (request.op === "events") {
    const abort = new AbortController();
    process.on("SIGTERM", () => {
      abort.abort();
      process.exit(0);
    });
    process.on("SIGINT", () => {
      abort.abort();
      process.exit(0);
    });
    try {
      const team = board({
        team: request.team,
        name: request.board || undefined,
        ensure: false,
      }).as(null);
      await follow(
        team,
        (notice) => process.stdout.write(JSON.stringify(notice) + "\n"),
        abort.signal,
      );
      process.exit(0);
    } catch (error) {
      console.log(
        JSON.stringify({
          kind: "error",
          message: clean(error?.message || "Board feed failed").slice(0, 600),
        }),
      );
      process.exit(1);
    }
  }
  const timeout = setTimeout(
    () => {
      console.log(
        JSON.stringify({
          ok: false,
          error: "Board request timed out. Retry the same operation.",
        }),
      );
      process.exit(1);
    },
    ["create", "reply", "prepare"].includes(request.op) ? 120_000 : 20_000,
  );
  try {
    const req = request;
    const data = req.demo
      ? await demo(req)
      : await execute(
          board({
            team: req.team,
            name: req.board || undefined,
            ensure: false,
          }).as(null),
          req,
        );
    console.log(JSON.stringify({ ok: true, data }));
    clearTimeout(timeout);
    process.exit(0);
  } catch (error) {
    // Report no headers, tokens or stack. Only the provider's user-facing error.
    let text = clean(error?.message || "Board request failed");
    if (/not logged|login|token.*expir|unauthorized|credential/i.test(text))
      text =
        "Vercel login is missing or expired. Run board-runtime/node_modules/.bin/vercel login, then retry.";
    console.log(
      JSON.stringify({
        ok: false,
        error: text.slice(0, 600),
        ...(error.conversation ? { conversation: error.conversation } : {}),
      }),
    );
    clearTimeout(timeout);
    process.exit(1);
  }
}
