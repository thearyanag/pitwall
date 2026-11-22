import test from "node:test";
import assert from "node:assert/strict";
import {
  mkdtempSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { setTimeout as delay } from "node:timers/promises";
import { open as acp } from "./providers/acp.mjs";
import { open as grok } from "./providers/grok.mjs";
import { open as codex } from "./providers/codex.mjs";
import { open as claude } from "./providers/claude.mjs";
import { providers } from "./providers/registry.mjs";
import { createHost } from "./providers/host.mjs";
import { acpModelState, modelSelection } from "./providers/models.mjs";
import { spawn } from "node:child_process";
import { once } from "node:events";

const peer = new URL("./providers/fixtures/agent.mjs", import.meta.url)
  .pathname;
const options = { cwd: tmpdir(), writableRoots: [] };
test(
  "ACP supports grouped catalogs, modern and legacy switches, default reset and rejection",
  { timeout: 10000 },
  async () => {
    const grouped = acpModelState({
      configOptions: [
        {
          id: "model",
          type: "select",
          currentValue: "a",
          options: [
            {
              name: "Group",
              group: "g",
              options: [{ value: "a", name: "Model A" }],
            },
          ],
        },
      ],
    });
    assert.deepEqual(
      grouped.models.map((m) => m.id),
      ["a"],
    );
    for (const mode of ["models", "normal"]) {
      const session = await acp(
        instance(mode),
        { ...options, resumeId: "native-session", model: "test-b" },
        host(),
      );
      try {
        assert.equal(session.model, "test-b");
        assert.equal(session.id, "native-session");
        assert.equal(session.defaultModel, "test-a");
      } finally {
        await session.close();
      }
      const current = instance(mode);
      current.env.PITWALL_TEST_MODEL = "test-b";
      const reset = await acp(
        current,
        {
          ...options,
          resumeId: "native-session",
          model: null,
          defaultModel: "test-a",
        },
        host(),
      );
      try {
        assert.equal(reset.model, "test-a");
        assert.equal(reset.resumed, true);
      } finally {
        await reset.close();
      }
    }
    await assert.rejects(
      acp(instance("models"), { ...options, model: "missing" }, host()),
      /no longer offers/,
    );
    await assert.rejects(
      acp(instance("models-refuse"), { ...options, model: "test-b" }, host()),
      /did not confirm/,
    );
    assert.throws(() => modelSelection("bad\nmodel"), /Invalid model/);
  },
);
test("Codex discovers without a thread and changes models without changing native thread ID", async () => {
  const discovery = await codex(
    instance("codex"),
    { ...options, discover: true },
    host(),
  );
  try {
    assert.equal(discovery.id, null);
    assert.equal(discovery.models.length, 2);
  } finally {
    await discovery.close();
  }
  const changed = await codex(
    instance("codex"),
    { ...options, resumeId: "native-thread", model: "test-b" },
    host(),
  );
  try {
    assert.equal(changed.id, "native-thread");
    assert.equal(changed.model, "test-b");
    assert.equal(changed.capabilities.images, false);
  } finally {
    await changed.close();
  }
  const reset = await codex(
    instance("codex"),
    { ...options, resumeId: "native-thread", model: null },
    host(),
  );
  try {
    assert.equal(reset.model, "test-a");
    assert.equal(reset.resumed, true);
  } finally {
    await reset.close();
  }
});
test("Claude selects a supported model before sending the resumed user turn", async () => {
  let chosen,
    closed = false;
  const run = await claude(
    { ...instance("claude"), args: [] },
    { ...options, resumeId: "old", model: "sonnet" },
    host(),
    ({ prompt, options: config }) => {
      assert.equal(config.resume, "old");
      const result = (async function* () {
        for await (const _ of prompt) assert.equal(chosen, "sonnet");
        yield {
          type: "system",
          subtype: "init",
          session_id: "old",
          model: "claude-sonnet-native",
        };
        yield {
          type: "result",
          subtype: "success",
          is_error: false,
          session_id: "old",
          result: "OK",
        };
      })();
      result.supportedModels = async () => [
        { value: "sonnet", displayName: "Sonnet" },
      ];
      result.setModel = async (value) => {
        chosen = value;
      };
      result.close = () => {
        closed = true;
      };
      return result;
    },
  );
  assert.equal(await run.prompt({ text: "Continue", images: [] }), "OK");
  assert.equal(run.model, "claude-sonnet-native");
  await run.close();
  assert.equal(closed, true);
});
test("recorded Grok stream boundaries keep the final answer separate and ignore replay", async () => {
  const h = host(),
    messages = [];
  h.emit = (event) => {
    if (event.type === "message") messages.push(event.text);
  };
  const session = await grok(instance("grok-streams"), options, h);
  try {
    assert.equal(
      await session.prompt({ text: "Synthetic check", images: [] }),
      "PITWALL_OK",
    );
    assert.deepEqual(messages, ["PITWALL_OK", "PITWALL_OK"]);
  } finally {
    await session.close();
  }
});
function instance(mode) {
  return {
    id: "fixture",
    label: "Fixture",
    path: process.execPath,
    args: [peer],
    env: { PITWALL_TEST_MODE: mode },
  };
}
function host() {
  return {
    signal: new AbortController().signal,
    emit() {},
    async ask(_title, _detail, choices) {
      assert.ok(!choices.some((o) => o.id === "native-always"));
      return choices[0].id;
    },
  };
}
test(
  "ACP SDK peer: permission IDs, images, resume replay suppression, and explicit cancellation",
  { timeout: 10000 },
  async () => {
    const h = host(),
      events = [];
    h.emit = (e) => events.push(e);
    const session = await acp(
      instance("normal"),
      { ...options, resumeId: "native-session" },
      h,
    );
    try {
      assert.equal(session.resumed, true);
      assert.equal(session.capabilities.images, true);
      assert.equal(
        events.length,
        0,
        "restored history must not be reposted to Board",
      );
      assert.equal(
        await session.prompt({ text: "Test", images: [] }),
        "native-once",
      );
    } finally {
      await session.close();
    }
    const cancelled = await acp(instance("cancel"), options, host());
    try {
      const pending = cancelled.prompt({ text: "Wait", images: [] });
      const rejection = assert.rejects(pending, /cancelled/);
      await delay(40);
      await cancelled.cancel();
      await rejection;
    } finally {
      await cancelled.close();
    }
  },
);
test(
  "ACP rejects unsupported images and does not claim unsupported resume",
  { timeout: 10000 },
  async () => {
    const session = await acp(instance("no-images"), options, host());
    try {
      await assert.rejects(
        session.prompt({ text: "image", images: [{ path: "missing" }] }),
        /does not advertise image/,
      );
    } finally {
      await session.close();
    }
    const fresh = await acp(
      instance("no-resume"),
      { ...options, resumeId: "old" },
      host(),
    );
    try {
      assert.equal(fresh.resumed, false);
      assert.equal(fresh.capabilities.resume, false);
    } finally {
      await fresh.close();
    }
  },
);
test(
  "provider startup fails promptly on process exit and missing auth",
  { timeout: 10000 },
  async () => {
    await assert.rejects(acp(instance("exit"), options, host()));
    await assert.rejects(acp(instance("auth"), options, host()));
    await assert.rejects(codex(instance("exit"), options, host()), /stopped/);
  },
);
test(
  "Codex native protocol routes approvals and preserves its session ID",
  { timeout: 10000 },
  async () => {
    const session = await codex(
      instance("codex"),
      { ...options, resumeId: "native-thread" },
      host(),
    );
    try {
      assert.equal(session.id, "native-thread");
      assert.equal(session.resumed, true);
      assert.equal(
        await session.prompt({ text: "Test", images: [] }),
        "accept",
      );
    } finally {
      await session.close();
    }
  },
);
test("Claude SDK uses installed binary, native resume, image blocks, permissions, and result errors", async () => {
  let closed = false;
  const image = new URL("../test-assets/vision-card.png", import.meta.url)
    .pathname;
  const run = await claude(
    { ...instance("claude"), args: [] },
    { ...options, resumeId: "previous" },
    host(),
    ({ prompt, options: config }) => {
      assert.equal(config.pathToClaudeCodeExecutable, process.execPath);
      assert.equal(config.resume, "previous");
      assert.equal(config.permissionMode, "default");
      const result = (async function* () {
        for await (const user of prompt)
          assert.equal(user.message.content[1].source.type, "base64");
        assert.equal(
          (await config.canUseTool("Edit", { file_path: "test" })).behavior,
          "allow",
        );
        yield {
          type: "assistant",
          session_id: "previous",
          message: { content: [{ type: "text", text: "Done" }] },
        };
        yield {
          type: "result",
          subtype: "success",
          is_error: false,
          session_id: "previous",
          result: "Done",
        };
      })();
      result.supportedModels = async () => [
        { value: "sonnet", displayName: "Sonnet" },
      ];
      result.setModel = async (model) => assert.equal(model, undefined);
      result.close = () => {
        closed = true;
      };
      return result;
    },
  );
  assert.equal(
    await run.prompt({
      text: "See image",
      images: [{ path: image, type: "image/png" }],
    }),
    "Done",
  );
  assert.equal(run.id, "previous");
  await run.close();
  assert.equal(closed, true);
});
test(
  "local approval responses must match request and expire on cancellation",
  { timeout: 5000 },
  async () => {
    const stateDir = mkdtempSync(join(tmpdir(), "pitwall-approvals-"));
    const controller = new AbortController();
    const h = createHost(
      {
        stateDir,
        team: "test",
        board: "General",
        conversation: "task",
        provider: "test",
        label: "Test",
      },
      controller.signal,
      () => {},
    );
    try {
      const pending = h.ask("Edit", "file", [
        { id: "native", label: "Allow once" },
      ]);
      await delay(20);
      const dir = join(stateDir, "approvals");
      const path = join(
        dir,
        readdirSync(dir).find((n) => n.endsWith(".request.json")),
      );
      const request = JSON.parse(readFileSync(path));
      writeFileSync(
        path.replace(".request.json", ".response.json"),
        JSON.stringify({ id: "wrong", choice: "native" }),
      );
      assert.equal(await pending, null);
      const next = h.ask("Edit", "file", []);
      await delay(20);
      controller.abort();
      assert.equal(await next, null);
      assert.deepEqual(readdirSync(dir), []);
    } finally {
      rmSync(stateDir, { recursive: true, force: true });
    }
  },
);
test("custom ACP agents are configuration only, invalid IDs and drivers fail closed", () => {
  const dir = mkdtempSync(join(tmpdir(), "pitwall-agents-"));
  const file = join(dir, "agents.json");
  try {
    writeFileSync(
      file,
      JSON.stringify([
        {
          id: "new-agent",
          label: "New",
          driver: "acp",
          command: process.execPath,
          args: [peer],
        },
      ]),
    );
    assert.equal(
      providers(file).find((p) => p.id === "new-agent").path,
      process.execPath,
    );
    writeFileSync(file, JSON.stringify([{ id: "../../escape" }]));
    assert.throws(() => providers(file), /IDs/);
    writeFileSync(file, JSON.stringify([{ id: "new", driver: "shell" }]));
    assert.throws(() => providers(file), /configuration/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test(
  "runner starts a configured ACP agent, resolves local approval, writes summary and closes",
  { timeout: 10000 },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "pitwall-runner-"));
    const config = join(dir, "agents.json"),
      summaryPath = join(dir, "summary.txt");
    writeFileSync(
      config,
      JSON.stringify([
        {
          id: "fixture",
          label: "Fixture",
          driver: "acp",
          command: process.execPath,
          args: [peer],
        },
      ]),
    );
    const child = spawn(
      process.execPath,
      [new URL("./agent-runner.mjs", import.meta.url).pathname],
      {
        env: { ...process.env, PITWALL_AGENTS: config },
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    let output = "",
      errors = "";
    child.stdout.on("data", (s) => {
      output += s;
    });
    child.stderr.on("data", (s) => {
      errors += s;
    });
    const exited = once(child, "exit");
    child.stdin.end(
      JSON.stringify({
        provider: "fixture",
        cwd: dir,
        writableRoots: [],
        stateDir: dir,
        team: "test",
        board: "General",
        conversation: "task",
        prompt: "Initial task",
        continuation: "New feedback",
        images: [],
        resumeImages: [],
        summaryPath,
      }),
    );
    try {
      for (let attempt = 0; attempt < 100; attempt++) {
        let files = [];
        try {
          files = readdirSync(join(dir, "approvals"));
        } catch {}
        const name = files.find((n) => n.endsWith(".request.json"));
        if (name) {
          const approval = JSON.parse(
            readFileSync(join(dir, "approvals", name)),
          );
          writeFileSync(
            join(dir, "approvals", `${approval.id}.response.json`),
            JSON.stringify({ id: approval.id, choice: "native-deny" }),
          );
          break;
        }
        if (child.exitCode !== null) break;
        await delay(30);
      }
      assert.equal((await exited)[0], 0, errors + output);
      assert.equal(readFileSync(summaryPath, "utf8"), "native-deny");
      const events = output.trim().split("\n").map(JSON.parse);
      assert.equal(
        events.find((e) => e.type === "completed").session_id,
        "native-session",
      );
      assert.deepEqual(readdirSync(join(dir, "approvals")), []);
    } finally {
      child.kill("SIGKILL");
      rmSync(dir, { recursive: true, force: true });
    }
  },
);
