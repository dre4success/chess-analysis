import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadModule } from '../lib/load-module.ts';

function browser() {
  const values = new Map<string, string>();
  let reloads = 0;
  return {
    sessionStorage: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => {
        values.set(key, value);
      },
      removeItem: (key: string) => {
        values.delete(key);
      },
    },
    location: {
      reload: () => {
        reloads++;
      },
    },
    navigator: { onLine: true },
    reloads: () => reloads,
  };
}

const missingChunk = new TypeError(
  'Failed to fetch dynamically imported module: /assets/old.js',
);
const fail = async () => {
  throw missingChunk;
};

test('a stale lazy chunk reloads the current URL once, without a reload loop', async () => {
  const page = browser();
  await assert.rejects(loadModule(fail, page), missingChunk);
  assert.equal(page.reloads(), 1);
  // sessionStorage survives navigation; a still-broken build must reach the boundary.
  await assert.rejects(loadModule(fail, page), missingChunk);
  assert.equal(page.reloads(), 1);
});

test('a successful import returns the module and enables recovery on a later update', async () => {
  const page = browser();
  await assert.rejects(loadModule(fail, page), missingChunk);
  const module = { default: 'chart' };
  assert.equal(await loadModule(async () => module, page), module);
  await assert.rejects(loadModule(fail, page), missingChunk);
  assert.equal(page.reloads(), 2);
});

test('offline failures stay in the dashboard without reloading', async () => {
  const page = browser();
  page.navigator.onLine = false;
  await assert.rejects(loadModule(fail, page), missingChunk);
  assert.equal(page.reloads(), 0);
});

test('blocked storage never causes an unguarded reload or hides a loaded module', async () => {
  const page = browser();
  Object.defineProperty(page, 'sessionStorage', {
    get() {
      throw new Error('Storage disabled');
    },
  });
  await assert.rejects(loadModule(fail, page), missingChunk);
  assert.equal(page.reloads(), 0);
  assert.equal(await loadModule(async () => 'loaded', page), 'loaded');
});

test('module execution errors are surfaced without automatic reload', async () => {
  const page = browser();
  const error = new Error('Chart render failed');
  await assert.rejects(
    loadModule(async () => {
      throw error;
    }, page),
    error,
  );
  assert.equal(page.reloads(), 0);
});
