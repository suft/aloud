import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fetchVerified } from "./lib.ts";

const URL =
  "https://github.com/thewh1teagle/piper-rs/releases/download/espeak-ng-files/espeak-ng-data.tar.gz";
const SIZE = 9177969;
const SHA = "fd250d1c3dc5d85e4aa68b1be34031f753b9a072d9dc622eb413a8dc4771dcb8";

const root = resolve(import.meta.dir, "..");
const dest = join(root, "src-tauri", "resources", "espeak-ng-data");

if (existsSync(join(dest, "en_dict"))) {
  console.log("espeak-ng-data: already present");
  process.exit(0);
}

const tmp = mkdtempSync(join(tmpdir(), "aloud-espeak-"));
try {
  const archive = join(tmp, "espeak-ng-data.tar.gz");
  try {
    await fetchVerified(URL, archive, SIZE, SHA, "espeak-ng-data");
  } catch (e) {
    const msg = String((e as Error).message);
    console.error(msg.includes("checksum") ? "espeak-ng-data checksum mismatch" : msg);
    process.exit(1);
  }
  const x = Bun.spawnSync(["tar", "-xzf", archive, "-C", tmp]);
  if (x.exitCode !== 0) {
    console.error(x.stderr.toString());
    process.exit(1);
  }
  const src = join(tmp, "espeak-ng-data");
  mkdirSync(dest, { recursive: true });
  for (const f of [
    "phondata",
    "phondata-manifest",
    "phontab",
    "phonindex",
    "intonations",
    "en_dict",
  ]) {
    cpSync(join(src, f), join(dest, f));
  }
  mkdirSync(join(dest, "lang", "gmw"), { recursive: true });
  for (const f of readdirSync(join(src, "lang", "gmw"))) {
    if (f.startsWith("en")) cpSync(join(src, "lang", "gmw", f), join(dest, "lang", "gmw", f));
  }
  cpSync(join(src, "voices"), join(dest, "voices"), { recursive: true });
  console.log("espeak-ng-data: installed");
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
