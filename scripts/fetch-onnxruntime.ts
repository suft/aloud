// Pre-fills ort-sys's download cache with ONNX Runtime builds mirrored on a GitHub release of this
// repo (see scripts/mirror-onnxruntime.sh), so builds never contact cdn.pyke.io.
//
// ort-sys looks for `<ORT_CACHE_DIR>/dfbin/<target>/<sha256 of pyke's archive>/` and only downloads
// when it is missing, so a directory placed there is used as is, keeping ort-sys's own link setup
// (including the DirectML and CoreML system libraries). Don't set ORT_SKIP_DOWNLOAD or
// CARGO_NET_OFFLINE: ort-sys checks them before the cache and fails.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fetchVerified, tarCommand } from "./lib.ts";

const MIRROR = "https://github.com/suft/aloud/releases/download/onnxruntime-1.31.0";

interface Dist {
  /** Rust target triple ort-sys builds for. */
  target: string;
  /** sha256 of pyke's archive; it names the cache directory. */
  pykeSha: string;
  /** Mirror asset (the pyke archive repacked as .tar.gz). */
  file: string;
  size: number;
  sha256: string;
}

/** Keyed by `${process.platform}-${process.arch}`. `size` and `sha256` are of the mirror asset. */
const DISTS: Record<string, Dist> = {
  "linux-x64": {
    target: "x86_64-unknown-linux-gnu",
    pykeSha: "02aac0d3b4927026292088986d7a377c657c88761b1c068be217a67b35447678",
    file: "onnxruntime-1.31.0-x86_64-unknown-linux-gnu.tar.gz",
    size: 23418372,
    sha256: "813b77548c85b375676e6f45dfc47e2ad79aff453ad103bf75d048a8bface9c3",
  },
  "win32-x64": {
    target: "x86_64-pc-windows-msvc",
    pykeSha: "f99aaef23f12986abb05924c944d8433521263ad756b462df762a5a982c98405",
    file: "onnxruntime-1.31.0-x86_64-pc-windows-msvc-directml.tar.gz",
    size: 66880861,
    sha256: "4f3ff773da51a8d787aecd656a32249efb0a2fd8a2610183225db46e1b4bc2e8",
  },
  "darwin-arm64": {
    target: "aarch64-apple-darwin",
    pykeSha: "4033efca0e6c1606bd2ec3870d4cb19d0dba0ad0160dbfb616e66cae969ed68c",
    file: "onnxruntime-1.31.0-aarch64-apple-darwin-coreml.tar.gz",
    size: 22073893,
    sha256: "b9cc075e49ffa75cae22d06b5f95d93aaf1b49f00107d5fb3ea9ef74ada18ff2",
  },
};

const root = resolve(import.meta.dir, "..");
const key = `${process.platform}-${process.arch}`;
const dist = DISTS[key];
if (!dist) {
  console.log(`onnxruntime: no mirrored build for ${key}; ort will download it`);
  process.exit(0);
}

const cache = process.env.ORT_CACHE_DIR ?? join(root, ".ort-cache");
const dest = join(cache, "dfbin", dist.target, dist.pykeSha);
// ort-sys only creates this directory after verifying pyke's hash, so its presence means done.
if (existsSync(dest)) {
  console.log("onnxruntime: already present");
  process.exit(0);
}

const parent = dirname(dest);
mkdirSync(parent, { recursive: true });
// Leftovers of an interrupted run.
for (const name of readdirSync(parent)) {
  if (name.startsWith(".staging-")) rmSync(join(parent, name), { recursive: true, force: true });
}

const tmp = mkdtempSync(join(tmpdir(), "aloud-onnxruntime-"));
const staging = join(parent, `.staging-${process.pid}`);
try {
  try {
    await fetchVerified(
      `${MIRROR}/${dist.file}`,
      join(tmp, dist.file),
      dist.size,
      dist.sha256,
      "onnxruntime",
    );
  } catch (e) {
    console.error((e as Error).message);
    process.exit(1);
  }
  mkdirSync(staging, { recursive: true });
  // The archive name is relative to `cwd`; bsdtar (Windows) accepts the absolute `-C` path.
  const x = Bun.spawnSync([tarCommand(), "-xzf", dist.file, "-C", staging], { cwd: tmp });
  if (x.exitCode !== 0) {
    console.error(x.stderr.toString());
    process.exit(1);
  }
  // Swapped in whole: a killed run never leaves a half install that ort-sys would trust.
  renameSync(staging, dest);
  console.log("onnxruntime: installed");
} finally {
  rmSync(tmp, { recursive: true, force: true });
  rmSync(staging, { recursive: true, force: true });
}
