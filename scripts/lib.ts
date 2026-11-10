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
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import catalog from "../crates/aloud-core/models.json";

export async function sha256File(path: string): Promise<string> {
  const h = createHash("sha256");
  for await (const chunk of createReadStream(path)) h.update(chunk as Buffer);
  return h.digest("hex");
}

/**
 * The tar to extract archives with. On Windows that is the bsdtar that ships with the system
 * rather than the one from Git: GNU tar would read a drive letter in an argument as a remote
 * host, so callers also pass relative paths.
 */
export function tarCommand(): string {
  return process.platform === "win32"
    ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe")
    : "tar";
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

export const KOKORO_BASE = catalog.baseUrl;

/** `[file name, size, sha256]` of the Kokoro files, from `crates/aloud-core/models.json`. */
export const KOKORO_FILES: Record<"voices" | "fast" | "standard", [string, number, string]> = {
  voices: [catalog.voices.file, catalog.voices.size, catalog.voices.sha256],
  fast: [catalog.fast.file, catalog.fast.size, catalog.fast.sha256],
  standard: [catalog.standard.file, catalog.standard.size, catalog.standard.sha256],
};
