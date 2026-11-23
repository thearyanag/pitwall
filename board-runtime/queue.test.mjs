import { labels as projectLabels } from "./project-test-fixture.mjs";
import test from "node:test";
import assert from "node:assert/strict";
import { board } from "@vercel/board";
import { local } from "@vercel/board/backend-local";
import { scanQueue, waitForTask } from "./queue.mjs";

const labels = { project: "sprout", source: "sprout-ui", ...projectLabels };
async function fixture(fn) {
  const backend = local({ memory: true });
  const team = board({ backend });
  try {
    await fn(team);
  } finally {
    await backend.close();
  }
}
function wrap(team, overrides) {
  return new Proxy(team, {
    get(target, key) {
      if (key in overrides) return overrides[key];
      const value = target[key];
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

test("waiting takes only explicitly assigned Pitwall tasks across channels", async () =>
  fixture(async (team) => {
    const actor = `${(await team.me()).principal}+codex-worker`;
    await team.tasks.create("general", { text: "Unassigned", labels });
    await team.tasks.create("general", {
      text: "Another worker",
      labels,
      for: actor + "-other",
    });
    await team.tasks.create("general", {
      text: "Different project",
      labels: { ...labels, project: "other" },
      for: actor,
    });
    assert.equal((await scanQueue(team)).task_id, null);
    const task = (
      await team.tasks.create("sprout-runs", {
        text: "Eligible follow-up",
        labels,
        for: actor,
      })
    ).task;
    assert.equal(
      await waitForTask(team, new AbortController().signal),
      task.id,
    );
    assert.equal(
      (await team.thread(task.id)).message.task.state,
      "ready",
      "waiting does not claim; the official CLI does",
    );
  }));

test(
  "idle queue stays inside Board wait and wakes when a task arrives",
  { timeout: 10000 },
  async () =>
    fixture(async (team) => {
      const actor = `${(await team.me()).principal}+codex-worker`;
      const entered = Promise.withResolvers();
      let scans = 0;
      const wrapped = wrap(team, {
        tasks: {
          ...team.tasks,
          list: async (...args) => {
            scans++;
            return team.tasks.list(...args);
          },
        },
        wait: (...args) => {
          entered.resolve();
          return team.wait(...args);
        },
      });
      const abort = new AbortController();
      const pending = waitForTask(wrapped, abort.signal);
      try {
        await entered.promise;
        assert.equal(scans, 1);
        const task = (
          await team.tasks.create("another-channel", {
            text: "Arrived while waiting",
            labels,
            for: actor,
          })
        ).task;
        assert.equal(await pending, task.id);
        assert.equal(scans, 2);
      } finally {
        abort.abort();
        await pending.catch(() => {});
      }
    }),
);

test(
  "a task arriving between the empty scan and wait is not missed",
  { timeout: 10000 },
  async () =>
    fixture(async (team) => {
      const actor = `${(await team.me()).principal}+codex-worker`;
      let task;
      let scans = 0;
      const wrapped = wrap(team, {
        tasks: {
          ...team.tasks,
          list: async (...args) => {
            const page = await team.tasks.list(...args);
            if (++scans === 1)
              task = (
                await team.tasks.create("general", {
                  text: "Snapshot/wait race",
                  labels,
                  for: actor,
                })
              ).task;
            return page;
          },
        },
      });
      const abort = new AbortController();
      try {
        assert.equal(await waitForTask(wrapped, abort.signal), task.id);
        assert.equal(scans, 2);
      } finally {
        abort.abort();
      }
    }),
);

test(
  "cancellation stops a built-in wait without claiming a task",
  { timeout: 10000 },
  async () =>
    fixture(async (team) => {
      const entered = Promise.withResolvers();
      const abort = new AbortController();
      const wrapped = wrap(team, {
        wait: (...args) => {
          entered.resolve();
          return team.wait(...args);
        },
      });
      const pending = waitForTask(wrapped, abort.signal);
      await entered.promise;
      abort.abort();
      assert.equal(await pending, null);
    }),
);

test("known lease expiry bounds the wait even if the backend emits no event", async () => {
  const task = {
    id: "expired-task",
    labels,
    task: {
      owner: "owner",
      for: "owner+codex-worker",
      state: "claimed",
      claim: { until: new Date(Date.now() + 30000).toISOString() },
    },
  };
  let waits = 0;
  const team = {
    me: async () => ({ principal: "owner" }),
    poll: async () => ({ cursor: "head" }),
    tasks: { list: async () => ({ tasks: [{ message: task }] }) },
    wait: async (query, options) => {
      waits++;
      assert.equal(query.after, "head");
      assert.ok(options.timeout > 0 && options.timeout <= 30000);
      task.task.claim.until = new Date(1).toISOString();
      return null;
    },
  };
  assert.equal(
    await waitForTask(team, new AbortController().signal),
    "expired-task",
  );
  assert.equal(waits, 1);
});

test("queue scans paginate past blocked work and reject a repeated cursor", async () => {
  const blocked = {
    id: "blocked",
    labels,
    task: { state: "blocked", owner: "owner", for: "owner+codex-worker" },
  };
  let calls = 0;
  const team = {
    me: async () => ({ principal: "owner" }),
    tasks: {
      list: async () =>
        ++calls === 1
          ? {
              tasks: Array.from({ length: 100 }, () => ({ message: blocked })),
              cursor: "next",
            }
          : {
              tasks: [
                {
                  message: {
                    ...blocked,
                    id: "ready",
                    task: { ...blocked.task, state: "ready" },
                  },
                },
              ],
              cursor: null,
            },
    },
  };
  assert.equal((await scanQueue(team)).task_id, "ready");
  assert.equal(calls, 2);
  team.tasks.list = async () => ({
    tasks: Array.from({ length: 100 }, () => ({ message: blocked })),
    cursor: "repeat",
  });
  await assert.rejects(scanQueue(team), /repeated task cursor/);
});
