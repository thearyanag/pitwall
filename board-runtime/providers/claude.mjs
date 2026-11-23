import { readFileSync } from "node:fs";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { choices } from "./models.mjs";

/**
 * @param {import("./contract.js").AgentInstance} instance
 * @param {import("./contract.js").SessionOptions} options
 * @param {import("./contract.js").AgentHost} host
 * @returns {Promise<import("./contract.js").AgentSession>}
 */
export async function open(instance, options, host, queryFn = query) {
  if (!instance.path)
    throw new Error(
      "Claude Code is not installed. Install it and run claude auth login.",
    );
  const controller = new AbortController();
  const inputReady = Promise.withResolvers();
  let running;
  const onAbort = () => {
    controller.abort();
    inputReady.resolve(null);
    running?.close();
  };
  host.signal.addEventListener("abort", onAbort, { once: true });
  const selected = options.model ?? instance.model ?? null;
  const session = {
    id: options.resumeId || null,
    resumed: !!options.resumeId,
    capabilities: { images: true, resume: true },
    models: [],
    model: selected,
    defaultModel: null,
    async prompt({ text, images }) {
      await initialize();
      const content = [
        { type: "text", text },
        ...images.map((i) => ({
          type: "image",
          source: {
            type: "base64",
            media_type: i.type || "image/png",
            data: readFileSync(i.path).toString("base64"),
          },
        })),
      ];
      inputReady.resolve({
        type: "user",
        message: { role: "user", content },
        parent_tool_use_id: null,
        session_id: session.id || "",
      });
      let result;
      for await (const event of running) {
        if (event.session_id) session.id = event.session_id;
        if (event.type === "system" && event.subtype === "init") {
          session.model = event.model;
          host.emit({ type: "model", model: session.model });
        }
        if (event.type === "assistant") {
          if (event.message.model) {
            session.model = event.message.model;
            host.emit({ type: "model", model: session.model });
          }
          for (const block of event.message.content) {
            if (block.type === "text")
              host.emit({ type: "message", text: block.text });
            if (block.type === "tool_use")
              host.emit({ type: "tool", title: block.name });
          }
        }
        if (
          event.type === "stream_event" &&
          event.event.type === "content_block_delta" &&
          event.event.delta.type === "text_delta"
        )
          host.emit({ type: "text_delta", text: event.event.delta.text });
        if (event.type === "result") {
          if (event.is_error || event.subtype !== "success")
            throw new Error(
              event.errors?.join("; ") ||
                event.result ||
                `Claude: ${event.subtype}`,
            );
          result = event.result;
        }
      }
      if (result === undefined)
        throw new Error("Claude stopped without a result.");
      return result;
    },
    async cancel() {
      controller.abort();
      inputReady.resolve(null);
    },
    async close() {
      host.signal.removeEventListener("abort", onAbort);
      inputReady.resolve(null);
      running?.close();
    },
  };
  async function initialize() {
    if (running) return;
    /** @returns {AsyncGenerator<import("@anthropic-ai/claude-agent-sdk").SDKUserMessage>} */
    async function* input() {
      const message = await inputReady.promise;
      if (message) yield message;
    }
    running = queryFn({
      prompt: input(),
      options: {
        cwd: options.cwd,
        pathToClaudeCodeExecutable: instance.path,
        abortController: controller,
        env: { ...process.env, ...instance.env },
        ...(session.id ? { resume: session.id } : {}),
        // Match the native coding CLI: project instructions and settings load
        // through the SDK; its permission engine decides which calls need us.
        systemPrompt: { type: "preset", preset: "claude_code" },
        settingSources: ["user", "project", "local"],
        includePartialMessages: true,
        stderr: (data) => process.stderr.write(data),
        canUseTool: async (name, toolInput) => {
          const choice = await host.ask(
            name,
            JSON.stringify(toolInput, null, 2),
            [
              { id: "allow", label: "Allow once" },
              { id: "deny", label: "Deny" },
            ],
          );
          return choice === "allow"
            ? { behavior: "allow", updatedInput: toolInput }
            : { behavior: "deny", message: "Not approved in Pitwall." };
        },
      },
    });
    try {
      const models = await running.supportedModels();
      session.models = choices(
        models.map((m) => ({
          id: m.value,
          label: m.displayName,
          description: m.description,
        })),
      );
      if (!options.discover) {
        if (
          selected &&
          !models.some(
            (m) => m.value === selected || m.resolvedModel === selected,
          )
        )
          throw new Error(
            `Claude no longer offers model ${selected}. Refresh the model picker.`,
          );
        // Explicitly reset the native default; omission on resume can retain the last model.
        await running.setModel(selected ?? undefined);
      }
    } catch (error) {
      inputReady.resolve(null);
      running.close();
      throw error;
    }
  }
  try {
    await initialize();
  } catch (error) {
    await session.close();
    throw error;
  }
  return session;
}
