import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { imageType, MAX_BYTES } from "./images.mjs";

test("library image inspection retains Pitwall format and size limits", () => {
  const png = readFileSync(
    new URL("../test-assets/vision-card.png", import.meta.url),
  );
  assert.equal(imageType(png), "image/png");
  // Header inspection only; decoding pixels remains the Rust image library's job.
  const jpegHeader = Buffer.from([
    255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
    255, 192, 0, 17, 8, 0, 1, 0, 1, 3, 1, 17, 0, 2, 17, 0, 3, 17, 0, 255, 217,
  ]);
  assert.equal(imageType(jpegHeader), "image/jpeg");

  assert.throws(
    () => imageType(Buffer.from("not an image")),
    /supported PNG or JPEG/,
  );
  assert.throws(() => imageType(png.subarray(0, 15)), /supported PNG or JPEG/);
  assert.throws(() => imageType(Buffer.alloc(MAX_BYTES + 1)), /20 MB/);
  const oversized = Buffer.from(png);
  oversized.writeUInt32BE(50000, 16);
  oversized.writeUInt32BE(50000, 20);
  assert.throws(() => imageType(oversized), /40 megapixels/);
  const gif = Buffer.from(
    "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",
    "base64",
  );
  assert.throws(() => imageType(gif), /supported PNG or JPEG/);
});

test("installed image storage stays outside the release directory", async () => {
  const { spawnSync } = await import("node:child_process");
  const { resolve, join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const state = resolve(tmpdir(), "pitwall-state-" + process.pid);
  const source = `import {IMAGE_ROOT,DOWNLOAD_ROOT} from ${JSON.stringify(new URL("./images.mjs", import.meta.url).href)};console.log(JSON.stringify([IMAGE_ROOT,DOWNLOAD_ROOT]));`;
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "-e", source],
    { encoding: "utf8", env: { ...process.env, SPROUT_STATE_DIR: state } },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), [
    join(state, "images"),
    join(state, "downloads"),
  ]);
});
