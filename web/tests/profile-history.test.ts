import assert from 'node:assert/strict';
import { test } from 'node:test';
import { searchedReviews, type ReviewPage } from '../lib/api.ts';
import {
  PROFILE_HISTORY_KEY,
  PROFILE_HISTORY_LIMIT,
  parseProfileHistory,
  readProfileHistory,
  rememberProfile,
} from '../lib/profile-history.ts';

function storage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
}

test('independent browsers only retain the profiles each one explicitly searched', () => {
  const first = storage();
  const second = storage();
  rememberProfile(first, 'First_Player');
  assert.deepEqual(readProfileHistory(first), ['first_player']);
  assert.deepEqual(readProfileHistory(second), []);

  rememberProfile(second, 'Second_Player');
  rememberProfile(first, 'Another_Player');
  assert.deepEqual(readProfileHistory(first), ['another_player', 'first_player']);
  assert.deepEqual(readProfileHistory(second), ['second_player']);
});

test('searched profiles survive a reload, normalize names, and move repeat searches first', () => {
  const browser = storage();
  rememberProfile(browser, '  Dre4Success007  ');
  rememberProfile(browser, 'OTHER-player');
  assert.deepEqual(readProfileHistory(browser), ['other-player', 'dre4success007']);
  const reloaded = readProfileHistory(browser);
  assert.deepEqual(rememberProfile(browser, 'DRE4SUCCESS007', reloaded), [
    'dre4success007',
    'other-player',
  ]);
  assert.deepEqual(readProfileHistory(browser), ['dre4success007', 'other-player']);
});

test('corrupt or non-array browser data produces an empty history', () => {
  for (const raw of [null, '', '{broken', 'null', '{}', '"player"', '42']) {
    assert.deepEqual(parseProfileHistory(raw), [], String(raw));
  }
  const browser = storage();
  assert.deepEqual(readProfileHistory(browser, ['old-session']), []);
  browser.setItem(PROFILE_HISTORY_KEY, '{broken');
  assert.deepEqual(readProfileHistory(browser, ['old-session']), []);
});

test('stored history excludes invalid entries and deduplicates normalized usernames', () => {
  const longestName = 'a'.repeat(64);
  assert.deepEqual(
    parseProfileHistory(
      JSON.stringify([
        ' Dre ',
        null,
        7,
        {},
        true,
        'bad username',
        '',
        'a'.repeat(65),
        'møve',
        'valid_name',
        'DRE',
        'valid-name',
        longestName,
      ]),
    ),
    ['dre', 'valid_name', 'valid-name', longestName],
  );
});

test('history keeps at most 100 distinct profiles and retains a new search at the front', () => {
  assert.equal(PROFILE_HISTORY_LIMIT, 100);
  const names = Array.from({ length: 105 }, (_, index) => `player-${index}`);
  const raw = JSON.stringify([null, names[0], names[0].toUpperCase(), ...names]);
  assert.deepEqual(parseProfileHistory(raw), names.slice(0, 100));

  const browser = storage();
  browser.setItem(PROFILE_HISTORY_KEY, raw);
  const result = rememberProfile(browser, 'Newest_Player');
  assert.deepEqual(result, ['newest_player', ...names.slice(0, 99)]);
  assert.deepEqual(readProfileHistory(browser), result);
});

test('searches merge the latest saved profiles when another tab updated the history', () => {
  const browser = storage();
  const firstTab = rememberProfile(browser, 'first-player');
  rememberProfile(browser, 'other-tab');
  const result = rememberProfile(browser, 'new-search', firstTab);
  assert.equal(result[0], 'new-search');
  assert.deepEqual(new Set(result), new Set(['new-search', 'other-tab', 'first-player']));
  assert.deepEqual(readProfileHistory(browser), result);
});

test('unavailable or unreadable storage preserves searches for the current session', () => {
  const unreadable = {
    getItem: () => {
      throw new Error('Storage is blocked');
    },
    setItem: () => {
      throw new Error('Storage is blocked');
    },
  };
  for (const browser of [null, unreadable]) {
    const prior = ['previous-player'];
    assert.deepEqual(readProfileHistory(browser, prior), prior);
    const first = rememberProfile(browser, 'New_Player', prior);
    const second = rememberProfile(browser, 'Next_Player', first);
    assert.deepEqual(second, ['next_player', 'new_player', 'previous-player']);
    assert.deepEqual(prior, ['previous-player']);
  }
});

test('consecutive failed writes retain earlier session searches as well as persisted profiles', () => {
  const browser = {
    getItem: () => JSON.stringify(['persisted-player']),
    setItem: () => {
      throw new Error('Storage quota exceeded');
    },
  };
  const first = rememberProfile(browser, 'first-search');
  const second = rememberProfile(browser, 'second-search', first);
  assert.equal(second[0], 'second-search');
  assert.deepEqual(
    new Set(second),
    new Set(['second-search', 'first-search', 'persisted-player']),
  );
  assert.deepEqual(readProfileHistory(browser), ['persisted-player']);
});

test('invalid explicit usernames cannot overwrite stored history', () => {
  const browser = storage();
  rememberProfile(browser, 'saved-player');
  assert.throws(() => rememberProfile(browser, 'not a username'), /username/);
  assert.deepEqual(readProfileHistory(browser), ['saved-player']);
});

test('an empty browser history returns an empty page without fetching shared history', async (t) => {
  const fetch = t.mock.method(globalThis, 'fetch', async () => {
    throw new Error('A fresh browser must not request the shared history');
  });
  assert.deepEqual(await searchedReviews([]), {
    reviews: [],
    total: 0,
    has_more: false,
    offset: 0,
    limit: 20,
  });
  assert.deepEqual(await searchedReviews([], { offset: 20, limit: 4 }), {
    reviews: [],
    total: 0,
    has_more: false,
    offset: 20,
    limit: 4,
  });
  assert.equal(fetch.mock.callCount(), 0);
});

test('history requests carry only the searched profiles and preserve pagination and cancellation', async (t) => {
  const page: ReviewPage = {
    reviews: [],
    total: 24,
    has_more: true,
    offset: 8,
    limit: 4,
  };
  const requests: { url: URL; options: RequestInit }[] = [];
  t.mock.method(globalThis, 'fetch', async (path: string, options: RequestInit) => {
    requests.push({ url: new URL(path, 'http://tempo.test'), options });
    return Response.json(page);
  });
  const controller = new AbortController();
  assert.deepEqual(
    await searchedReviews(['dre4success007', 'other-player'], {
      offset: 8,
      limit: 4,
      signal: controller.signal,
    }),
    page,
  );
  assert.equal(requests.length, 1);
  const { url, options } = requests[0];
  assert.equal(url.pathname, '/api/reviews');
  assert.deepEqual(Object.fromEntries(url.searchParams), {
    usernames: 'dre4success007,other-player',
    offset: '8',
    limit: '4',
  });
  assert.equal(options.method, undefined);
  assert.equal(options.signal, controller.signal);
});
