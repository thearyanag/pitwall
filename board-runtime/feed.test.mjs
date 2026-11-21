import test from "node:test";
import assert from "node:assert/strict";
import { board } from "@vercel/board";
import { local } from "@vercel/board/backend-local";
import { follow, notice } from "./feed.mjs";

test("feed routes replies, task changes and channel changes without exposing message text", () => {
  assert.deepEqual(
    notice({
      type: "message.posted",
      message: { id: "reply", re: "root", channel: "general", text: "private" },
    }),
    {
      kind: "changed",
      channel: "general",
      thread: "root",
      parent: null,
      root: false,
    },
  );
  assert.equal(
    notice({
      type: "task.completed",
      message: {
        id: "run",
        channel: "sprout-runs",
        labels: { "sprout.conversation": "root" },
      },
    }).parent,
    "root",
  );
  assert.deepEqual(notice({ type: "channel.created" }), { kind: "reset" });
  assert.equal(notice({ type: "actor.seen" }), null);
});

test(
  "official SDK feed observes a reply after subscription and stops on abort",
  { timeout: 10000 },
  async () => {
    const backend = local({ memory: true });
    const team = board({ backend });
    const root = await team.tasks.create("general", {
      text: "Feed regression task",
      labels: { project: "sprout" },
    });
    const abort = new AbortController();
    const ready = Promise.withResolvers();
    const received = Promise.withResolvers();
    const run = follow(
      team,
      (entry) => {
        if (entry.kind === "state" && entry.state === "live") ready.resolve();
        if (
          entry.kind === "changed" &&
          entry.thread === root.task.id &&
          !entry.root
        ) {
          received.resolve(entry);
          abort.abort();
        }
      },
      abort.signal,
    );
    try {
      await ready.promise;
      await team.reply(root.task.id, "A new worker reply");
      assert.equal((await received.promise).channel, "general");
      await run;
    } finally {
      abort.abort();
      await run;
      await backend.close();
    }
  },
);
