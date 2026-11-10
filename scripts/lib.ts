import { createHash } from "node:crypto";
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
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

/** Downloads the public-domain Alice in Wonderland EPUB (dev fixture) unless `dest` exists. */
export async function ensureAlice(dest: string): Promise<void> {
  if (existsSync(dest)) return;
  const res = await fetch("https://www.gutenberg.org/ebooks/11.epub3.images", {
    redirect: "follow",
  });
  if (!res.ok) throw new Error(`alice.epub: HTTP ${res.status}`);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, new Uint8Array(await res.arrayBuffer()));
  console.log("alice.epub: ok");
}

export const KOKORO_BASE =
  "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0";

/** `[file name, size, sha256]` of the Kokoro files; keep in sync with `crates/aloud-core/src/models.rs`. */
export const KOKORO_FILES: Record<"voices" | "fast" | "standard", [string, number, string]> = {
  voices: [
    "voices-v1.0.bin",
    28214398,
    "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d",
  ],
  fast: [
    "kokoro-v1.0.int8.onnx",
    92361271,
    "6e742170d309016e5891a994e1ce1559c702a2ccd0075e67ef7157974f6406cb",
  ],
  standard: [
    "kokoro-v1.0.onnx",
    325532387,
    "7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5",
  ],
};
