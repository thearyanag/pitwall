import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
// Local configuration owns launch commands. Board only carries an instance ID.
import { accessSync, constants, readFileSync } from "node:fs";
import { delimiter, isAbsolute, join } from "node:path";
import { homedir } from "node:os";

export const PROVIDER_LABEL = "pitwall.agent";
export const defaultProvider = "codex";
export function stateDirectory() {
  return (
    process.env.SPROUT_STATE_DIR ||
    fileURLToPath(new URL("../../worker/", import.meta.url))
  );
}
export function executable(command, env = process.env) {
  const expanded = command.startsWith("~/")
    ? join(homedir(), command.slice(2))
    : command;
  const candidates = isAbsolute(expanded)
    ? [expanded]
    : (env.PATH || "").split(delimiter).map((dir) => join(dir, expanded));
  return (
    candidates.find((path) => {
      try {
        accessSync(path, constants.X_OK);
        return true;
      } catch {
        return false;
      }
    }) || null
  );
}
export function providers(
  configPath = process.env.PITWALL_AGENTS ||
    join(stateDirectory(), "agents.json"),
) {
  const builtins = [
    {
      id: "codex",
      label: "Codex",
      driver: "codex",
      command: process.env.SPROUT_CODEX || "codex",
      args: [],
      login: "codex login",
    },
    {
      id: "claude",
      label: "Claude Code",
      driver: "claude",
      command: "claude",
      args: [],
      login: "claude auth login",
    },
    {
      id: "grok",
      label: "Grok Build",
      driver: "grok",
      command: "grok",
      args: ["--no-auto-update", "agent", "--no-leader", "stdio"],
      login: "grok",
    },
    {
      id: "antigravity",
      label: "Antigravity",
      driver: "acp",
      command: "agy",
      args: ["acp"],
      login: "agy",
    },
  ];
  let custom = [];
  try {
    custom = JSON.parse(readFileSync(configPath, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT")
      throw new Error(`Cannot read agent settings: ${error.message}`);
  }
  if (!Array.isArray(custom))
    throw new Error("agents.json must contain an array of agent instances.");
  const all = new Map(builtins.map((p) => [p.id, p]));
  const seen = new Set();
  for (const entry of custom) {
    if (
      !entry ||
      !/^[a-z][a-z0-9-]{0,63}$/.test(entry.id) ||
      seen.has(entry.id)
    )
      throw new Error("Agent IDs must be unique lowercase names.");
    seen.add(entry.id);
    all.set(entry.id, { ...all.get(entry.id), ...entry });
  }
  return [...all.values()].map((p) => {
    if (
      !["codex", "claude", "acp", "grok"].includes(p.driver) ||
      typeof p.command !== "string" ||
      !p.command ||
      !Array.isArray(p.args) ||
      p.args.some((s) => typeof s !== "string") ||
      typeof p.label !== "string" ||
      /[\x00-\x1f\x7f]/.test(p.label) ||
      (p.env &&
        (typeof p.env !== "object" ||
          Array.isArray(p.env) ||
          Object.values(p.env).some((v) => typeof v !== "string")))
    )
      throw new Error(`Invalid agent configuration: ${p.id}`);
    if (p.driver === "claude" && p.args.length)
      throw new Error(
        "Claude instances use SDK settings, not launch arguments.",
      );
    return { ...p, path: executable(p.command, { ...process.env, ...p.env }) };
  });
}
export function provider(id = defaultProvider) {
  const found = providers().find((p) => p.id === id);
  if (!found)
    throw new Error(
      `Unknown agent instance: ${id}. Configure it locally in agents.json.`,
    );
  return found;
}
export function catalog() {
  return providers().map(({ id, label, path, login }) => ({
    id,
    label,
    installed: !!path,
    login: login || "Use this agent's native login",
  }));
}

export function instanceKey(instance) {
  return createHash("sha256")
    .update(
      JSON.stringify({
        // Never resume a thread created with Pitwall's former external sandbox
        // or suppressed project settings. Board history bootstraps a fresh one.
        execution: "native-project-v1",
        driver: instance.driver,
        command: instance.path || instance.command,
        args: instance.args,
        env: instance.env || {},
        model: instance.model || null,
      }),
    )
    .digest("hex");
}
