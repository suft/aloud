import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { ensureAlice, fetchVerified } from "./lib.ts";

const dir = resolve(process.argv[2] ?? ".dev-data/models");
const BASE = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0";
const FILES: [string, number, string][] = [
  ["voices-v1.0.bin", 28214398, "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"],
  [
    "kokoro-v1.0.int8.onnx",
    92361271,
    "6e742170d309016e5891a994e1ce1559c702a2ccd0075e67ef7157974f6406cb",
  ],
  [
    "kokoro-v1.0.onnx",
    325532387,
    "7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5",
  ],
];

mkdirSync(dir, { recursive: true });
for (const [name, size, sha] of FILES) {
  await fetchVerified(`${BASE}/${name}`, join(dir, name), size, sha, name);
}

// Dev e2e fixture: Alice in Wonderland (public domain).
await ensureAlice(join(dirname(dir), "alice.epub"));
