// Protocol peer for deterministic lifecycle tests, using the official ACP SDK.
import {
  agent,
  ndJsonStream,
  PROTOCOL_VERSION,
} from "@agentclientprotocol/sdk";
import { Readable, Writable } from "node:stream";
import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
const mode = process.env.PITWALL_TEST_MODE;
if (mode === "exit") process.exit(7);
if (mode === "codex") {
  let model = process.env.PITWALL_TEST_MODEL || "test-a";
  const send = (e) => process.stdout.write(JSON.stringify(e) + "\n");
  createInterface({ input: process.stdin }).on("line", (line) => {
    const e = JSON.parse(line),
      result = (r) => send({ id: e.id, result: r });
    if (e.method === "initialize") result({});
    if (e.method === "model/list")
      result({
        data: [
          {
            model: "test-a",
            displayName: "Model A",
            isDefault: true,
            inputModalities: ["text", "image"],
          },
          {
            model: "test-b",
            displayName: "Model B",
            isDefault: false,
            inputModalities: ["text"],
          },
        ],
        nextCursor: null,
      });
    if (e.method === "thread/start" || e.method === "thread/resume") {
      model = e.params.model || model;
      result({ thread: { id: e.params.threadId || "native-thread" }, model });
    }
    if (e.method === "turn/start") {
      result({ turn: { id: "turn" } });
      send({
        method: "turn/started",
        params: { threadId: "native-thread", turn: { id: "turn" } },
      });
      send({
        id: "approval",
        method: "item/commandExecution/requestApproval",
        params: {
          command: "echo hello",
          threadId: "native-thread",
          turnId: "turn",
        },
      });
    }
    if (e.id === "approval" && e.result) {
      send({
        method: "item/completed",
        params: {
          threadId: "native-thread",
          item: { type: "agentMessage", text: e.result.decision },
        },
      });
      send({
        method: "turn/completed",
        params: {
          threadId: "native-thread",
          turn: { id: "turn", status: "completed" },
        },
      });
    }
  });
} else {
  let cancelled,
    authenticated = mode !== "auth",
    model = process.env.PITWALL_TEST_MODEL || "test-a";
  const modelState = () => ({
    models: {
      currentModelId: model,
      availableModels: [
        { modelId: "test-a", name: "Model A" },
        { modelId: "test-b", name: "Model B" },
      ],
    },
    ...(mode?.startsWith("models")
      ? {
          configOptions: [
            {
              id: "model",
              category: "model",
              name: "Model",
              type: "select",
              currentValue: model,
              options: [
                { value: "test-a", name: "Model A" },
                { value: "test-b", name: "Model B" },
              ],
            },
          ],
        }
      : {}),
  });
  agent()
    .onRequest("initialize", () => ({
      protocolVersion: PROTOCOL_VERSION,
      agentCapabilities: {
        loadSession: mode !== "no-resume",
        promptCapabilities: { image: mode !== "no-images" },
      },
      authMethods: [],
    }))
    .onRequest("session/new", () => {
      if (!authenticated) throw new Error("Authentication required");
      return { sessionId: "native-session", ...modelState() };
    })
    .onRequest("session/load", async ({ params: p, client: context }) => {
      await context.notify("session/update", {
        sessionId: p.sessionId,
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "old history" },
        },
      });
      return modelState();
    })
    .onRequest("session/set_config_option", ({ params }) => {
      if (mode !== "models-refuse") model = params.value;
      return { configOptions: modelState().configOptions };
    })
    .onRequest(
      "session/set_model",
      (p) => p,
      ({ params }) => {
        model = params.modelId;
        return {};
      },
    )
    .onNotification("session/cancel", () => cancelled?.())
    .onRequest("session/prompt", async ({ params: p, client: context }) => {
      if (mode === "grok-streams") {
        for (const event of JSON.parse(
          readFileSync(new URL("./grok-streams.json", import.meta.url), "utf8"),
        )) {
          await context.notify("session/update", event);
        }
        return { stopReason: "end_turn" };
      }
      if (mode === "cancel") {
        await new Promise((resolve) => {
          cancelled = resolve;
        });
        return { stopReason: "cancelled" };
      }
      const permission = await context.request("session/request_permission", {
        sessionId: p.sessionId,
        toolCall: {
          toolCallId: "tool",
          title: "Write test file",
          status: "pending",
        },
        options: [
          { optionId: "native-once", kind: "allow_once", name: "Allow once" },
          { optionId: "native-always", kind: "allow_always", name: "Always" },
          { optionId: "native-deny", kind: "reject_once", name: "Deny" },
        ],
      });
      const text =
        permission.outcome.outcome === "selected"
          ? permission.outcome.optionId
          : "cancelled";
      await context.notify("session/update", {
        sessionId: p.sessionId,
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text },
        },
      });
      return { stopReason: "end_turn" };
    })
    .connect(
      ndJsonStream(
        Writable.toWeb(process.stdout),
        Readable.toWeb(process.stdin),
      ),
    );
}
