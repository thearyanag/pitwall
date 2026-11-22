import { open as openAcp } from "./acp.mjs";

// Grok may emit several assistant streams within one prompt (for example when
// tools finish initializing). Its extension marks stream boundaries and replays.
// Keep these semantics out of the generic ACP driver; never deduplicate by text.
/** @type {import("./contract.js").OpenAgent} */
export async function open(instance, options, host) {
  return openAcp(instance, options, host, {
    streamKey: (notification) => notification._meta?.streamStartMs,
    isReplay: (notification) => notification._meta?.isReplay === true,
  });
}
