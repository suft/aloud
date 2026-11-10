import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fetchVerified, tarCommand } from "./lib.ts";

const URL =
  "https://github.com/thewh1teagle/piper-rs/releases/download/espeak-ng-files/espeak-ng-data.tar.gz";
const SIZE = 9177969;
const SHA = "fd250d1c3dc5d85e4aa68b1be34031f753b9a072d9dc622eb413a8dc4771dcb8";

const root = resolve(import.meta.dir, "..");
const resources = join(root, "src-tauri", "resources");
const dest = join(resources, "espeak-ng-data");
const STAGING_PREFIX = ".espeak-ng-data-";

/** The install is complete and matches the pinned archive only if its marker says so. */
function upToDate(): boolean {
  try {
    return readFileSync(join(dest, ".aloud-source"), "utf8").trim() === SHA;
  } catch {
    return false;
  }
}

if (upToDate()) {
  console.log("espeak-ng-data: already present");
  process.exit(0);
}

// Leftovers of an interrupted run.
mkdirSync(resources, { recursive: true });
for (const name of readdirSync(resources)) {
  if (name.startsWith(STAGING_PREFIX))
    rmSync(join(resources, name), { recursive: true, force: true });
}

const tmp = mkdtempSync(join(tmpdir(), "aloud-espeak-"));
const staging = join(resources, `${STAGING_PREFIX}${process.pid}`);
try {
  const archive = join(tmp, "espeak-ng-data.tar.gz");
  try {
    await fetchVerified(URL, archive, SIZE, SHA, "espeak-ng-data");
  } catch (e) {
    const msg = String((e as Error).message);
    console.error(msg.includes("checksum") ? "espeak-ng-data checksum mismatch" : msg);
    process.exit(1);
  }
  const x = Bun.spawnSync([tarCommand(), "-xzf", "espeak-ng-data.tar.gz"], { cwd: tmp });
  if (x.exitCode !== 0) {
    console.error(x.stderr.toString());
    process.exit(1);
  }
  const src = join(tmp, "espeak-ng-data");
  mkdirSync(staging, { recursive: true });
  for (const f of [
    "phondata",
    "phondata-manifest",
    "phontab",
    "phonindex",
    "intonations",
    "en_dict",
  ]) {
    cpSync(join(src, f), join(staging, f));
  }
  mkdirSync(join(staging, "lang", "gmw"), { recursive: true });
  for (const f of readdirSync(join(src, "lang", "gmw"))) {
    if (f.startsWith("en")) cpSync(join(src, "lang", "gmw", f), join(staging, "lang", "gmw", f));
  }
  cpSync(join(src, "voices"), join(staging, "voices"), { recursive: true });
  // Written last, then the whole directory is swapped in: a killed run never leaves a half install.
  writeFileSync(join(staging, ".aloud-source"), `${SHA}\n`);
  rmSync(dest, { recursive: true, force: true });
  renameSync(staging, dest);
  console.log("espeak-ng-data: installed");
} finally {
  rmSync(tmp, { recursive: true, force: true });
  rmSync(staging, { recursive: true, force: true });
}
