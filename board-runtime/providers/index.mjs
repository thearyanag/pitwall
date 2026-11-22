import { provider } from "./registry.mjs";
const drivers = {
  codex: () => import("./codex.mjs"),
  claude: () => import("./claude.mjs"),
  acp: () => import("./acp.mjs"),
  grok: () => import("./grok.mjs"),
};
export async function openAgent(id, options, host) {
  const instance = provider(id);
  if (!instance.path)
    throw new Error(
      `${instance.label} is not installed. Install it and run ${instance.login || "its native login"}.`,
    );
  const driver = await drivers[instance.driver]();
  return driver.open(instance, options, host);
}
