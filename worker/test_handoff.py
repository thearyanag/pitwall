import unittest
from handoff import plan_context, select_history


def message(id, text, run=None, kind=None):
    return {
        "id": id,
        "text": text,
        "token": id + ":" + text,
        "run": run,
        "kind": kind,
        "author": "Test",
        "attachments": [],
    }


class HandoffTests(unittest.TestCase):
    def test_codex_antigravity_codex_preserves_missing_context_and_pixels(self):
        root = message("root", "Read the screenshot")
        image = {"id": "image-original", "path": "original.png"}
        prepared = {
            "conversation": "root",
            "task": "root",
            "request": root,
            "history": [],
            "images": [image],
        }
        first = plan_context(prepared, {}, False, "history.json")
        saved_codex = first["receipt"]
        own = message("codex-answer", "MAPLE-73", "root", "message")
        review = message("review", "Remember ORBIT-29")
        prepared.update(task="review-run", request=review, history=[root, own])
        other = plan_context(prepared, {}, False, "history.json")
        self.assertIn("MAPLE-73", other["text"])
        self.assertEqual(other["images"], [image])
        answer = message("review-answer", "ORBIT-29 verified", "review-run", "result")
        feedback_image = {"id": "image-feedback", "path": "feedback.png"}
        prepared.update(
            task="return-run",
            request=message("return", "Combine both"),
            history=[root, own, review, answer],
            images=[image, feedback_image],
        )
        back = plan_context(prepared, saved_codex, True, "history.json")
        self.assertEqual(back["receipt"]["history_ids"], ["review", "review-answer"])
        self.assertNotIn("MAPLE-73", back["text"], "do not replay Codex's own answer")
        self.assertIn("ORBIT-29", back["text"])
        self.assertEqual(back["images"], [image, feedback_image])
        self.assertNotIn(
            "Combine both", back["text"], "new request is supplied separately"
        )

    def test_same_agent_sends_only_new_images_and_includes_comments_and_edits(self):
        root = message("root", "Original")
        old = {"id": "old", "path": "old.png"}
        prepared = {
            "conversation": "root",
            "task": "one",
            "request": root,
            "history": [],
            "images": [old],
        }
        saved = plan_context(prepared, {}, False, "history.json")["receipt"]
        own = message("answer", "Result", "one", "result")
        new = {"id": "new", "path": "new.png"}
        prepared.update(
            task="two",
            request=message("next", "Next"),
            history=[root, own],
            images=[old, new],
        )
        continuation = plan_context(prepared, saved, True, "history.json")
        self.assertEqual(continuation["text"], "")
        self.assertEqual(continuation["images"], [new])
        prepared["history"] += [message("comment", "Keep the design accessible")]
        self.assertIn(
            "accessible", plan_context(prepared, saved, True, "history.json")["text"]
        )
        prepared["history"][0] = message("root", "Edited original")
        self.assertIn(
            "Edited original",
            plan_context(prepared, saved, True, "history.json")["text"],
        )

    def test_bounded_history_keeps_whole_messages_and_does_not_mark_omissions_seen(
        self,
    ):
        history = [
            message("huge", "x" * 40000),
            message("small", "Keep all of this 🌱"),
        ]
        selected, omitted = select_history(history)
        self.assertEqual(selected, [history[1]])
        self.assertEqual(omitted, ["huge"])
        prepared = {
            "conversation": "root",
            "task": "run",
            "request": message("current", "Current"),
            "history": history,
            "images": [],
        }
        result = plan_context(prepared, {}, False, "history.json")
        self.assertNotIn(history[0]["token"], result["receipt"]["covered"])
        self.assertIn("history.json", result["text"])


if __name__ == "__main__":
    unittest.main()
