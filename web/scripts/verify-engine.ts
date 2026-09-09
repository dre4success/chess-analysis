// End-to-end numerical parity: compiled Rust + browser Stockfish versus the
// native Rust CLI + the exact same Stockfish binary and node budgets.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, writeFile, mkdtemp, chmod } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { parseGame, type ApiGame } from '../lib/chess.ts';
import { BrowserEngine, analyseGame } from '../lib/engine.ts';
import { convertFinding, type SavedReview } from '../lib/rust-core.ts';
const root = process.cwd();
class NodeStockfish {
  onmessage: ((e: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  child = spawn(process.execPath, [
    path.join(root, 'node_modules/stockfish/bin/stockfish-18-lite-single.js'),
  ]);
  constructor() {
    let pending = '';
    this.child.stdout.on('data', (data) => {
      pending += data;
      const lines = pending.split('\n');
      pending = lines.pop()!;
      for (const line of lines) this.onmessage?.({ data: line.trim() });
    });
    this.child.on('error', () => this.onerror?.());
  }
  postMessage(value: string) {
    this.child.stdin.write(`${value}\n`);
  }
  terminate() {
    this.child.kill();
  }
}
Object.assign(globalThis, { Worker: NodeStockfish });
const nativeFetch = globalThis.fetch;
globalThis.fetch = ((
  input: Parameters<typeof fetch>[0],
  init?: Parameters<typeof fetch>[1],
) =>
  nativeFetch(
    typeof input === 'string' && input.startsWith('/')
      ? `http://localhost:3000${input}`
      : input,
    init,
  )) as typeof fetch;
const archive = JSON.parse(await readFile('public/example/games.json', 'utf8')) as {
  games: ApiGame[];
};
const games = archive.games
  .map((g) => parseGame(g, 'dre4success007')!)
  .sort((a, b) => a.moves.length - b.moves.length);
const saved = JSON.parse(
  await readFile('public/example/review.json', 'utf8'),
) as SavedReview;
const game = games.find(
  (g) => saved.games.find((r) => r.url === g.id)!.findings.length > 0,
)!;
const controller = new AbortController(),
  engine = new BrowserEngine(controller.signal);
const dir = await mkdtemp(path.join(tmpdir(), 'tempo-parity-'));
await writeFile(path.join(dir, 'game.pgn'), game.pgn);
const quote = (s: string) => `'${s.replaceAll("'", "'\\''")}'`;
const shim = path.join(dir, 'stockfish');
await writeFile(
  shim,
  `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(path.join(root, 'node_modules/stockfish/bin/stockfish-18-lite-single.js'))}\n`,
);
await chmod(shim, 0o700);
console.log(
  `Comparing Rust WASM and native CLI: ${game.opponent}, ${game.moves.length} plies.`,
);
const native = spawn(
  'cargo',
  [
    'run',
    '--quiet',
    '--',
    'review',
    'dre4success007',
    '--pgn',
    path.join(dir, 'game.pgn'),
    '--engine',
    shim,
    '--last',
    '1',
    '--output',
    path.join(dir, 'native'),
  ],
  { cwd: path.dirname(root), stdio: ['ignore', 'pipe', 'pipe'] },
);
let nativeLog = '';
native.stdout.on('data', (d) => (nativeLog += d));
native.stderr.on('data', (d) => (nativeLog += d));
const nativeDone = new Promise<void>((resolve, reject) => {
  native.on('error', reject);
  native.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(nativeLog))));
});
try {
  await engine.ready();
  const result = await analyseGame(
    game,
    engine,
    (done, total) => {
      if (done === total) console.log(`Completed ${total} position searches.`);
    },
    controller.signal,
  );
  await nativeDone;
  const expected = JSON.parse(
    await readFile(path.join(dir, 'native/review.json'), 'utf8'),
  ) as SavedReview;
  assert.deepEqual(result.findings, expected.games[0].findings.map(convertFinding));
  console.log(
    `PASS: identical ${result.findings.length} findings, moves, evaluations, labels and principal variations.`,
  );
} finally {
  engine.close();
  native.kill();
}
