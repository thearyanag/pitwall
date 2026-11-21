import { imageSize } from "image-size";
import { createHash } from "node:crypto";
import {
  readFileSync,
  writeFileSync,
  renameSync,
  mkdirSync,
  lstatSync,
  realpathSync,
  existsSync,
} from "node:fs";
import { resolve, join, sep } from "node:path";
import { fileURLToPath } from "node:url";
export const MAX_BYTES = 20 * 1024 * 1024;
export const IMAGE_ROOT = fileURLToPath(
  new URL("../worker/images/", import.meta.url),
);
export const DOWNLOAD_ROOT = fileURLToPath(
  new URL("../worker/downloads/", import.meta.url),
);
const hash = (data) => createHash("sha256").update(data).digest("hex");
export function imageType(bytes) {
  if (bytes.length > MAX_BYTES)
    throw new Error("Images must be 20 MB or smaller.");
  let metadata;
  try {
    metadata = imageSize(bytes);
  } catch {
    throw new Error("Attachment is not a supported PNG or JPEG image.");
  }
  if (!["png", "jpg"].includes(metadata.type)) {
    throw new Error("Attachment is not a supported PNG or JPEG image.");
  }
  if (
    !metadata.width ||
    !metadata.height ||
    metadata.width * metadata.height > 40_000_000
  ) {
    throw new Error("Image exceeds 40 megapixels.");
  }
  return metadata.type === "png" ? "image/png" : "image/jpeg";
}

export async function uploadImages(team, req, root = IMAGE_ROOT) {
  const images = req.attachments ?? [];
  if (!images.length) return [];
  if (images.length > 10) throw new Error("Attach at most 10 images.");
  if (!/^[a-zA-Z0-9_-]+$/.test(req.key)) throw new Error("Invalid draft key.");
  const folder = realpathSync(join(root, req.key));
  const me = await team.me();
  const target = [req.team ?? "", req.board ?? "", me.principal].join("|");
  const receiptPath = join(folder, "uploads.json");
  const receipt = existsSync(receiptPath)
    ? JSON.parse(readFileSync(receiptPath, "utf8"))
    : { target, files: {} };
  if (receipt.target !== target)
    throw new Error(
      "Pending image uploads belong to a different Board or account.",
    );
  const attachments = [];
  for (const image of images) {
    const path = resolve(image.path);
    if (
      lstatSync(path).isSymbolicLink() ||
      !realpathSync(path).startsWith(folder + sep)
    )
      throw new Error("Image is outside this task’s staged files.");
    if (lstatSync(path).size > MAX_BYTES)
      throw new Error("Image exceeds 20 MB.");
    const bytes = readFileSync(path);
    const type = imageType(bytes);
    const digest = hash(bytes);
    if (!receipt.files[digest]) {
      receipt.files[digest] = await team.upload({
        data: new Uint8Array(bytes),
        name: String(image.name).replace(/[\x00-\x1f\x7f/\\]/g, "_"),
        type,
      });
      const temporary = receiptPath + ".tmp";
      writeFileSync(temporary, JSON.stringify(receipt));
      renameSync(temporary, receiptPath);
    }
    attachments.push(receipt.files[digest]);
  }
  return attachments;
}
export async function downloadImage(team, attachment, dir = DOWNLOAD_ROOT) {
  if (!["image/png", "image/jpeg"].includes(attachment.type))
    throw new Error("Only PNG and JPEG images can be previewed.");
  if (attachment.size > MAX_BYTES) throw new Error("Image exceeds 20 MB.");
  mkdirSync(dir, { recursive: true });
  const ext = attachment.type === "image/png" ? "png" : "jpg";
  const path = join(dir, `${hash(String(attachment.id))}.${ext}`);
  if (existsSync(path)) {
    imageType(readFileSync(path));
    return path;
  }
  const response = await team.download(attachment.id);
  if (!response.ok)
    throw new Error(`Image download failed (${response.status}).`);
  const chunks = [];
  let size = 0;
  const reader = response.body.getReader();
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > MAX_BYTES) {
      await reader.cancel();
      throw new Error("Image download exceeds 20 MB.");
    }
    chunks.push(Buffer.from(value));
  }
  const bytes = Buffer.concat(chunks);
  const type = imageType(bytes);
  if (type !== attachment.type)
    throw new Error("Image content does not match its attachment type.");
  const temporary = path + ".tmp";
  writeFileSync(temporary, bytes);
  renameSync(temporary, path);
  return path;
}
