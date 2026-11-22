import {
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";

export function atomicJson(path, value) {
  const tmp = `${path}.${process.pid}.tmp`;
  writeFileSync(tmp, JSON.stringify(value), { mode: 0o600 });
  renameSync(tmp, path);
}
export function createHost(request, signal, emit) {
  const directory = join(request.stateDir, "approvals");
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  // Serialize prompts so parallel tool calls cannot replace the visible question.
  let tail = Promise.resolve();
  return {
    signal,
    emit,
    ask(title, detail, options) {
      const ask = async () => {
        if (signal.aborted) return null;
        const id = randomUUID();
        const path = join(directory, `${id}.request.json`),
          response = join(directory, `${id}.response.json`);
        const expires = Date.now() + 5 * 60_000;
        atomicJson(path, {
          id,
          title,
          detail,
          options,
          team: request.team,
          board: request.board,
          task: request.conversation,
          provider: request.provider,
          expires,
        });
        emit({
          type: "approval",
          text: `${request.label} needs permission: ${title}. Open Pitwall to allow or deny.`,
        });
        try {
          while (!signal.aborted && Date.now() < expires) {
            try {
              const answer = JSON.parse(readFileSync(response, "utf8"));
              return answer.id === id &&
                options.some((o) => o.id === answer.choice)
                ? answer.choice
                : null;
            } catch (error) {
              if (error.code !== "ENOENT") return null;
            }
            await delay(200, undefined, { signal }).catch(() => {});
          }
          return null;
        } finally {
          rmSync(path, { force: true });
          rmSync(response, { force: true });
        }
      };
      const pending = tail.then(ask);
      tail = pending.catch(() => {});
      return pending;
    },
  };
}
