# Local agent verification — 2026-11-22

These checks exercised the installed native agents through Pitwall’s adapters.
They did not create live Board tasks or change project files.

| Agent | Initial response | Resume after process restart | Screenshot input |
| --- | --- | --- | --- |
| Codex | Passed | Passed | Passed |
| Grok Build 1.0.92 | Passed | Passed | Passed |
| Antigravity CLI 1.5.14 | Passed | Passed | Passed |
| Claude Code | Skipped at the user’s request | Not run | Not run |

All three tested agents returned `PITWALL_OK`, remembered `TANGERINE73` after closing
and restarting their native process, and read `MAPLE-73`, a blue triangle and an
orange square from the synthetic image. The image prompt did not supply those
answers and prohibited inspecting local files.

Grok emitted multiple native assistant streams during a prompt. A small Grok
adapter now separates streams by its `streamStartMs` metadata and ignores
explicit replay updates. A recorded, synthetic fixture covers this behavior
without deleting legitimate identical messages or chunks based on their text.

Gemini CLI 0.70.0 rejected the consumer account both through ACP and its normal
CLI, citing migration to Antigravity. The Gemini CLI built-in was removed at the
user’s request; Google now appears as Antigravity (`agy acp`). See the
[migration announcement](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/).
Antigravity CLI 1.5.14 was subsequently installed and signed in. Its live
adapter test passed without further code changes, including native process
restart/resume and image input. One observed session startup took 2.9 seconds;
this is a single local measurement, not a latency guarantee.

All 32 JavaScript tests and the adapter SDK type checks passed after these changes.
The preceding implementation pass also passed 34 Rust tests, Rust lint/build,
and 5 installer tests; this pass changed no Rust or installer code.


## Full Pitwall flow

The Antigravity task-and-feedback flow was subsequently verified through the
actual Pitwall UI in Rex. Both submissions used the composer. The Board queue
claimed each run automatically, and both completed in the original conversation.
The second run resumed the same native Antigravity session and shortened the
welcome message to one sentence while retaining `APRICOT-82`, which was not
repeated in the feedback prompt. Both runs were answer-only; no source files
were changed by the worker. The UI showed the revised result and an idle worker.


## Agent switching round trip

Codex → Antigravity → Codex was verified through the Pitwall composer in Rex,
within one Board conversation. The first turn attached the MAPLE-73 reference
image and introduced a Tuesday scheduling note. The Antigravity reply added the
CEDAR-19 image and the ORBIT-29 approval decision. The final prompt repeated none
of those values; Codex recovered all of them and correctly described both images.

The final Codex run resumed the original native Codex session. Its delta handoff
contained only the intervening Antigravity request/results, not its own earlier
messages. Both reference images were passed again, and their downloaded SHA-256
hashes matched across runs. All three jobs completed automatically, without a
manual claim or worker restart. One pickup was delayed while Board's wait was
active; the queue eventually woke and completed it without intervention.

The tests passed: 35 Rust, 34 JavaScript, and 3 Python handoff tests, plus SDK type
checks, Rust lint and build. The tests cover immutable queued selections, retries
with changed agents, lost responses, saved reply choices, comment/edit context,
bounded whole-message selection, and image delivery on switches and returns.
Model switching within an agent is not part of this pass.


## Model switching

The following installed-runtime checks passed with two small prompts each:

| Agent | Switch | Context retained | Reset to default | Same native session |
| --- | --- | --- | --- | --- |
| Codex | GPT-6-Astra → GPT-5.6-Luna | Yes | Yes | Yes |
| Antigravity | Gemini 3.8 Flash → Claude Sonnet 4.6 | Yes | Yes | Yes |
| Grok Build | Grok 4.7 → Grok 4.7 Fast | Yes | Yes | Yes |

These tests use runtime-reported model selection and session IDs, not an LLM's
self-description. Antigravity's Claude model uses Antigravity's own login; the
separate Claude Code runtime remains covered by SDK contract tests, not a live
account test. A preceding Grok 4.6 test confirmed selection but generation exceeded
the 180-second test timeout. That is recorded as an incomplete test, not a pass.

The model picker was also exercised in the actual Pitwall UI in Rex. The existing
agent-switch screenshot conversation selected GPT-5.6-Luna for a new reply. Board
stored that choice, the worker resumed the original Codex session, and the model
returned Tuesday, ORBIT-29, MAPLE-73 and CEDAR-19 without those values in the new
request. Both reference images were reattached on the model change.

The checks passed: 38 JavaScript tests, 36 Rust tests, 3 Python handoff tests,
SDK type checks, Rust lint and build.
