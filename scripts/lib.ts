import { createHash } from "node:crypto";
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  renameSync,
  statSync,
  unlinkSync,
} from "node:fs";
import { dirname } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

export async function sha256File(path: string): Promise<string> {
  const h = createHash("sha256");
  for await (const chunk of createReadStream(path)) h.update(chunk as Buffer);
  return h.digest("hex");
}

/** Downloads `url` to `dest`, verifying size and sha256. Skips when `dest` already has the right size. */
export async function fetchVerified(
  url: string,
  dest: string,
  size: number,
  sha256: string,
  label: string,
) {
  if (existsSync(dest) && statSync(dest).size === size) {
    console.log(`${label}: already present`);
    return;
  }
  mkdirSync(dirname(dest), { recursive: true });
  const part = `${dest}.part`;
  console.log(`${label}: downloading ${url}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok || !res.body) throw new Error(`${label}: HTTP ${res.status}`);
  await pipeline(Readable.fromWeb(res.body as never), createWriteStream(part));
  const got = statSync(part).size;
  if (got !== size) {
    unlinkSync(part);
    throw new Error(`${label}: size mismatch (${got} != ${size})`);
  }
  if ((await sha256File(part)) !== sha256) {
    unlinkSync(part);
    throw new Error(`${label}: checksum mismatch`);
  }
  renameSync(part, dest);
  console.log(`${label}: ok`);
}
