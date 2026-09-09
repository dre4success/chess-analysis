import { mkdir, copyFile } from 'node:fs/promises';
await mkdir('public/engine', { recursive: true });
for (const name of ['stockfish-18-lite-single.js', 'stockfish-18-lite-single.wasm'])
  await copyFile(`node_modules/stockfish/bin/${name}`, `public/engine/${name}`);
await copyFile('node_modules/stockfish/Copying.txt', 'public/engine/COPYING.txt');
