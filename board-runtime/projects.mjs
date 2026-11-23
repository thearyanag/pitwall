// Only an explicit local folder-picker action registers a project. Board stores
// the opaque ID, never a command, executable, or authoritative filesystem path.
import { createHash } from "node:crypto";
import {
  mkdirSync,
  readFileSync,
  realpathSync,
  statSync,
  writeFileSync,
  readdirSync,
  existsSync,
} from "node:fs";
import { basename, isAbsolute, join } from "node:path";
import { stateDirectory } from "./providers/registry.mjs";

export const PROJECT_LABEL = "pitwall.project";
export function knownProject(id, state = stateDirectory()) {
  return (
    typeof id === "string" &&
    /^[a-f0-9]{64}$/.test(id) &&
    existsSync(join(state, "projects", `${id}.json`))
  );
}
function identity(path) {
  const canonical = realpathSync(path),
    stat = statSync(canonical, { bigint: true });
  if (!stat.isDirectory())
    throw new Error("Choose an existing project folder.");
  const device = stat.dev.toString(),
    inode = stat.ino.toString();
  const id = createHash("sha256")
    .update(JSON.stringify([canonical, device, inode]))
    .digest("hex");
  return {
    id,
    path: canonical,
    name: basename(canonical) || canonical,
    device,
    inode,
  };
}
export function registerProject(path, state = stateDirectory()) {
  if (
    typeof path !== "string" ||
    !isAbsolute(path) ||
    /[\x00-\x1f\x7f]/.test(path)
  )
    throw new Error("Choose an absolute project folder path.");
  const project = identity(path);
  const directory = join(state, "projects");
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  try {
    writeFileSync(
      join(directory, `${project.id}.json`),
      JSON.stringify(project),
      { flag: "wx", mode: 0o600 },
    );
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
  }
  return resolveProject(project.id, state);
}
export function resolveProject(id, state = stateDirectory()) {
  if (typeof id !== "string" || !/^[a-f0-9]{64}$/.test(id))
    throw new Error(
      "Open a project folder before running an agent. Older projectless conversations need a new task.",
    );
  let saved;
  try {
    saved = JSON.parse(
      readFileSync(join(state, "projects", `${id}.json`), "utf8"),
    );
  } catch {
    throw new Error(
      "This project has not been opened on this computer. Open the folder and start a new task.",
    );
  }
  let current;
  try {
    current = identity(saved.path);
  } catch {
    throw new Error(
      "The project folder is no longer available. Open it again before running an agent.",
    );
  }
  if (current.id !== id || saved.id !== id)
    throw new Error(
      "The project folder changed or was replaced. Open it again and start a new task.",
    );
  return current;
}
export function listProjects(state = stateDirectory()) {
  let names;
  try {
    names = readdirSync(join(state, "projects"));
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  return names
    .filter((name) => /^[a-f0-9]{64}\.json$/.test(name))
    .flatMap((name) => {
      try {
        return [resolveProject(name.slice(0, -5), state)];
      } catch {
        return [];
      }
    });
}
