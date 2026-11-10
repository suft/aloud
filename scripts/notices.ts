// Generates src-tauri/resources/THIRD-PARTY-NOTICES.txt: hand-written native components,
// the Rust crates (cargo-about) and the JavaScript packages bundled into the UI.
// Usage: bun scripts/notices.ts   (needs cargo-about; `mise install` provides it)
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const OUT = join(ROOT, "src-tauri/resources/THIRD-PARTY-NOTICES.txt");
const RUST_TXT = join(ROOT, "target/notices-rust.txt");
const RULE = "-".repeat(80);

mkdirSync(dirname(RUST_TXT), { recursive: true });
const about = Bun.spawnSync(
  [
    "cargo",
    "about",
    "generate",
    "--locked",
    "-m",
    "src-tauri/Cargo.toml",
    "-o",
    RUST_TXT,
    "about.hbs",
  ],
  { cwd: ROOT, stdout: "inherit", stderr: "inherit" },
);
if (about.exitCode !== 0) throw new Error("cargo about failed");
const rust = readFileSync(RUST_TXT, "utf8");

interface Pkg {
  name: string;
  version?: string;
  license?: string | { type?: string };
  licenses?: { type?: string }[];
  dependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
}

const readPkg = (dir: string): Pkg => JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));

function resolveDep(name: string, parentDir: string): string | null {
  for (const base of [parentDir, ROOT]) {
    const dir = join(base, "node_modules", name);
    if (existsSync(join(dir, "package.json"))) return dir;
  }
  return null;
}

/** Every installed package reachable from the app's `dependencies`, by resolved directory. */
function reachablePackages(): Map<string, Pkg> {
  const found = new Map<string, Pkg>();
  const queue: [string, string][] = Object.keys(readPkg(ROOT).dependencies ?? {}).map((n) => [
    n,
    ROOT,
  ]);
  while (queue.length > 0) {
    const [name, parent] = queue.shift()!;
    const dir = resolveDep(name, parent);
    if (!dir || found.has(dir)) continue;
    const pkg = readPkg(dir);
    found.set(dir, pkg);
    for (const dep of [
      ...Object.keys(pkg.dependencies ?? {}),
      ...Object.keys(pkg.peerDependencies ?? {}),
    ]) {
      queue.push([dep, dir]);
    }
  }
  return found;
}

const LICENSE_FILE = /^(licen[cs]e|copying|notice)(\.(md|txt))?$/i;

function licenseName(pkg: Pkg): string {
  if (typeof pkg.license === "string") return pkg.license;
  if (pkg.license?.type) return pkg.license.type;
  return (
    pkg.licenses
      ?.map((l) => l.type)
      .filter(Boolean)
      .join(" OR ") ?? ""
  );
}

function js(): string {
  const sections = [...reachablePackages()]
    .sort(([, a], [, b]) => a.name.localeCompare(b.name))
    .map(([dir, pkg]) => {
      const texts = readdirSync(dir)
        .filter((f) => LICENSE_FILE.test(f) && statSync(join(dir, f)).isFile())
        .sort()
        .map((f) => readFileSync(join(dir, f), "utf8").trim());
      const body = texts.length > 0 ? texts.join("\n\n") : "No licence file in the package.";
      const name =
        licenseName(pkg) || (texts.length > 0 ? "see the licence text" : "licence not stated");
      return `${pkg.name} ${pkg.version ?? ""} — ${name}\n\n${body}`;
    });
  return sections.join(`\n\n${RULE}\n`);
}

const MIT = `Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.`;

const header = `Aloud — third-party notices

Aloud is free software, licensed under the GNU General Public License, version 3 or (at your
option) any later version. The full text is in LICENSE.txt, next to this file. The source code
is at https://github.com/suft/aloud

Aloud contains or uses the components below.`;

const native = `Native components

espeak-ng — GPL-3.0-or-later
  https://github.com/espeak-ng/espeak-ng
  Compiled into Aloud through espeak-rs-sys, together with its English data.

ONNX Runtime — MIT
  https://github.com/microsoft/onnxruntime
  Compiled in through the ort crate.
  Copyright (c) Microsoft Corporation

${MIT}

  ONNX Runtime's own third-party notices:
  https://github.com/microsoft/onnxruntime/blob/main/ThirdPartyNotices.txt

Kokoro-82M — Apache-2.0
  https://huggingface.co/hexgrad/Kokoro-82M
  Not part of the installer. The voice model is downloaded on first launch from
  https://github.com/thewh1teagle/kokoro-onnx

rusty_aac — Apache-2.0
  Aloud's AAC encoder. Its authors state that it was written from ISO/IEC 14496-3; FFmpeg
  appears only in comments and in its test harness. No patent rights are granted.`;

const text = [
  header,
  native,
  `Rust crates\n\n${rust.trim()}`,
  `JavaScript packages (user interface)\n\n${js()}`,
].join(`\n\n${RULE}\n\n`);

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, `${text}\n`);
console.log(`${OUT}: ${Math.round(text.length / 1024)} KiB`);
