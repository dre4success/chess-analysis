import { spawnSync } from 'node:child_process';
import { mkdir, copyFile, readFile, writeFile, access, mkdtemp } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(webRoot, 'public/rust');
await mkdir(output, { recursive: true });
let repo = path.resolve(webRoot, '..');
function run(command, args, cwd) {
  // macOS tar otherwise adds AppleDouble files containing local file metadata.
  const env = command === 'tar' ? { ...process.env, COPYFILE_DISABLE: '1' } : process.env;
  const result = spawnSync(command, args, { cwd, stdio: 'inherit', env });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
try {
  await access(path.join(repo, 'src/browser.rs'));
} catch {
  repo = await mkdtemp(path.join(tmpdir(), 'tempo-rust-source-'));
  run(
    'tar',
    ['-xzf', path.join(output, 'chess-review-source.tar.gz'), '-C', repo],
    webRoot,
  );
}
run(
  'cargo',
  ['build', '--locked', '--lib', '--release', '--target', 'wasm32-unknown-unknown'],
  repo,
);
const source = path.join(repo, 'target/wasm32-unknown-unknown/release/chess_review.wasm');
await copyFile(source, path.join(output, 'chess_review.wasm'));
run(
  'tar',
  [
    '-czf',
    path.join(output, 'chess-review-source.tar.gz'),
    'Cargo.toml',
    'Cargo.lock',
    'src',
    'tests',
  ],
  repo,
);
const digest = async (name) =>
  createHash('sha256')
    .update(await readFile(path.join(output, name)))
    .digest('hex');
const manifest = {
  engine: 'chess-review',
  scanNodes: 150000,
  confirmationNodes: 1000000,
  thresholdCp: 200,
  sha256: await digest('chess_review.wasm'),
  sourceSha256: await digest('chess-review-source.tar.gz'),
};
await writeFile(
  path.join(output, 'manifest.json'),
  JSON.stringify(manifest, null, 2) + '\n',
);
