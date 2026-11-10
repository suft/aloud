import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { ensureAlice, fetchVerified, KOKORO_BASE, KOKORO_FILES } from "./lib.ts";

const dir = resolve(process.argv[2] ?? ".dev-data/models");

mkdirSync(dir, { recursive: true });
for (const [name, size, sha] of Object.values(KOKORO_FILES)) {
  await fetchVerified(`${KOKORO_BASE}/${name}`, join(dir, name), size, sha, name);
}

// Dev e2e fixture: Alice in Wonderland (public domain).
await ensureAlice(join(dirname(dir), "alice.epub"));
