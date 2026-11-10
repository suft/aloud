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
import { dirname, join } from "node:path";
import { Readable, Transform } from "node:stream";
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

const ATTEMPTS = 3;
const STALL_MS = 30_000;

/** A download failure worth another attempt: network error, 429/5xx, stalled body, short body. */
class RetryableError extends Error {}

function removeQuietly(path: string) {
  try {
    unlinkSync(path);
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
  }
}

/** One download attempt into `part`; throws `RetryableError` for failures another attempt may fix. */
async function download(url: string, part: string, size: number, label: string) {
  const controller = new AbortController();
  let timer: NodeJS.Timeout | undefined;
  const arm = () => {
    clearTimeout(timer);
    timer = setTimeout(() => controller.abort(), STALL_MS);
  };
  const stalled = () => new RetryableError(`${label}: no data for ${STALL_MS / 1000} s`);
  arm();
  try {
    let res: Response;
    try {
      res = await fetch(url, { redirect: "follow", signal: controller.signal });
    } catch (e) {
      throw controller.signal.aborted
        ? stalled()
        : new RetryableError(`${label}: ${e instanceof Error ? e.message : String(e)}`);
    }
    if (!res.ok || !res.body) {
      const message = `${label}: HTTP ${res.status}`;
      throw res.status >= 500 || res.status === 429
        ? new RetryableError(message)
        : new Error(message);
    }
    const watchdog = new Transform({
      transform(chunk, _encoding, done) {
        arm();
        done(null, chunk);
      },
    });
    try {
      await pipeline(Readable.fromWeb(res.body as never), watchdog, createWriteStream(part));
    } catch (e) {
      throw controller.signal.aborted
        ? stalled()
        : new RetryableError(`${label}: ${e instanceof Error ? e.message : String(e)}`);
    }
  } finally {
    clearTimeout(timer);
  }
  const got = statSync(part).size;
  if (got !== size) throw new RetryableError(`${label}: size mismatch (${got} != ${size})`);
}

/**
 * Downloads `url` to `dest`, verifying size and sha256. An existing `dest` of the right size is
 * hashed and kept when it matches, and replaced when it doesn't. Network errors, 429/5xx
 * responses, stalled and short bodies are retried; a checksum mismatch or another HTTP status is not.
 */
export async function fetchVerified(
  url: string,
  dest: string,
  size: number,
  sha256: string,
  label: string,
) {
  if (existsSync(dest) && statSync(dest).size === size) {
    if ((await sha256File(dest)) === sha256) {
      console.log(`${label}: already present`);
      return;
    }
    console.log(`${label}: the existing file is damaged; downloading again`);
    unlinkSync(dest);
  }
  mkdirSync(dirname(dest), { recursive: true });
  const part = `${dest}.part`;
  for (let attempt = 1; ; attempt++) {
    console.log(`${label}: downloading ${url}`);
    try {
      await download(url, part, size, label);
      break;
    } catch (e) {
      removeQuietly(part);
      if (!(e instanceof RetryableError) || attempt === ATTEMPTS) throw e;
      const next = attempt + 1;
      console.log(
        `${label}: ${e.message.replace(`${label}: `, "")}; retrying (${next}/${ATTEMPTS})`,
      );
      await Bun.sleep(2000 * (next - 1));
    }
  }
  if ((await sha256File(part)) !== sha256) {
    removeQuietly(part);
    throw new Error(`${label}: checksum mismatch`);
  }
  renameSync(part, dest);
  console.log(`${label}: ok`);
}

export const KOKORO_BASE = catalog.baseUrl;

/** `[file name, size, sha256]` of the Kokoro files, from `crates/aloud-core/models.json`. */
export const KOKORO_FILES: Record<"voices" | "fast" | "standard", [string, number, string]> = {
  voices: [catalog.voices.file, catalog.voices.size, catalog.voices.sha256],
  fast: [catalog.fast.file, catalog.fast.size, catalog.fast.sha256],
  standard: [catalog.standard.file, catalog.standard.size, catalog.standard.sha256],
};
