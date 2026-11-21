// The SDK owns reconnects, backoff and its cursor. Rust receives invalidations,
// then reads an authoritative snapshot, coalescing bursts while writes finish.
import { RUN_CHANNEL, CONVERSATION_LABEL } from "./feedback.mjs";

export function notice(event) {
  if (event.type.startsWith("channel.")) return { kind: "reset" };
  const message = event.message;
  if (!message) return null;
  return {
    kind: "changed",
    channel: message.channel,
    thread: message.re ?? message.id,
    parent: message.labels?.[CONVERSATION_LABEL] ?? null,
    root: !message.re,
  };
}

export async function follow(team, emit, signal) {
  for await (const event of team.events(
    { all: true },
    {
      signal,
      // The first live notification follows the initial cursor read. A fresh
      // snapshot after it closes the startup race between snapshot and feed.
      onState: (state) => emit({ kind: "state", state }),
    },
  )) {
    const change = notice(event);
    if (change && (change.channel !== RUN_CHANNEL || change.parent))
      emit(change);
  }
}
