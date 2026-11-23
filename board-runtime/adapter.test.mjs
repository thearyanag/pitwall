import {
  project as testProject,
  labels as projectLabels,
} from "./project-test-fixture.mjs";
import { scanQueue } from "./queue.mjs";
import test from "node:test";
import assert from "node:assert/strict";
import { board } from "@vercel/board";
import { local } from "@vercel/board/backend-local";
import { execute as executeRaw, summary, clean } from "./adapter.mjs";

test("actual Board SDK: channels, tasks, replies, state changes, pagination and empty state", async () => {
  const backend = local({ memory: true });
  const team = board({ backend });
  try {
    const task = (
      await team.tasks.create("development", "Build the Board reader")
    ).task;
    await team.reply(task.id, "The three columns are ready.");
    await team.reply(task.id, "Keyboard navigation verified.");
    const init = await execute(team, { op: "init" });
    assert.deepEqual(init.channels, [{ path: "development" }]);
    const list = await execute(team, {
      op: "list",
      channel: "development",
      mode: "tasks",
    });
    assert.equal(list.items[0].id, task.id);
    assert.equal(list.items[0].state, "ready");
    assert.equal(list.items[0].replies, 2);
    const thread = await execute(team, { op: "thread", id: task.id });
    assert.equal(thread.messages.length, 3);
    assert.equal(thread.messages[2].text, "Keyboard navigation verified.");
    const claim = await team.tasks.claim(task.id);
    await claim.complete("Verified locally");
    assert.equal(
      (await execute(team, { op: "thread", id: task.id })).state,
      "completed",
    );
    for (let n = 0; n < 51; n++)
      await team.tasks.create("development", `Task ${n}`);
    const first = await execute(team, {
      op: "list",
      channel: "development",
      mode: "tasks",
    });
    assert.equal(first.items.length, 50);
    assert.ok(first.next);
    const second = await execute(team, {
      op: "list",
      channel: "development",
      mode: "tasks",
      cursor: first.next,
    });
    assert.equal(second.items.length, 2);
    assert.equal(second.next, null);
    assert.equal(
      new Set([...first.items, ...second.items].map((i) => i.id)).size,
      52,
    );
    await team.createChannel("empty");
    assert.deepEqual(
      (await execute(team, { op: "list", channel: "empty", mode: "tasks" }))
        .items,
      [],
    );
    const threads = await execute(team, {
      op: "list",
      channel: "development",
      mode: "threads",
    });
    assert.equal(threads.items.length, 50);
    assert.ok(threads.next);
    const older = await execute(team, {
      op: "list",
      channel: "development",
      mode: "threads",
      cursor: threads.next,
    });
    assert.equal(older.items.length, 2);
  } finally {
    await backend.close();
  }
});

test("expired claims display ready, and control codes cannot reach the terminal", () => {
  assert.equal(
    summary({
      message: {
        id: "1",
        text: "Hello\u001b[2J",
        task: { state: "claimed", claim: { until: "2000-01-01T00:00:00Z" } },
      },
    }).state,
    "ready",
  );
  assert.equal(clean("safe\u001b[2J\u009b31m"), "safe[2J31m");
});

test("channel pagination follows next and rejects a repeated cursor", async () => {
  let calls = 0;
  const fake = {
    me: async () => ({ name: "Test" }),
    channels: async () =>
      ++calls === 1
        ? { channels: [{ path: "a" }], next: "page2" }
        : { channels: [{ path: "b" }], next: null },
  };
  assert.equal((await execute(fake, { op: "init" })).channels.length, 2);
  fake.channels = async () => ({ channels: [], next: "repeat" });
  await assert.rejects(execute(fake, { op: "init" }), /repeated/);
});

test("composer creates one assigned task, retry deduplicates, queue ignores other tasks", async () => {
  const backend = local({ memory: true });
  const team = board({ backend });
  try {
    const req = {
      op: "create",
      channel: "general",
      text: "Test the task composer\n\nNo cloud writes.",
      key: "sprout-compose-test",
    };
    const first = await execute(team, req);
    const retry = await execute(team, req);
    assert.equal(first.item.id, retry.item.id);
    const page = await team.tasks.list({ state: "all" });
    assert.equal(page.tasks.length, 1);
    assert.equal(page.tasks[0].message.noSync, true);
    assert.equal(page.tasks[0].message.labels.source, "sprout-ui");
    const identity = await team.me();
    const workerActor = `${identity.principal}+codex-worker`;
    assert.equal(page.tasks[0].message.task.for, workerActor);
    assert.equal((await scanQueue(team)).task_id, first.item.id);
    await team.tasks.cancel(first.item.id, "test finished");
    await team.tasks.create("general", {
      text: "Unrelated task",
      for: workerActor,
      labels: { project: "other", source: "sprout-ui" },
    });
    await team.tasks.create("general", {
      text: "Manual task",
      for: workerActor,
      labels: { project: "sprout" },
    });
    assert.equal((await scanQueue(team)).task_id, null);
    await assert.rejects(
      execute(team, { ...req, text: "   ", key: "empty" }),
      /Write a task/,
    );
  } finally {
    await backend.close();
  }
});

test("image upload, uncertain submit retry, and authenticated download preserve the pixels", async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } =
    await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { downloadImage } = await import("./images.mjs");
  const root = mkdtempSync(join(tmpdir(), "sprout-upload-"));
  const key = "sprout-image-retry";
  mkdirSync(join(root, key));
  const original = readFileSync(
    new URL("../test-assets/vision-card.png", import.meta.url),
  );
  const path = join(root, key, "screenshot.png");
  writeFileSync(path, original);
  const backend = local({ memory: true });
  const team = board({ backend });
  let uploads = 0;
  let creates = 0;
  const wrapped = {
    me: () => team.me(),
    upload: async (args) => {
      uploads++;
      return team.upload(args);
    },
    tasks: {
      create: async (...args) => {
        const result = await team.tasks.create(...args);
        if (++creates === 1) throw new Error("simulated lost create response");
        return result;
      },
    },
  };
  const req = {
    op: "create",
    channel: "images",
    text: "Use this screenshot",
    key,
    team: "test",
    attachments: [{ path, name: "screenshot.png" }],
  };
  try {
    await assert.rejects(execute(wrapped, req, root), /lost create response/);
    const result = await execute(wrapped, req, root);
    assert.equal(uploads, 1);
    const list = await team.tasks.list({ state: "all" });
    assert.equal(list.tasks.length, 1);
    const thread = await team.thread(result.item.id);
    assert.equal(thread.message.attachments.length, 1);
    const attachment = thread.message.attachments[0];
    assert.equal(attachment.type, "image/png");
    const downloaded = await downloadImage(
      team,
      attachment,
      join(root, "download"),
    );
    assert.deepEqual(readFileSync(downloaded), original);
    await assert.rejects(
      execute(
        wrapped,
        {
          ...req,
          key: "sprout-invalid",
          attachments: [{ path, name: "outside.png" }],
        },
        root,
      ),
    );
  } finally {
    await backend.close();
    rmSync(root, { recursive: true, force: true });
  }
});

function execute(team, req, imageRoot) {
  return executeRaw(
    team,
    {
      ...(req.op === "create" && !("project" in req)
        ? { project: testProject.id }
        : {}),
      ...req,
    },
    imageRoot,
  );
}
