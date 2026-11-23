// Explicit model-picker action only. No Board access, prompts, or token copying.
import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { resolveProject } from "./projects.mjs";
import { openAgent } from "./providers/index.mjs";

const abort = new AbortController();
const timer = setTimeout(() => abort.abort(), 18000);
process.once("SIGTERM", () => abort.abort());
process.once("SIGINT", () => abort.abort());
let session, scratch;
try {
  const request = JSON.parse(readFileSync(0, "utf8"));
  const cwd = request.project
    ? resolveProject(request.project).path
    : (scratch = mkdtempSync(join(tmpdir(), "pitwall-models-")));
  session = await openAgent(
    request.provider,
    { cwd, discover: true },
    {
      signal: abort.signal,
      emit() {},
      async ask() {
        return null;
      },
    },
  );
  console.log(
    JSON.stringify({
      ok: true,
      data: {
        provider: request.provider,
        models: session.models,
        current_model: session.model,
        default_model: session.defaultModel,
      },
    }),
  );
} catch (error) {
  console.log(
    JSON.stringify({
      ok: false,
      error: abort.signal.aborted
        ? "Model discovery timed out. Check the agent's login and retry."
        : String(error.message)
            .replace(/[\x00-\x1f\x7f]/g, " ")
            .slice(0, 600),
    }),
  );
  process.exitCode = 1;
} finally {
  clearTimeout(timer);
  await session?.close();
  if (scratch) rmSync(scratch, { recursive: true, force: true });
}
