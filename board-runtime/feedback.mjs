// Completed Board tasks are immutable. Follow-up jobs run separately, but all
// user/worker messages remain in the original conversation.
import { uploadImages } from "./images.mjs";
export const RUN_CHANNEL = "sprout-runs";
export const RUN_LABEL = "sprout.latest-run";
export const CONVERSATION_LABEL = "sprout.conversation";
export const FEEDBACK_LABEL = "sprout.feedback";
export const REQUEST_LABEL = "sprout.request";

export async function effectiveTask(team, root) {
  const id = root.labels?.[RUN_LABEL];
  if (!id) return { task: root.task, run: null };
  const { message: job } = await team.thread(id);
  if (
    job.labels?.[CONVERSATION_LABEL] !== root.id ||
    job.task?.owner !== root.task?.owner
  )
    throw new Error("The follow-up job does not belong to this conversation.");
  return { task: job.task, run: job.id };
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
  const { message: root } = await team.thread(req.thread);
  if (
    !root.task ||
    root.labels?.project !== "sprout" ||
    root.task.owner !== me.principal
  )
    throw new Error(
      "Select one of your Pitwall tasks to send feedback to the worker.",
    );
  const attachments = await uploadImages(team, req, imageRoot);
  const runWorker = req.run_worker !== false;
  const reply = await team.reply(root.id, {
    text,
    id: req.key,
    noSync: true,
    attachments,
    labels: {
      [FEEDBACK_LABEL]: runWorker ? "request" : "comment",
      [REQUEST_LABEL]: req.key,
    },
  });
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
  return {
    conversation: root.id,
    root_text: root.text,
    request_text: request.text,
    follow_up: !!feedbackId,
    context,
    images,
  };
}
