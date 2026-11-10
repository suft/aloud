// Prepares a tiny end-to-end render: the fast model, the voices, and a generated EPUB.
// Usage: bun scripts/smoke.ts <dir>   (then run the `aloud` example with `--data <dir>`)
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { zipSync, strToU8 } from "fflate";
import { fetchVerified, KOKORO_BASE, KOKORO_FILES } from "./lib.ts";

const dir = resolve(process.argv[2] ?? ".smoke-data");
mkdirSync(join(dir, "models"), { recursive: true });
for (const key of ["voices", "fast"] as const) {
  const [name, size, sha] = KOKORO_FILES[key];
  await fetchVerified(`${KOKORO_BASE}/${name}`, join(dir, "models", name), size, sha, name);
}

const xhtml = (body: string) =>
  `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>x</title></head><body>${body}</body></html>`;

const opf = `<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title>Aloud Smoke Test</dc:title><dc:creator>Aloud</dc:creator><dc:language>en</dc:language>
 </metadata>
 <manifest>
  <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
 </manifest>
 <spine><itemref idref="ch1"/></spine>
</package>`;

const epub = zipSync({
  // The EPUB container requires `mimetype` to come first and be stored uncompressed.
  mimetype: [strToU8("application/epub+zip"), { level: 0 }],
  "META-INF/container.xml": strToU8(
    `<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>`,
  ),
  "OEBPS/content.opf": strToU8(opf),
  "OEBPS/nav.xhtml": strToU8(
    xhtml(`<nav epub:type="toc"><ol><li><a href="ch1.xhtml">Chapter One</a></li></ol></nav>`),
  ),
  "OEBPS/ch1.xhtml": strToU8(
    xhtml(
      "<h1>Chapter One</h1><p>Aloud turns books into audiobooks.</p><p>This short chapter checks the whole pipeline.</p>",
    ),
  ),
});
writeFileSync(join(dir, "smoke.epub"), epub);
console.log(`smoke.epub: ok`);
