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
          attachments: [{ path, name: "revision.png" }],
        },
        directory,
      );
      const thread = await team.thread(root.id);
      assert.equal(thread.replies[0].attachments.length, 1);
      const context = await prepareRun(team, result.run_id);
      assert.equal(context.images.length, 1);
      assert.equal(context.images[0].id, thread.replies[0].attachments[0].id);
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
