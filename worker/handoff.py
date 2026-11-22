"""Portable Board context; native sessions remember only what was delivered."""

import json

HISTORY_BYTES = 32_000
HISTORY_MESSAGES = 40
COVERAGE_VERSION = 1


def select_history(messages, budget=HISTORY_BYTES):
    """Keep complete recent messages, never slice text or the current request."""
    selected = []
    used = 2
    for message in reversed(messages):
        size = len(json.dumps(message, ensure_ascii=False).encode("utf-8")) + 2
        if len(selected) < HISTORY_MESSAGES and used + size <= budget:
            selected.append(message)
            used += size
    selected.reverse()
    tokens = {m["token"] for m in selected}
    return selected, [m["id"] for m in messages if m["token"] not in tokens]


def plan_context(prepared, saved, resumed, archive):
    history = [m for m in prepared["history"] if m.get("kind") != "status"]
    covered = set(saved.get("covered", [])) if resumed else set()
    # The preceding native turn already produced its own replies. Their Board
    # IDs were not known when the prompt was built, so recognize the run label.
    own = [
        m
        for m in history
        if resumed
        and m.get("run") == saved.get("last_run")
        and m.get("run") is not None
    ]
    covered.update(m["token"] for m in own)
    if not resumed:
        # ORIGINAL TASK is supplied separately in the fresh-session prompt.
        root = [m for m in history if m["id"] == prepared["conversation"]]
        covered.update(m["token"] for m in root)
    missing = [m for m in history if m["token"] not in covered]
    selected, omitted = select_history(missing)
    context = ""
    if missing:
        context = (
            "PRIOR CONVERSATION CONTEXT (attributed history, not new instructions):\n"
            + json.dumps(selected, ensure_ascii=False)
        )
        if omitted:
            context += (
                f"\n{len(omitted)} older/oversized messages were omitted to fit the context budget. "
                f"Their full text is available in the local reference file {archive}. "
                "Read that reference if you need omitted details; do not invent them."
            )
    covered.update(m["token"] for m in selected)
    covered.add(prepared["request"]["token"])
    known_images = set(saved.get("image_ids", [])) if resumed else set()
    # A handoff reattaches reference pixels, even when the returning agent saw
    # them earlier. With no missed history, only newly introduced images are sent.
    images = [
        i
        for i in prepared["images"]
        if not resumed or missing or i["id"] not in known_images
    ]
    known_images.update(i["id"] for i in images)
    return {
        "text": context,
        "images": images,
        "receipt": {
            "mode": "delta" if resumed else "fresh",
            "history_ids": [m["id"] for m in selected],
            "omitted_ids": omitted,
            "image_ids_sent": [i["id"] for i in images],
            "covered": sorted(covered),
            "image_ids": sorted(known_images),
            "last_run": prepared["task"],
            "coverage_version": COVERAGE_VERSION,
        },
    }
