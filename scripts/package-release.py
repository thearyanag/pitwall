#!/usr/bin/env python3
"""Build self-contained macOS archives from a clean committed checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
NODE_VERSION = "22.22.3"
TARGETS = {"arm64": "aarch64-apple-darwin", "x64": "x86_64-apple-darwin"}


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def download(url, path):
    if not path.exists():
        temporary = path.with_suffix(path.suffix + ".part")
        run(
            "curl",
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fLsS",
            "--retry",
            "3",
            url,
            "-o",
            str(temporary),
        )
        temporary.replace(path)


def digest(path):
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", choices=TARGETS, action="append")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if output("git", "status", "--porcelain"):
        raise SystemExit(
            "Commit the source first; release archives require a clean checkout."
        )
    version = (
        "v" + tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    )
    commit = output("git", "rev-parse", "HEAD")
    tracked = output("git", "ls-files", "-z").split("\0")
    work, out = args.work.resolve(), args.out.resolve()
    downloads = work / "downloads"
    downloads.mkdir(parents=True, exist_ok=True)
    out.mkdir(parents=True, exist_ok=True)
    sums = downloads / f"node-{NODE_VERSION}-SHASUMS256.txt"
    download(f"https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt", sums)
    expected = {
        line.split()[1]: line.split()[0] for line in sums.read_text().splitlines()
    }
    env = {**os.environ, "MACOSX_DEPLOYMENT_TARGET": "13.5"}
    target_dir = Path(env.get("CARGO_TARGET_DIR", str(work / "target"))).resolve()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    for arch in args.arch or list(TARGETS):
        stage = work / f"stage-{arch}"
        if stage.exists():
            shutil.rmtree(stage)
        bundle = stage / "pitwall"
        bundle.mkdir(parents=True)
        for name in tracked:
            if not name:
                continue
            source, destination = ROOT / name, bundle / name
            if source.is_symlink():
                raise RuntimeError(f"Tracked symlinks need review: {name}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        target = TARGETS[arch]
        print(f"Building {version} for {target}", flush=True)
        run(
            "cargo",
            "build",
            "--release",
            "--locked",
            "--target",
            target,
            "--bins",
            cwd=ROOT,
            env=env,
        )
        for name in ("sprout", "reader"):
            shutil.copy2(target_dir / target / "release" / name, bundle / name)
            run("codesign", "--force", "--sign", "-", str(bundle / name))
        node_name = f"node-v{NODE_VERSION}-darwin-{arch}"
        archive = downloads / f"{node_name}.tar.gz"
        download(f"https://nodejs.org/dist/v{NODE_VERSION}/{archive.name}", archive)
        if digest(archive) != expected[archive.name]:
            raise RuntimeError(f"Official Node checksum mismatch: {archive.name}")
        node_root = bundle / "runtimes/node"
        (node_root / "bin").mkdir(parents=True)
        with tarfile.open(archive) as source:
            for item, destination in [
                ("bin/node", node_root / "bin/node"),
                ("LICENSE", node_root / "LICENSE"),
            ]:
                with source.extractfile(
                    f"{node_name}/{item}"
                ) as stream, destination.open("wb") as file:
                    shutil.copyfileobj(stream, file)
        (node_root / "bin/node").chmod(0o755)
        run(
            "npm",
            "ci",
            "--prefix",
            str(bundle / "board-runtime"),
            "--omit=dev",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            f"--cpu={arch}",
            "--os=darwin",
            cwd=ROOT,
        )
        (bundle / "VERSION").write_text(version + "\n")
        (bundle / "BUILD-INFO.json").write_text(
            json.dumps(
                {
                    "version": version,
                    "commit": commit,
                    "target": target,
                    "node": NODE_VERSION,
                    "macos_minimum": "13.5",
                    "signing": "ad-hoc; not Developer ID notarized",
                },
                indent=2,
            )
            + "\n"
        )
        run(str(bundle / "sprout"), "--version")
        run(str(node_root / "bin/node"), "--version")
        run(
            str(node_root / "bin/node"),
            str(bundle / "board-runtime/node_modules/.bin/vercel"),
            "--version",
            env={**env, "VERCEL_TELEMETRY_DISABLED": "1"},
        )
        # Exercise production dependencies under the archive's actual Node/CPU,
        # without requiring any installed agent or using a live account.
        run(
            str(node_root / "bin/node"),
            "--input-type=module",
            "-e",
            "await import('@agentclientprotocol/sdk'); "
            "await import('@anthropic-ai/claude-agent-sdk'); "
            "const {catalog} = await import('./providers/registry.mjs'); "
            "if (catalog().length < 4) throw new Error('Missing agent adapters');",
            cwd=bundle / "board-runtime",
            env={
                **env,
                "PITWALL_AGENTS": str(bundle / "no-local-agent-overrides.json"),
            },
        )
        for required in (
            "board-runtime/agent-runner.mjs",
            "board-runtime/discover-models.mjs",
            "worker/handoff.py",
            "board-runtime/open-project.mjs",
        ):
            if not (bundle / required).is_file():
                raise RuntimeError(f"Release is missing {required}")
        for path in bundle.rglob("*"):
            if path.is_symlink() and not path.resolve().is_relative_to(bundle):
                raise RuntimeError(
                    f"Escaping package symlink: {path.relative_to(bundle)}"
                )

        def metadata(member):
            member.uid = member.gid = 0
            member.uname = member.gname = ""
            return member

        artifact = out / f"pitwall-darwin-{arch}.tar.gz"
        with tarfile.open(artifact, "w:gz") as tar:
            tar.add(bundle, arcname="pitwall", filter=metadata)
        print(
            f"Packaged {artifact.name}: {artifact.stat().st_size // 1024 // 1024} MiB",
            flush=True,
        )
    archives = sorted(out.glob("pitwall-darwin-*.tar.gz"))
    (out / "SHA256SUMS").write_text(
        "".join(f"{digest(path)}  {path.name}\n" for path in archives)
    )


if __name__ == "__main__":
    main()
