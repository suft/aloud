// Renders the README screenshots: the built frontend runs in headless Chromium with the Tauri
// backend replaced by ./mock.ts. Usage: bun scripts/screenshots/capture.ts
import { mkdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { unzipSync } from "fflate";
import { chromium, type Page } from "playwright";
import { ensureAlice } from "../lib.ts";
import { PLAYER_STATE, type Scenario } from "./alice.ts";

const ROOT = resolve(import.meta.dir, "../..");
const EPUB = join(ROOT, ".dev-data/alice.epub");
const WEB = join(ROOT, ".dev-data/screenshots/web");
const OUT = join(ROOT, "docs/screenshots");

interface Shot {
  name: string;
  scenario: Scenario;
  height: number;
  prepare: (page: Page) => Promise<void>;
}

async function openBook(page: Page) {
  await page.locator("main button", { hasText: "Lewis Carroll" }).click();
  await page.getByRole("tab", { name: "Chapters" }).waitFor();
}

const SHOTS: Shot[] = [
  {
    name: "library",
    scenario: "rendering",
    height: 760,
    prepare: async (page) => {
      await page.getByRole("heading", { name: "Your books" }).waitFor();
      await page.getByText("Making “Alice", { exact: false }).waitFor();
    },
  },
  {
    name: "book",
    scenario: "rendering",
    height: 1080,
    prepare: async (page) => {
      await openBook(page);
      await page.evaluate(
        (s) =>
          (
            window as unknown as { __aloudMock: { emit(e: string, p: unknown): void } }
          ).__aloudMock.emit("player-state", s),
        PLAYER_STATE,
      );
      await page.getByRole("button", { name: "Close player" }).waitFor();
    },
  },
  {
    name: "voices",
    scenario: "draft",
    height: 760,
    prepare: async (page) => {
      await openBook(page);
      await page.getByRole("button", { name: "Change voice" }).click();
      await page.getByRole("dialog").filter({ hasText: "Choose a voice" }).waitFor();
      await page.waitForTimeout(500); // sheet slide-in animation
    },
  },
  {
    name: "pronunciations",
    scenario: "draft",
    height: 880, // tall enough for the Gryphon row under "Your fixes"
    prepare: async (page) => {
      await openBook(page);
      await page.getByRole("tab", { name: "Pronunciations" }).click();
      await page.getByText("Seaography").waitFor();
      await page.getByRole("tablist").evaluate((el) => {
        const main = el.closest("main");
        if (!main) throw new Error("tablist is not inside <main>");
        main.scrollTop += el.getBoundingClientRect().top - main.getBoundingClientRect().top - 24;
      });
    },
  },
  {
    name: "settings",
    scenario: "draft",
    height: 760,
    prepare: async (page) => {
      await page.getByRole("button", { name: "Settings" }).click();
      await page.getByRole("heading", { name: "Settings" }).waitFor();
      await page.getByText("412 MB").waitFor();
    },
  },
];

await ensureAlice(EPUB);

const cover = Object.entries(
  unzipSync(readFileSync(EPUB), { filter: (f) => /cover\.(jpe?g|png)$/i.test(f.name) }),
)[0];
if (!cover) throw new Error("alice.epub: no cover image found");
const [coverName, coverBytes] = cover;
const coverType = coverName.toLowerCase().endsWith(".png") ? "image/png" : "image/jpeg";

const build = Bun.spawnSync(
  ["bun", "x", "vite", "build", "--outDir", WEB, "--emptyOutDir", "--logLevel", "warn"],
  { cwd: ROOT, stdout: "inherit", stderr: "inherit" },
);
if (build.exitCode !== 0) throw new Error("vite build failed");

const bundle = await Bun.build({
  entrypoints: [join(import.meta.dir, "mock.ts")],
  target: "browser",
  format: "iife",
});
if (!bundle.success) throw new Error(bundle.logs.join("\n"));
const mockSource = await bundle.outputs[0].text();

const server = Bun.serve({
  port: 0,
  async fetch(req) {
    const { pathname } = new URL(req.url);
    if (pathname === "/__mock/cover") {
      return new Response(coverBytes, { headers: { "content-type": coverType } });
    }
    const file = Bun.file(join(WEB, pathname === "/" ? "index.html" : pathname));
    return (await file.exists()) ? new Response(file) : new Response("not found", { status: 404 });
  },
});
const base = `http://localhost:${server.port}`;

const browser = await chromium.launch();
mkdirSync(OUT, { recursive: true });

const failures: string[] = [];
const written: string[] = [];

for (const theme of ["light", "dark"] as const) {
  for (const shot of SHOTS) {
    const context = await browser.newContext({
      viewport: { width: 1120, height: shot.height },
      deviceScaleFactor: 2,
      colorScheme: theme,
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    await page.addInitScript({ content: mockSource });

    const tag = `${shot.name}-${theme}`;
    page.on("pageerror", (e) => failures.push(`${tag}: ${e.message}`));
    page.on("console", (m) => {
      if (m.text().startsWith("[mock]")) failures.push(`${tag}: ${m.text()}`);
      else if (m.type() === "error") console.warn(`${tag}: console error: ${m.text()}`);
    });

    try {
      await page.goto(`${base}/?scenario=${shot.scenario}`);
      await shot.prepare(page);

      await page.addStyleTag({
        content: "*{scrollbar-width:none!important;caret-color:transparent!important}",
      });
      await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
      await page.mouse.move(0, 0);
      await page.evaluate(() => document.fonts.ready);
      await page.waitForFunction(() =>
        [...document.images].every((i) => i.complete && i.naturalWidth > 0),
      );
      await page.waitForTimeout(300);

      const path = join(OUT, `${tag}.png`);
      await page.screenshot({ path });
      written.push(path);
    } catch (e) {
      failures.push(`${tag}: ${e instanceof Error ? e.message : String(e)}`);
    }
    await context.close();
  }
}

await browser.close();
await server.stop();

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}
for (const path of written) console.log(path);
