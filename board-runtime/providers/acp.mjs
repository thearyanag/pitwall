import {
  client,
  ndJsonStream,
  PROTOCOL_VERSION,
} from "@agentclientprotocol/sdk";
import { Readable, Writable } from "node:stream";
import { readFileSync } from "node:fs";
import { launch, stop } from "./process.mjs";
import { acpModelState } from "./models.mjs";

// All ACP agents use this driver. Agent IDs do not affect protocol behavior.
/**
 * @param {import("./contract.js").AgentInstance} instance
 * @param {import("./contract.js").SessionOptions} options
 * @param {import("./contract.js").AgentHost} host
 * @returns {Promise<import("./contract.js").AgentSession>}
 */
export async function open(
  instance,
  options,
  host,
  flavor = {
    streamKey: (_notification) => undefined,
    isReplay: (_notification) => false,
  },
) {
  const child = launch(instance, [], options.cwd);
  let text = "",
    segment = "",
    sessionId,
    replaying = false,
    streamKey;
  let setup = {};
  const flush = () => {
    if (segment.trim()) host.emit({ type: "message", text: segment });
    segment = "";
  };
  const connection = client()
    .onRequest("session/request_permission", async ({ params: p }) => {
      const choices = p.options.filter(
        (o) => o.kind === "allow_once" || o.kind.startsWith("reject"),
      );
      const choice = await host.ask(
        p.toolCall.title || "Tool permission",
        JSON.stringify(p.toolCall, null, 2),
        choices.map((o) => ({ id: o.optionId, label: o.name })),
      );
      return {
        outcome: choice
          ? { outcome: "selected", optionId: choice }
          : { outcome: "cancelled" },
      };
    })
    .onNotification("session/update", ({ params: notification }) => {
      const { sessionId: id, update } = notification;
      if (
        update.sessionUpdate === "config_option_update" &&
        (!sessionId || id === sessionId)
      )
        setup = { ...setup, configOptions: update.configOptions };
      if (
        replaying ||
        flavor.isReplay(notification) ||
        (sessionId && id !== sessionId)
      )
        return;
      if (
        update.sessionUpdate === "agent_message_chunk" &&
        update.content.type === "text"
      ) {
        const nextKey = flavor.streamKey(notification);
        if (nextKey !== undefined && nextKey !== streamKey) {
          flush();
          text = "";
          streamKey = nextKey;
        }
        text += update.content.text;
        segment += update.content.text;
        host.emit({ type: "text_delta", text: update.content.text });
      }
      if (update.sessionUpdate === "tool_call") {
        flush();
        host.emit({ type: "tool", title: update.title });
      }
    })
    .connect(
      ndJsonStream(Writable.toWeb(child.stdin), Readable.toWeb(child.stdout)),
    );
  child.on("error", (error) => connection.close(error));
  child.on("exit", (code, signal) =>
    connection.close(
      new Error(`${instance.label} stopped (${signal || code}).`),
    ),
  );
  let capabilities,
    resumed = false;
  try {
    const init = await connection.agent.request(
      "initialize",
      {
        protocolVersion: PROTOCOL_VERSION,
        clientInfo: { name: "pitwall", version: "0.3.0" },
        // Agents execute their own tools. Do not advertise unimplemented fs/terminal callbacks.
        clientCapabilities: {},
      },
      { cancellationSignal: host.signal, stopWaitingOnCancel: true },
    );
    capabilities = {
      images: init.agentCapabilities?.promptCapabilities?.image === true,
      resume: init.agentCapabilities?.loadSession === true,
    };
    if (options.resumeId && capabilities.resume) {
      replaying = true;
      setup = await connection.agent.request(
        "session/load",
        { sessionId: options.resumeId, cwd: options.cwd, mcpServers: [] },
        { cancellationSignal: host.signal, stopWaitingOnCancel: true },
      );
      sessionId = options.resumeId;
      resumed = true;
      replaying = false;
    } else {
      const created = await connection.agent.request(
        "session/new",
        { cwd: options.cwd, mcpServers: [] },
        { cancellationSignal: host.signal, stopWaitingOnCancel: true },
      );
      sessionId = created.sessionId;
      setup = created;
    }
  } catch (error) {
    connection.close();
    await stop(child);
    throw error;
  }
  let state = acpModelState(setup);
  const defaultModel = options.defaultModel ?? instance.model ?? state.current;
  const selected =
    options.model === undefined ? (instance.model ?? null) : options.model;
  const target = selected ?? defaultModel;
  try {
    if (!options.discover && target && target !== state.current) {
      if (!state.models.some((m) => m.id === target))
        throw new Error(
          `${instance.label} no longer offers model ${target}. Refresh the model picker.`,
        );
      if (state.configId) {
        const updated =
          /** @type {import("@agentclientprotocol/sdk").SetSessionConfigOptionResponse} */ (
            await connection.agent.request(
              "session/set_config_option",
              { sessionId, configId: state.configId, value: target },
              { cancellationSignal: host.signal, stopWaitingOnCancel: true },
            )
          );
        setup = { ...setup, ...updated };
        state = acpModelState(setup);
        if (state.current !== target)
          throw new Error(`${instance.label} did not confirm model ${target}.`);
      } else {
        await connection.agent.request(
          "session/set_model",
          { sessionId, modelId: target },
          { cancellationSignal: host.signal, stopWaitingOnCancel: true },
        );
        state.current = target;
      }
    }
  } catch (error) {
    connection.close();
    await stop(child);
    throw error;
  }
  return {
    id: sessionId,
    resumed,
    capabilities,
    models: state.models,
    model: state.current,
    defaultModel,
    async prompt(input) {
      if (input.images.length && !capabilities.images)
        throw new Error(
          `${instance.label} does not advertise image input. Remove attachments or choose another agent.`,
        );
      text = "";
      segment = "";
      streamKey = undefined;
      /** @type {import("@agentclientprotocol/sdk").ContentBlock[]} */
      const prompt = [{ type: "text", text: input.text }];
      for (const image of input.images)
        prompt.push({
          type: "image",
          mimeType: image.type || "image/png",
          data: readFileSync(image.path).toString("base64"),
        });
      const result =
        /** @type {import("@agentclientprotocol/sdk").PromptResponse} */ (
          await connection.agent.request(
            "session/prompt",
            { sessionId, prompt },
            { cancellationSignal: host.signal, stopWaitingOnCancel: true },
          )
        );
      if (result.stopReason !== "end_turn")
        throw new Error(`${instance.label} stopped: ${result.stopReason}`);
      flush();
      return text;
    },
    async cancel() {
      await connection.agent.notify("session/cancel", { sessionId });
    },
    async close() {
      connection.close();
      await stop(child);
    },
  };
}
