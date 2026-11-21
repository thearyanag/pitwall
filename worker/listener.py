#!/usr/bin/env python3
"""A local supervisor: Board waits for work; its CLI claims and runs it."""
import fcntl
import json
import os
import pathlib
import signal
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
NODE = os.environ.get("SPROUT_NODE", "node")
TEAM = os.environ.get("SPROUT_TEAM", "")
if not TEAM:
    raise SystemExit("Set SPROUT_TEAM to your Vercel team before starting the worker.")
BOARD = os.environ.get("SPROUT_BOARD") or "General"
WORKER = ROOT / "worker"
STATE = pathlib.Path(os.environ.get("SPROUT_STATE_DIR", WORKER)).expanduser().resolve()
STATE.mkdir(parents=True, exist_ok=True)
STATUS = STATE / "status.json"
stop = False
active = None


def status(state, task=None, error=None):
    data = {
        "state": state,
        "task": task,
        "error": error,
        "pid": os.getpid(),
        "team": TEAM,
        "board": BOARD,
        "updated_at": int(time.time()),
    }
    tmp = STATUS.with_suffix(".tmp")
    tmp.write_text(json.dumps(data))
    os.replace(tmp, STATUS)


def interrupt(signum, frame):
    global stop
    stop = True
    if active and active.poll() is None:
        try:
            os.killpg(active.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass


def stop_child():
    if active and active.poll() is None:
        try:
            os.killpg(active.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            active.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(active.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            active.wait()
    if active:
        for pipe in (active.stdin, active.stdout):
            if pipe and not pipe.closed:
                pipe.close()


def await_child(state, task=None):
    """Heartbeat and shutdown only; this loop never queries Board."""
    stopping_at = None
    while active.poll() is None:
        status("stopping" if stop else state, task)
        if stop:
            stopping_at = stopping_at or time.monotonic()
            if time.monotonic() - stopping_at > 10:
                try:
                    os.killpg(active.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        time.sleep(0.5)


signal.signal(signal.SIGINT, interrupt)
signal.signal(signal.SIGTERM, interrupt)
lock = (STATE / "queue.lock").open("a+")
try:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    raise SystemExit("A Pitwall queue worker is already running in this workspace.")
print(
    f"Pitwall Codex queue worker · {TEAM} / {BOARD}\nWaiting through Board for your Pitwall tasks across channels. One task at a time. Ctrl+C stops.",
    flush=True,
)
backoff = 3
try:
    while not stop:
        status("waiting")
        request = json.dumps({"op": "queue_wait", "team": TEAM, "board": BOARD})
        try:
            active = subprocess.Popen(
                [NODE, str(ROOT / "board-runtime/adapter.mjs"), "-"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                cwd=ROOT,
                start_new_session=True,
            )
            active.stdin.write(request)
            active.stdin.close()
            await_child("waiting")
            if stop:
                break
            out = active.stdout.read()
            active.stdout.close()
            response = json.loads(out)
            if not response.get("ok"):
                raise RuntimeError(response.get("error", "Board request failed"))
            task = response["data"]["task_id"]
            if not task:
                raise RuntimeError("Board wait ended without a task.")
            backoff = 3
        except Exception as error:
            stop_child()
            message = str(error)[:400]
            print(f"Queue unavailable: {message}", flush=True)
            until = time.monotonic() + backoff
            while not stop and time.monotonic() < until:
                status("error", error=message)
                time.sleep(0.5)
            backoff = min(backoff * 2, 60)
            continue
        print(f"Picking up {task}", flush=True)
        status("working", task)
        active = subprocess.Popen(
            [str(WORKER / "Launch Codex Worker.command"), task],
            cwd=ROOT,
            env={
                **os.environ,
                "SPROUT_TEAM": TEAM,
                "SPROUT_BOARD": BOARD,
                "SPROUT_NODE": NODE,
            },
            start_new_session=True,
        )
        await_child("working", task)
        print(f"Task runner exited {active.returncode}.", flush=True)
        if active.returncode:
            # A refused claim may leave the task ready. Avoid a rapid retry loop.
            until = time.monotonic() + 3
            while not stop and time.monotonic() < until:
                status("error", task, "Task runner failed; see the queue log.")
                time.sleep(0.5)

finally:
    stop_child()
    status("stopped")
    print("Pitwall queue worker stopped.", flush=True)
