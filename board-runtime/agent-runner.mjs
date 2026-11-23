import { resolveProject } from "./projects.mjs";
// One claimed Board job. The Python coordinator owns project context and reporting.
import { realpathSync, readFileSync, writeFileSync } from "node:fs";
import { openAgent } from "./providers/index.mjs";
import { catalog, provider } from "./providers/registry.mjs";
import { createHost } from "./providers/host.mjs";

if (process.argv[2] === "list") {
  console.log(JSON.stringify(catalog(), null, 2));
} else {
  const emit = (event) => process.stdout.write(JSON.stringify(event) + "\n");
  const abort = new AbortController();
  let session, instance;
  let stage = "preflight";
  const cancel = () => {
    abort.abort();
    session?.cancel().catch(() => {});
    // A broken provider must not hold the Board claim indefinitely on shutdown.
    setTimeout(() => process.exit(130), 2500).unref();
  };
  process.once("SIGTERM", cancel);
  process.once("SIGINT", cancel);
  const timeout = setTimeout(cancel, 20 * 60_000);
  try {
    const request = JSON.parse(readFileSync(0, "utf8"));
    const project = resolveProject(request.projectId);
    if (realpathSync(request.cwd) !== project.path)
      throw new Error("Run workspace does not match the selected project.");
    instance = provider(request.provider);
    const host = createHost(
      { ...request, label: instance.label },
      abort.signal,
      emit,
    );
    session = await openAgent(request.provider, request, host);
    emit({
      type: "session",
      id: session.id,
      resumed: session.resumed,
      capabilities: session.capabilities,
      model: session.model,
      default_model: session.defaultModel,
    });
    const text = session.resumed ? request.continuation : request.prompt;
    const modelChanged =
      session.resumed && session.model !== request.previousModel;
    const images =
      session.resumed && !modelChanged ? request.resumeImages : request.images;
    const plan = request.contextPlans?.[session.resumed ? "resumed" : "fresh"];
    const receipt = plan
      ? {
          ...plan,
          image_ids_sent: images.map((i) => i.id),
          model: session.model,
          model_changed: modelChanged,
        }
      : null;
    if (receipt) {
      writeFileSync(request.auditPath, JSON.stringify(receipt, null, 2), {
        mode: 0o600,
      });
      emit({ type: "context", receipt });
    }
    if (images.length && !session.capabilities.images)
      throw new Error(`${instance.label} does not support image input.`);
    stage = "turn";
    emit({ type: "prompt_started" });
    const summary = await session.prompt({ text, images });
    if (abort.signal.aborted) throw new Error("Agent run interrupted.");
    if (!summary?.trim())
      throw new Error(`${instance.label} returned no final answer.`);
    writeFileSync(request.summaryPath, summary, { mode: 0o600 });
    emit({
      type: "completed",
      session_id: session.id,
      resumable: session.capabilities.resume,
      model: session.model,
      default_model: session.defaultModel,
    });
  } catch (error) {
    const auth = /authenticat|unauthorized|401|token.*expir/i.test(
      error.message,
    );
    emit({
      type: "error",
      stage,
      message:
        error.message +
        (auth && instance
          ? ` Sign in with ${instance.login || "the agent native login"}, then retry.`
          : ""),
    });
    process.exitCode = abort.signal.aborted ? 130 : 1;
  } finally {
    clearTimeout(timeout);
    abort.abort();
    await session?.close();
  }
}
