// Alice in Wonderland fixture for the README screenshots. Chapter ids, titles and word counts
// come from a real import of the Project Gutenberg EPUB; the render state is staged.
import pkg from "../../package.json";
import type {
  AppStateDto,
  PlayerState,
  PronEntry,
  ProjectDetail,
  ProjectSummary,
  RenderProgress,
  Settings,
  TrickyWord,
  VoiceInfo,
} from "../../src/lib/api.ts";

export type Scenario = "rendering" | "draft";

export const ALICE_ID = "alice";
export const COVER_PATH = "/alice/cover.jpg";

const ACCENT = "#9f5738";
const TITLE = "Alice's Adventures in Wonderland";

/** `[id, title, words]` */
const CHAPTERS: [string, string, number][] = [
  ["pg-header", "Alice's Adventures in Wonderland", 216],
  ["item4", "CHAPTER I. Down the Rabbit-Hole", 2146],
  ["item5", "CHAPTER II. The Pool of Tears", 2099],
  ["item6", "CHAPTER III. A Caucus-Race and a Long Tale", 1702],
  ["item7", "CHAPTER IV. The Rabbit Sends in a Little Bill", 2616],
  ["item8", "CHAPTER V. Advice from a Caterpillar", 2166],
  ["item9", "CHAPTER VI. Pig and Pepper", 2593],
  ["item10", "CHAPTER VII. A Mad Tea-Party", 2287],
  ["item11", "CHAPTER VIII. The Queen's Croquet-Ground", 2487],
  ["item12", "CHAPTER IX. The Mock Turtle's Story", 2272],
  ["item13", "CHAPTER X. The Lobster Quadrille", 2030],
  ["item14", "CHAPTER XI. Who Stole the Tarts?", 1878],
  ["item15", "CHAPTER XII. Alice's Evidence", 2105],
  ["pg-footer", "THE FULL PROJECT GUTENBERG™ LICENSE", 2907],
];

const EXCLUDED = new Set(["pg-header", "pg-footer"]);

/** Rendered fraction per chapter in the `rendering` scenario. */
const RENDERED: Record<string, number> = { item4: 1, item5: 1, item6: 1, item7: 0.4 };

const rendered = (s: Scenario, id: string) => (s === "rendering" ? (RENDERED[id] ?? 0) : 0);

/** Mirrors `est_secs` / `WORDS_PER_MINUTE` in `src-tauri/src/commands.rs`. */
const estSecs = (words: number, speed: number) => Math.round(((words / 175) * 60) / speed);

/** Verbatim `CATALOG` from `crates/aloud-core/src/voices.rs`. */
export const VOICES: VoiceInfo[] = [
  ["af_heart", "Heart", "us", "female", "A", true],
  ["af_bella", "Bella", "us", "female", "A-", true],
  ["af_nicole", "Nicole", "us", "female", "B-", true],
  ["af_aoede", "Aoede", "us", "female", "C+", false],
  ["af_kore", "Kore", "us", "female", "C+", false],
  ["af_sarah", "Sarah", "us", "female", "C+", false],
  ["af_alloy", "Alloy", "us", "female", "C", false],
  ["af_nova", "Nova", "us", "female", "C", false],
  ["af_sky", "Sky", "us", "female", "C-", false],
  ["af_jessica", "Jessica", "us", "female", "D", false],
  ["af_river", "River", "us", "female", "D", false],
  ["am_fenrir", "Fenrir", "us", "male", "C+", false],
  ["am_michael", "Michael", "us", "male", "C+", false],
  ["am_puck", "Puck", "us", "male", "C+", false],
  ["am_echo", "Echo", "us", "male", "D", false],
  ["am_eric", "Eric", "us", "male", "D", false],
  ["am_liam", "Liam", "us", "male", "D", false],
  ["am_onyx", "Onyx", "us", "male", "D", false],
  ["am_santa", "Santa", "us", "male", "D-", false],
  ["am_adam", "Adam", "us", "male", "F+", false],
  ["bf_emma", "Emma", "uk", "female", "B-", true],
  ["bf_isabella", "Isabella", "uk", "female", "C", false],
  ["bm_fable", "Fable", "uk", "male", "C", false],
  ["bm_george", "George", "uk", "male", "C", false],
  ["bm_lewis", "Lewis", "uk", "male", "D+", false],
  ["bf_alice", "Alice", "uk", "female", "D", false],
  ["bf_lily", "Lily", "uk", "female", "D", false],
  ["bm_daniel", "Daniel", "uk", "male", "D", false],
].map(([id, name, accent, gender, grade, recommended]) => ({
  id,
  name,
  accent,
  gender,
  grade,
  recommended,
})) as VoiceInfo[];

const pron = (id: string, word: string, say: string): PronEntry => ({
  id,
  word,
  sayAs: { kind: "text", value: say },
  matchCase: false,
});

const BOOK_PRONS: PronEntry[] = [
  pron("p-dinah", "Dinah", "Dye-nuh"),
  pron("p-hjckrrh", "Hjckrrh", "hick-err"),
];
const GLOBAL_PRONS: PronEntry[] = [pron("p-gryphon", "Gryphon", "Griffin")];

/** The real `plan.oov` of Alice, minus the fixed and dismissed words. */
export const TRICKY: TrickyWord[] = [
  ["Oop", 7, "Beau—ootiful Soo—oop!"],
  ["Soo", 7, "Beau—ootiful Soo—oop!"],
  ["jurymen", 4, "upsetting all the jurymen on to the heads of the crowd below"],
  ["ootiful", 4, "Beau—ootiful Soo—oop!"],
  ["Curiouser", 2, '"Curiouser and curiouser!" cried Alice'],
  ["Edwin", 2, "Edwin and Morcar, the earls of Mercia and Northumbria"],
  ["Morcar", 2, "Edwin and Morcar, the earls of Mercia and Northumbria"],
  ["camomile", 1, "and camomile that makes them bitter"],
  ["Elsie", 1, "their names were Elsie, Lacie, and Tillie"],
  ["Lacie", 1, "their names were Elsie, Lacie, and Tillie"],
  ["Seaography", 1, "Mystery, ancient and modern, with Seaography"],
  ["Stigand", 1, "Edwin and Morcar, the earls of Mercia and Northumbria"],
  ["Tillie", 1, "their names were Elsie, Lacie, and Tillie"],
].map(([word, count, sample]) => ({ word, count, sample })) as TrickyWord[];

export const SETTINGS: Settings = {
  model: "standard",
  outputDir: "/home/alice/Audiobooks",
  performance: "balanced",
  gpu: false,
  keepAwake: true,
  notifyOnFinish: true,
  checkUpdates: true,
  defaultVoice: "bf_emma",
  defaultSpeed: 1,
  pronunciations: GLOBAL_PRONS,
  theme: "system",
};

export function renderProgress(): RenderProgress {
  const chapters = CHAPTERS.filter(([id]) => !EXCLUDED.has(id)).map(([id, , words]) => ({
    id,
    doneUnits: Math.round(words * (RENDERED[id] ?? 0)),
    totalUnits: words,
  }));
  const doneUnits = chapters.reduce((n, c) => n + c.doneUnits, 0);
  const totalUnits = chapters.reduce((n, c) => n + c.totalUnits, 0);
  return {
    projectId: ALICE_ID,
    phase: "rendering",
    doneUnits,
    totalUnits,
    fraction: doneUnits / totalUnits,
    chapters,
    etaSecs: 3720,
    realtimeFactor: 4.6,
  };
}

const status = (s: Scenario) =>
  s === "rendering" ? ({ state: "rendering" } as const) : ({ state: "draft" } as const);

export function projectSummary(s: Scenario): ProjectSummary {
  return {
    id: ALICE_ID,
    title: TITLE,
    authors: ["Lewis Carroll"],
    coverPath: COVER_PATH,
    accent: ACCENT,
    status: status(s),
    progress: s === "rendering" ? renderProgress().fraction : 0,
    updatedAt: 1794278800,
    damaged: false,
  };
}

export function projectDetail(s: Scenario): ProjectDetail {
  const speed = 1;
  const chapters = CHAPTERS.map(([id, title, words]) => {
    const fraction = rendered(s, id);
    return {
      id,
      title,
      kind: "body" as const,
      included: !EXCLUDED.has(id),
      words,
      estSecs: estSecs(words, speed),
      renderedFraction: fraction,
      playable: fraction === 1,
    };
  });
  const included = chapters.filter((c) => c.included);
  return {
    project: {
      id: ALICE_ID,
      sourceFileName: "alice.epub",
      createdAt: 1794277674,
      updatedAt: 1794278800,
      meta: {
        title: TITLE,
        authors: ["Lewis Carroll"],
        language: "en",
        publisher: null,
        date: "2008-06-27",
        description: null,
        coverFile: "cover.jpg",
        accent: ACCENT,
      },
      chapters: chapters.map(({ id, title, included, words, kind }) => ({
        id,
        title,
        included,
        words,
        kind,
      })),
      voice: "bf_emma",
      speed,
      intro: true,
      pronunciations: BOOK_PRONS,
      dismissedWords: ["gutenberg"],
      status: status(s),
    },
    coverPath: COVER_PATH,
    wordCount: included.reduce((n, c) => n + c.words, 0),
    estimatedSecs: included.reduce((n, c) => n + c.estSecs, 0),
    chapters,
    stale: false,
    redoFraction: 1,
    outputFileName: `${TITLE}.m4b`,
  };
}

export function appState(s: Scenario): AppStateDto {
  return {
    settings: SETTINGS,
    models: { standard: true, fast: false, voices: true },
    queue: { items: s === "rendering" ? [ALICE_ID] : [], paused: false },
    active: s === "rendering" ? renderProgress() : null,
    gpuSupported: true,
    speedRange: { min: 0.5, max: 2, uiMin: 0.8, uiMax: 1.3 },
    version: pkg.version,
    startupNotices: [],
    sampleText:
      "Once upon a time, in a quiet town by the sea, there lived a girl who loved stories more than anything in the world.",
  };
}

export const PLAYER_STATE: PlayerState = {
  projectId: ALICE_ID,
  chapterId: "item4",
  title: "CHAPTER I. Down the Rabbit-Hole",
  positionMs: 263_000,
  durationMs: 736_000,
  playing: true,
};
