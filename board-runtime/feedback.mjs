import {
  MODEL_LABEL,
  modelLabel,
  modelSelection,
} from "./providers/models.mjs";
import {
  provider,
  PROVIDER_LABEL,
  defaultProvider,
  instanceKey,
} from "./providers/registry.mjs";
// Completed Board tasks are immutable. Follow-up jobs run separately, but all
// user/worker messages remain in the original conversation.
import { uploadImages } from "./images.mjs";
import { createHash } from "node:crypto";
export const RUN_CHANNEL = "sprout-runs";
export const RUN_LABEL = "sprout.latest-run";
export const CONVERSATION_LABEL = "sprout.conversation";
export const FEEDBACK_LABEL = "sprout.feedback";
export const REQUEST_LABEL = "sprout.request";
export const WORKER_RUN_LABEL = "pitwall.run";
export const WORKER_KIND_LABEL = "pitwall.kind";

export function historyMessage(m) {
  const attachments = (m.attachments ?? []).map((a) => a.id);
  return {
    id: m.id,
    token: createHash("sha256")
      .update(JSON.stringify([m.id, m.text, attachments]))
      .digest("hex"),
    text: m.text,
    author: m.author?.name || m.by?.actor || "Unknown",
    role: m.by?.actor?.endsWith("+codex-worker") ? "assistant" : "user",
    provider: m.labels?.[PROVIDER_LABEL] ?? null,
    model: modelSelection(m.labels?.[MODEL_LABEL]),
    run: m.labels?.[WORKER_RUN_LABEL] ?? null,
    kind: m.labels?.[WORKER_KIND_LABEL] ?? null,
    attachments,
  };
}

export async function effectiveTask(team, root) {
  const id = root.labels?.[RUN_LABEL];
  if (!id)
    return {
      task: root.task,
      run: null,
      provider: root.labels?.[PROVIDER_LABEL] ?? defaultProvider,
      model: modelSelection(root.labels?.[MODEL_LABEL]),
    };
  const { message: job } = await team.thread(id);
  if (
    job.labels?.[CONVERSATION_LABEL] !== root.id ||
    job.task?.owner !== root.task?.owner
  )
    throw new Error("The follow-up job does not belong to this conversation.");
  return {
    task: job.task,
    run: job.id,
    model: modelSelection(job.labels?.[MODEL_LABEL]),
    provider:
      job.labels?.[PROVIDER_LABEL] ??
      root.labels?.[PROVIDER_LABEL] ??
      defaultProvider,
  };
}

async function pointToLatest(team, rootId, replyId, jobId) {
  for (let attempt = 0; attempt < 4; attempt++) {
    const current = await team.thread(rootId);
    const latest = [...current.replies]
      .reverse()
      .find(
        (m) =>
          !m.deleted &&
          m.labels?.[FEEDBACK_LABEL] === "request" &&
          m.by?.principal === current.message.task.owner,
      );
    // An old retry must not replace the pointer to newer feedback.
    if (latest?.id !== replyId) return;
    const prior = current.message.labels?.[RUN_LABEL] ?? null;
    if (prior === jobId) return;
    try {
      await team.label(rootId, {
        set: { [RUN_LABEL]: jobId },
        if: { [RUN_LABEL]: prior },
      });
      return;
    } catch (error) {
      if (error.status !== 409) throw error;
    }
  }
  throw new Error(
    "Feedback was saved, but the task changed concurrently. Retry to finish linking it.",
  );
}

export async function sendFeedback(team, req, imageRoot) {
  const text = String(req.text ?? "").trim();
  if (!text || [...text].length > 8000)
    throw new Error("Write a reply between 1 and 8,000 characters.");
  if (!req.thread || !req.key)
    throw new Error("A conversation and submission key are required.");
  const me = await team.me();
  const original = await team.thread(req.thread);
  const root = original.message;
  if (
    !root.task ||
    root.labels?.project !== "sprout" ||
    root.task.owner !== me.principal
  )
    throw new Error(
      "Select one of your Pitwall tasks to send feedback to the worker.",
    );
  const runWorker = req.run_worker !== false;
  const existingReply = original.replies.find(
    (m) => m.labels?.[REQUEST_LABEL] === req.key,
  );
  const previous = runWorker ? await effectiveTask(team, root) : null;
  const agent = runWorker
    ? provider(
        req.provider ??
          existingReply?.labels?.[PROVIDER_LABEL] ??
          previous.provider,
      )
    : null;
  const model = runWorker
    ? modelSelection(
        "model" in req
          ? req.model
          : existingReply
            ? existingReply.labels?.[MODEL_LABEL]
            : previous.provider === agent.id
              ? previous.model
              : null,
      )
    : null;
  const assertSameSubmission = (message) => {
    if (
      message &&
      (message.text !== text ||
        message.labels?.[FEEDBACK_LABEL] !==
          (runWorker ? "request" : "comment") ||
        (runWorker &&
          (message.labels?.[PROVIDER_LABEL] ??
            root.labels?.[PROVIDER_LABEL] ??
            defaultProvider) !== agent.id) ||
        (runWorker && modelSelection(message.labels?.[MODEL_LABEL]) !== model))
    )
      throw new Error(
        "Cannot change a submitted reply, agent, or model. Use a new submission key.",
      );
  };
  assertSameSubmission(existingReply);
  const attachments = await uploadImages(team, req, imageRoot);
  const reply = await team.reply(root.id, {
    text,
    id: req.key,
    noSync: true,
    attachments,
    labels: {
      [FEEDBACK_LABEL]: runWorker ? "request" : "comment",
      [REQUEST_LABEL]: req.key,
      ...(agent
        ? { [PROVIDER_LABEL]: agent.id, [MODEL_LABEL]: modelLabel(model) }
        : {}),
    },
  });
  // The SDK may return a prior message after an uncertain response or concurrent retry.
  assertSameSubmission(reply);
  if (!runWorker) {
    const page = await team.thread(root.id);
    const effective = await effectiveTask(team, page.message);
    return { page, effective, reply_id: reply.id, run_id: null };
  }
  const { task: job } = await team.tasks.create(RUN_CHANNEL, {
    text: `Follow-up: ${[...root.text.split("\n")[0]].slice(0, 160).join("")}\n\n${text}`,
    id: `${req.key}-run`,
    for: `${me.principal}+codex-worker`,
    noSync: true,
    labels: {
      project: "sprout",
      source: "sprout-ui",
      kind: "followup",
      [PROVIDER_LABEL]: agent.id,
      [MODEL_LABEL]: modelLabel(model),
      [CONVERSATION_LABEL]: root.id,
      [FEEDBACK_LABEL]: reply.id,
    },
  });
  await pointToLatest(team, root.id, reply.id, job.id);
  const page = await team.thread(root.id);
  const effective = await effectiveTask(team, page.message);
  return { page, effective, reply_id: reply.id, run_id: job.id };
}

export async function prepareRun(team, id) {
  const taskPage = await team.thread(id);
  const task = taskPage.message;
  const rootId = task.labels?.[CONVERSATION_LABEL] ?? task.id;
  const page = rootId === task.id ? taskPage : await team.thread(rootId);
  const root = page.message;
  if (
    !task.task ||
    !root.task ||
    task.labels?.project !== "sprout" ||
    root.labels?.project !== "sprout" ||
    task.task.owner !== root.task.owner
  )
    throw new Error("Invalid task/conversation relationship.");
  const feedbackId = task.labels?.[CONVERSATION_LABEL]
    ? task.labels?.[FEEDBACK_LABEL]
    : null;
  const targetIndex = feedbackId
    ? page.replies.findIndex((m) => m.id === feedbackId)
    : -1;
  if (feedbackId && targetIndex < 0)
    throw new Error("The requested feedback message is missing.");
  const request = feedbackId ? page.replies[targetIndex] : root;
  if (request.deleted) throw new Error("The requested message was deleted.");
  if (
    feedbackId &&
    (request.by?.principal !== root.task.owner ||
      request.labels?.[FEEDBACK_LABEL] !== "request")
  )
    throw new Error("The queued feedback is not an owner request.");
  // Include later worker results, but defer newer user feedback to its own queued run.
  const relevant = page.replies.filter(
    (m, index) =>
      !m.deleted &&
      (m.labels?.[FEEDBACK_LABEL] !== "request" ||
        (!feedbackId ? false : index <= targetIndex)),
  );
  const context = relevant
    .slice(-40)
    .map((m) => `${m.author?.name || m.by.actor}:\n${m.text}`)
    .join("\n\n")
    .slice(-40000);
  const images = [];
  const seen = new Set();
  for (const m of [root, ...relevant])
    for (const image of m.attachments ?? []) {
      if (!seen.has(image.id)) {
        seen.add(image.id);
        images.push(image);
      }
    }
  if (images.length > 10)
    throw new Error(
      "This conversation has more than 10 reference images. Please start a new task with the relevant images.",
    );
  const instance = provider(
    task.labels?.[PROVIDER_LABEL] ??
      root.labels?.[PROVIDER_LABEL] ??
      defaultProvider,
  );
  if (
    feedbackId &&
    request.labels?.[PROVIDER_LABEL] &&
    request.labels[PROVIDER_LABEL] !== instance.id
  )
    throw new Error("The queued agent does not match the submitted feedback.");
  const model = modelSelection(task.labels?.[MODEL_LABEL]);
  if (feedbackId && modelSelection(request.labels?.[MODEL_LABEL]) !== model)
    throw new Error("The queued model does not match the submitted feedback.");
  return {
    model,
    provider: instance.id,
    provider_key: instanceKey(instance),
    driver: instance.driver,
    new_image_ids: (request.attachments ?? []).map((a) => a.id),
    conversation: root.id,
    root_text: root.text,
    request_text: request.text,
    follow_up: !!feedbackId,
    task: task.id,
    request: historyMessage(request),
    history: [root, ...relevant]
      .filter((m) => m.id !== request.id)
      .map(historyMessage),
    context,
    images,
  };
}
