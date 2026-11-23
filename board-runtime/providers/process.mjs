import { spawn } from "node:child_process";
import { once } from "node:events";

export function launch(instance, args, cwd) {
  if (!instance.path)
    throw new Error(
      `${instance.label} is not installed. Install it and run ${instance.login || "its native login"}.`,
    );
  // The installed agent owns configuration, trust and execution permissions.
  const child = spawn(instance.path, [...instance.args, ...args], {
    cwd,
    env: { ...process.env, ...instance.env },
    stdio: ["pipe", "pipe", "pipe"],
  });
  // A closed pipe during cancellation must not become an uncaught exception.
  child.stdin.on("error", () => {});
  child.stderr.pipe(process.stderr);
  return child;
}
export async function stop(child) {
  if (
    child.pid === undefined ||
    child.exitCode !== null ||
    child.signalCode !== null
  )
    return;
  const exited = once(child, "exit").catch(() => {});
  child.kill("SIGTERM");
  const timer = setTimeout(() => child.kill("SIGKILL"), 2000);
  try {
    await exited;
  } finally {
    clearTimeout(timer);
  }
}
