import test from "node:test";
import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  symlinkSync,
  renameSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { registerProject, resolveProject } from "./projects.mjs";
import { board } from "@vercel/board";
import { local } from "@vercel/board/backend-local";
import { execute } from "./adapter.mjs";
import { prepareRun } from "./feedback.mjs";
import { scanQueue } from "./queue.mjs";
import { state, project } from "./project-test-fixture.mjs";

test("project registration canonicalizes aliases and refuses unknown or replaced folders", () => {
  const root = mkdtempSync(join(tmpdir(), "pitwall-project-"));
  try {
    const folder = join(root, "[test] project ' 日本語");
    mkdirSync(folder);
    const registered = registerProject(folder, state);
    symlinkSync(folder, join(root, "alias"));
    assert.equal(registerProject(join(root, "alias"), state).id, registered.id);
    assert.equal(resolveProject(registered.id, state).path, registered.path);
    assert.throws(
      () => resolveProject("../../escape", state),
      /Open a project/,
    );
    renameSync(folder, folder + "-old");
    mkdirSync(folder);
    assert.throws(
      () => resolveProject(registered.id, state),
      /changed or was replaced/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("no project means no agent job; a conversation's folder cannot be retargeted", async () => {
  const backend = local({ memory: true });
  const team = board({ backend });
  try {
    await assert.rejects(
      execute(team, {
        op: "create",
        channel: "general",
        key: "missing",
        text: "Change something",
      }),
      /Open a project/,
    );
    const root = await execute(team, {
      op: "create",
      channel: "general",
      key: "project-root",
      text: "Work here",
      project: project.id,
    });
    const next = await execute(team, {
      op: "reply",
      thread: root.item.id,
      key: "same-project",
      text: "Continue",
      project: project.id,
      provider: "antigravity",
    });
    assert.equal(
      (await prepareRun(team, next.run_id)).project.path,
      project.path,
    );
    await assert.rejects(
      execute(team, {
        op: "reply",
        thread: root.item.id,
        key: "retarget",
        text: "Elsewhere",
        project: "b".repeat(64),
      }),
      /cannot change/,
    );
    const legacy = (
      await team.tasks.create("general", {
        text: "Old self-edit task",
        for: `${(await team.me()).principal}+codex-worker`,
        labels: { project: "sprout", source: "sprout-ui" },
      })
    ).task;
    await assert.rejects(prepareRun(team, legacy.id), /no matching project/);
    await assert.rejects(
      execute(team, {
        op: "reply",
        thread: legacy.id,
        key: "legacy-run",
        text: "Continue",
      }),
      /Open a project/,
    );
    assert.equal(
      (
        await execute(team, {
          op: "reply",
          thread: legacy.id,
          key: "legacy-comment",
          text: "Comment",
          run_worker: false,
        })
      ).run_id,
      null,
    );
    await team.tasks.cancel(root.item.id, "finished");
    await team.tasks.cancel(next.run_id, "finished");
    assert.equal(
      (await scanQueue(team)).task_id,
      null,
      "legacy projectless jobs must never be claimed",
    );
  } finally {
    await backend.close();
  }
});
