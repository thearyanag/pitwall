import { scanQueue } from "./queue.mjs";
import test from "node:test";
import assert from "node:assert/strict";
import { board } from "@vercel/board";
import { local } from "@vercel/board/backend-local";
import { execute } from "./adapter.mjs";
import { prepareRun, RUN_CHANNEL, RUN_LABEL } from "./feedback.mjs";

async function fixture(fn) {
  const backend = local({ memory: true });
  const team = board({ backend });
  try {
    await fn(team);
  } finally {
    await backend.close();
  }
}
async function original(team) {
  return (
    await team.tasks.create("general", {
      text: "Original Pitwall task",
      labels: { project: "sprout", source: "sprout-ui" },
    })
  ).task;
}
const request = (id, key, text) => ({ op: "reply", thread: id, key, text });

test("model selection is per run, retry-safe, and does not split the agent session key", async () =>
  fixture(async (team) => {
    const root = await execute(team, {
      op: "create",
      channel: "general",
      text: "First",
      key: "model-root",
      provider: "codex",
      model: "model-a",
    });
    const initial = await prepareRun(team, root.item.id);
    assert.equal(initial.model, "model-a");
    const req = {
      ...request(root.item.id, "model-next", "Continue"),
      provider: "codex",
      model: "model-b",
    };
    const next = await execute(team, req);
    const prepared = await prepareRun(team, next.run_id);
    assert.equal(prepared.model, "model-b");
    assert.equal(prepared.provider_key, initial.provider_key);
    assert.equal((await execute(team, req)).run_id, next.run_id);
    await assert.rejects(
      execute(team, { ...req, model: "model-c" }),
      /Cannot change a submitted reply/,
    );
    const reset = await execute(team, {
      ...request(root.item.id, "model-default", "Default now"),
      provider: "codex",
      model: null,
    });
    assert.equal((await prepareRun(team, reset.run_id)).model, null);
    assert.equal((await prepareRun(team, next.run_id)).model, "model-b");
    const list = await execute(team, {
      op: "list",
      channel: "general",
      mode: "tasks",
    });
    assert.ok(list.items[0].labels.includes("pitwall.model=default"));
  }));

test("Codex -> Antigravity -> Codex keeps one root and immutable per-run agent choices", async () =>
  fixture(async (team) => {
    const root = await execute(team, {
      op: "create",
      channel: "general",
      key: "switch-root",
      text: "Original",
      provider: "codex",
    });
    const reviewReq = {
      ...request(root.item.id, "switch-review", "Review with Antigravity"),
      provider: "antigravity",
    };
    const review = await execute(team, reviewReq);
    const back = await execute(team, {
      ...request(root.item.id, "switch-back", "Codex, apply the review"),
      provider: "codex",
    });
    assert.equal(
      (await prepareRun(team, review.run_id)).provider,
      "antigravity",
      "later selections must not reroute queued work",
    );
    assert.equal((await prepareRun(team, back.run_id)).provider, "codex");
    assert.equal(
      (await prepareRun(team, review.run_id)).conversation,
      root.item.id,
    );
    assert.equal(
      (await prepareRun(team, back.run_id)).conversation,
      root.item.id,
    );
    const retry = await execute(team, reviewReq);
    assert.equal(retry.run_id, review.run_id);
    assert.equal(
      (await team.thread(root.item.id)).message.labels[RUN_LABEL],
      back.run_id,
    );
    await assert.rejects(
      execute(team, { ...reviewReq, provider: "codex" }),
      /Cannot change a submitted reply/,
    );
    await assert.rejects(
      execute(team, { ...reviewReq, run_worker: false }),
      /Cannot change a submitted reply/,
    );
    const history = await prepareRun(team, back.run_id);
    assert.ok(
      history.history.some(
        (m) => m.id === review.reply_id && m.provider === "antigravity",
      ),
    );
    assert.ok(!history.history.some((m) => m.id === back.reply_id));
    const page = await team.tasks.list({ state: "all", channel: RUN_CHANNEL });
    assert.equal(page.tasks.length, 2);
  }));

test("retry after a lost switched-run response retains the selected agent", async () =>
  fixture(async (team) => {
    const root = await original(team);
    let calls = 0;
    const wrapped = new Proxy(team, {
      get(target, prop) {
        if (prop === "tasks")
          return {
            ...team.tasks,
            create: async (...args) => {
              const result = await team.tasks.create(...args);
              if (++calls === 1) throw new Error("lost switched-run response");
              return result;
            },
          };
        const value = target[prop];
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    const req = {
      ...request(root.id, "switch-retry", "Review it"),
      provider: "antigravity",
    };
    await assert.rejects(execute(wrapped, req), /lost switched/);
    await assert.rejects(
      execute(team, { ...req, provider: "codex" }),
      /Cannot change/,
    );
    const result = await execute(team, req);
    assert.equal(
      (await prepareRun(team, result.run_id)).provider,
      "antigravity",
    );
    assert.equal((await team.thread(root.id)).replies.length, 1);
  }));

test("agent choice survives creation and retries; omitted reply selection uses the current agent", async () =>
  fixture(async (team) => {
    for (const provider of ["codex", "claude", "grok", "antigravity"]) {
      const req = {
        op: "create",
        channel: "general",
        text: "An agent task",
        key: `agent-${provider}`,
        provider,
      };
      const root = await execute(team, req);
      assert.equal((await execute(team, req)).item.id, root.item.id);
      const prepared = await prepareRun(team, root.item.id);
      assert.equal(prepared.provider, provider);
      assert.equal(prepared.provider_key.length, 64);
      const follow = await execute(team, {
        ...request(root.item.id, `follow-${provider}`, "Revise"),
        provider,
      });
      const next = await prepareRun(team, follow.run_id);
      assert.equal(next.provider, provider);
      assert.equal(next.provider_key, prepared.provider_key);
    }
    await assert.rejects(
      execute(team, {
        op: "create",
        channel: "general",
        text: "Bad",
        key: "unknown-provider",
        provider: "not-configured",
      }),
      /Unknown agent/,
    );
  }));

test("completed tasks get one follow-up job while feedback stays in the original thread", async () =>
  fixture(async (team) => {
    const root = await original(team);
    const claim = await team.tasks.claim(root.id);
    await claim.complete("Initial result");
    const req = request(
      root.id,
      "reply-one",
      "Please clarify the keyboard shortcut.",
    );
    const first = await execute(team, req);
    const retry = await execute(team, req);
    assert.equal(first.reply_id, retry.reply_id);
    assert.equal(first.run_id, retry.run_id);
    const thread = await team.thread(root.id);
    assert.equal(thread.message.task.state, "completed");
    assert.equal(thread.replies.length, 1);
    assert.equal(thread.replies[0].text, req.text);
    assert.equal(thread.message.labels[RUN_LABEL], first.run_id);
    assert.equal((await scanQueue(team)).task_id, first.run_id);
    assert.equal(
      (await execute(team, { op: "list", channel: "general", mode: "tasks" }))
        .items[0].state,
      "ready",
    );
    assert.ok(
      !(await execute(team, { op: "init" })).channels.some(
        (c) => c.path === RUN_CHANNEL,
      ),
    );
    const prepared = await prepareRun(team, first.run_id);
    assert.equal(prepared.conversation, root.id);
    assert.equal(prepared.request_text, req.text);
    assert.equal(prepared.follow_up, true);
    // The local backend has a different profile-ID scheme; simulate the queued run as the owner.
    await team.tasks.assign(first.run_id, null);
    const follow = await team.tasks.claim(first.run_id);
    await follow.complete("Answered");
    assert.equal(
      (await execute(team, { op: "thread", id: root.id })).state,
      "completed",
    );
  }));

test("feedback during a run keeps its claim intact and later feedback is deferred to its own run", async () =>
  fixture(async (team) => {
    const root = await original(team);
    const active = await team.tasks.claim(root.id);
    const a = request(root.id, "feedback-a", "First requested change");
    const b = request(root.id, "feedback-b", "Second requested change");
    const first = await execute(team, a);
    const second = await execute(team, b);
    await team.reply(root.id, "Worker finished the current implementation.");
    assert.equal((await team.thread(root.id)).message.task.state, "claimed");
    const prepared = await prepareRun(team, first.run_id);
    assert.equal(prepared.request_text, a.text);
    assert.ok(prepared.context.includes("Worker finished"));
    assert.ok(!prepared.context.includes(b.text));
    await execute(team, a);
    assert.equal(
      (await team.thread(root.id)).message.labels[RUN_LABEL],
      second.run_id,
    );
    await active.complete("Done");
  }));

test("lost job-creation response retries without duplicate feedback or duplicate jobs", async () =>
  fixture(async (team) => {
    const root = await original(team);
    let creates = 0;
    const wrapped = new Proxy(team, {
      get(target, prop) {
        if (prop === "tasks")
          return {
            ...team.tasks,
            create: async (...args) => {
              const result = await team.tasks.create(...args);
              if (++creates === 1) throw new Error("lost response");
              return result;
            },
          };
        const value = target[prop];
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    const req = request(root.id, "reply-retry", "Make a small change.");
    await assert.rejects(execute(wrapped, req), /lost response/);
    await execute(wrapped, req);
    assert.equal((await team.thread(root.id)).replies.length, 1);
    assert.equal(
      (await team.tasks.list({ channel: RUN_CHANNEL, state: "all" })).tasks
        .length,
      1,
    );
  }));

test("reply requires a Pitwall task owned by the caller", async () =>
  fixture(async (team) => {
    const message = await team.post("general", "Ordinary conversation");
    await assert.rejects(
      execute(team, request(message.id, "not-task", "Hi")),
      /Select one of your Pitwall tasks/,
    );
  }));

test("feedback images remain attached to the original thread and enter the next run context", async () =>
  fixture(async (team) => {
    const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } =
      await import("node:fs");
    const { tmpdir } = await import("node:os");
    const { join } = await import("node:path");
    const root = await original(team);
    const directory = mkdtempSync(join(tmpdir(), "sprout-feedback-image-"));
    const key = "reply-image";
    mkdirSync(join(directory, key));
    const path = join(directory, key, "image.png");
    writeFileSync(
      path,
      readFileSync(new URL("../test-assets/vision-card.png", import.meta.url)),
    );
    try {
      const result = await execute(
        team,
        {
          ...request(root.id, key, "Use this screenshot for the revision."),
          provider: "antigravity",
          attachments: [{ path, name: "revision.png" }],
        },
        directory,
      );
      const thread = await team.thread(root.id);
      assert.equal(thread.replies[0].attachments.length, 1);
      const context = await prepareRun(team, result.run_id);
      assert.equal(context.images.length, 1);
      assert.equal(context.images[0].id, thread.replies[0].attachments[0].id);
      assert.equal(context.provider, "antigravity");
      const prepared = await execute(team, {
        op: "prepare",
        task: result.run_id,
        dir: join(directory, "run"),
      });
      assert.equal(prepared.conversation, root.id);
      assert.equal(
        prepared.request_text,
        "Use this screenshot for the revision.",
      );
      assert.deepEqual(
        readFileSync(prepared.images[0].path),
        readFileSync(path),
      );
      const back = await execute(team, {
        ...request(root.id, "image-switch-back", "Use that screenshot again"),
        provider: "codex",
      });
      const returned = await prepareRun(team, back.run_id);
      assert.equal(returned.provider, "codex");
      assert.equal(returned.images[0].id, context.images[0].id);
      assert.deepEqual(returned.new_image_ids, []);
      const failing = new Proxy(team, {
        get(target, prop) {
          if (prop === "download")
            return async () => {
              throw new Error("download unavailable");
            };
          const value = target[prop];
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
      await assert.rejects(
        execute(failing, {
          op: "prepare",
          task: result.run_id,
          dir: join(directory, "failed"),
        }),
        (error) =>
          error.message === "download unavailable" &&
          error.conversation === root.id,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  }));

test("comment only posts once without queueing work or changing task status", async () =>
  fixture(async (team) => {
    const root = await original(team);
    const claim = await team.tasks.claim(root.id);
    await claim.complete("done");
    const req = {
      ...request(root.id, "comment-only", "Thanks, this looks good."),
      run_worker: false,
    };
    const a = await execute(team, req);
    const b = await execute(team, req);
    assert.equal(a.reply_id, b.reply_id);
    assert.equal(a.run_id, null);
    const thread = await team.thread(root.id);
    assert.equal(thread.replies.length, 1);
    assert.equal(thread.replies[0].labels["sprout.feedback"], "comment");
    assert.equal(thread.message.task.state, "completed");
    assert.equal(thread.message.labels[RUN_LABEL], undefined);
    assert.equal(
      (await team.tasks.list({ channel: RUN_CHANNEL, state: "all" })).tasks
        .length,
      0,
    );
    assert.equal((await scanQueue(team)).task_id, null);
  }));

test("run preparation rejects a labeled non-task and reads an original task once", async () =>
  fixture(async (team) => {
    const post = await team.post("general", {
      text: "Not a task",
      labels: { project: "sprout" },
    });
    await assert.rejects(
      prepareRun(team, post.id),
      /Invalid task\/conversation/,
    );
    const task = await original(team);
    let reads = 0;
    const counting = new Proxy(team, {
      get(target, prop) {
        if (prop === "thread")
          return (...args) => {
            reads++;
            return team.thread(...args);
          };
        const value = target[prop];
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    const context = await prepareRun(counting, task.id);
    assert.equal(context.conversation, task.id);
    assert.equal(reads, 1);
  }));
