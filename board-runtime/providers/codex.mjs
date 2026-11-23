import { createInterface } from "node:readline";
import { launch, stop } from "./process.mjs";
import { choices } from "./models.mjs";

/** Codex uses its own JSONL RPC (not ACP or Content-Length framing). */
/**
 * @param {import("./contract.js").AgentInstance} instance
 * @param {import("./contract.js").SessionOptions} options
 * @param {import("./contract.js").AgentHost} host
 * @returns {Promise<import("./contract.js").AgentSession>}
 */
export async function open(instance, options, host) {
  const child = launch(instance, ["app-server"], options.cwd);
  const pending = new Map();
  let nextId = 0,
    sessionId,
    activeTurn,
    completion,
    finalText = "",
    failure;
  let models = [],
    model = null,
    defaultModel = null;
  const write = (message) => child.stdin.write(JSON.stringify(message) + "\n");
  const fail = (error) => {
    failure = error;
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      p.reject(error);
    }
    pending.clear();
    completion?.reject(error);
  };
  const onAbort = () => fail(new Error("Codex connection cancelled."));
  host.signal.addEventListener("abort", onAbort, { once: true });
  child.on("error", fail);
  child.on("exit", (code, signal) =>
    fail(new Error(`Codex stopped (${signal || code}).`)),
  );
  const call = (method, params) =>
    new Promise((resolve, reject) => {
      if (failure) return reject(failure);
      const id = ++nextId;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`Codex ${method} timed out.`));
      }, 60000);
      pending.set(id, { resolve, reject, timer });
      write({ id, method, params });
    });
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      fail(new Error("Codex returned invalid JSON."));
      return;
    }
    if (event.id !== undefined && !event.method) {
      const p = pending.get(event.id);
      if (!p) return;
      pending.delete(event.id);
      clearTimeout(p.timer);
      if (event.error) p.reject(new Error(event.error.message));
      else p.resolve(event.result);
      return;
    }
    const p = event.params || {};
    if (event.id !== undefined) {
      const supported = [
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
      ].includes(event.method);
      if (!supported) {
        write({
          id: event.id,
          error: {
            code: -32601,
            message: "This interaction is not supported by Pitwall.",
          },
        });
        return;
      }
      host
        .ask(p.command || "Approve file changes", JSON.stringify(p, null, 2), [
          { id: "accept", label: "Allow once" },
          { id: "decline", label: "Deny" },
        ])
        .then(
          (decision) =>
            write({
              id: event.id,
              result: { decision: decision || "decline" },
            }),
          () => write({ id: event.id, result: { decision: "decline" } }),
        );
      return;
    }
    if (p.threadId && p.threadId !== sessionId) return;
    if (event.method === "item/agentMessage/delta")
      host.emit({ type: "text_delta", text: p.delta });
    if (event.method === "item/completed" && p.item?.type === "agentMessage") {
      finalText = p.item.text;
      host.emit({ type: "message", text: finalText });
    }
    if (event.method === "item/started" && p.item?.type !== "agentMessage")
      host.emit({
        type: "tool",
        title: p.item?.command || p.item?.type || "Working",
      });
    if (event.method === "turn/started") activeTurn = p.turn.id;
    if (event.method === "turn/completed") {
      if (p.turn.status === "completed") completion?.resolve(finalText);
      else
        completion?.reject(
          new Error(p.turn.error?.message || `Codex turn ${p.turn.status}`),
        );
    }
  });
  try {
    await call("initialize", {
      clientInfo: { name: "pitwall", version: "0.4.0" },
    });
    write({ method: "initialized" });
    const { config: projectConfig } = await call("config/read", {
      cwd: options.cwd,
      includeLayers: false,
    });
    const entries = [],
      cursors = new Set();
    let cursor;
    do {
      const page = await call("model/list", {
        limit: 100,
        ...(projectConfig.model_provider ? { modelProvider: projectConfig.model_provider } : {}),
        ...(cursor ? { cursor } : {}),
      });
      entries.push(...page.data);
      cursor = page.nextCursor;
      if (cursor && cursors.has(cursor))
        throw new Error("Codex returned a repeated model cursor.");
      cursors.add(cursor);
    } while (cursor);
    models = choices(
      entries.map((m) => ({
        id: m.model,
        label: m.displayName,
        description: m.description,
        ...(Array.isArray(m.inputModalities)
          ? { images: m.inputModalities.includes("image") }
          : {}),
      })),
    );
    defaultModel =
      instance.model ??
      projectConfig.model ??
      entries.find((m) => m.isDefault)?.model ??
      options.defaultModel ??
      null;
    model = options.model ?? defaultModel;
    if (
      model &&
      !models.some((m) => m.id === model) &&
      model !== instance.model
    )
      throw new Error(
        `Codex no longer offers model ${model}. Refresh the model picker.`,
      );
    // Resolve native configuration for this project. Do not replace its sandbox,
    // approval policy, network settings or configured MCP servers.
    const params = {
      cwd: options.cwd,
      ...(model ? { model } : {}),
    };
    if (!options.discover) {
      const response = await call(
        options.resumeId ? "thread/resume" : "thread/start",
        options.resumeId ? { ...params, threadId: options.resumeId } : params,
      );
      sessionId = response.thread.id;
      model = response.model ?? model;
      defaultModel ??= model;
    }
  } catch (error) {
    host.signal.removeEventListener("abort", onAbort);
    lines.close();
    await stop(child);
    throw error;
  }
  return {
    id: sessionId ?? null,
    resumed: !!options.resumeId,
    capabilities: {
      images: models.find((m) => m.id === model)?.images !== false,
      resume: true,
    },
    models,
    model,
    defaultModel,
    async prompt({ text, images }) {
      if (!sessionId)
        throw new Error("Model discovery does not create a Codex thread.");
      finalText = "";
      completion = Promise.withResolvers();
      // Attach a rejection handler before turn/start can race with process exit.
      completion.promise.catch(() => {});
      await call("turn/start", {
        threadId: sessionId,
        ...(model ? { model } : {}),
        input: [
          { type: "text", text, text_elements: [] },
          ...images.map((i) => ({ type: "localImage", path: i.path })),
        ],
      });
      return await completion.promise;
    },
    async cancel() {
      if (activeTurn && !failure)
        await call("turn/interrupt", {
          threadId: sessionId,
          turnId: activeTurn,
        });
    },
    async close() {
      host.signal.removeEventListener("abort", onAbort);
      lines.close();
      await stop(child);
    },
  };
}
