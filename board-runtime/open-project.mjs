import { readFileSync } from "node:fs";
import { registerProject } from "./projects.mjs";
try {
  const request = JSON.parse(readFileSync(0, "utf8"));
  console.log(
    JSON.stringify({ ok: true, data: registerProject(request.path) }),
  );
} catch (error) {
  console.log(
    JSON.stringify({
      ok: false,
      error: String(error.message).replace(/[\x00-\x1f\x7f]/g, " "),
    }),
  );
  process.exitCode = 1;
}
