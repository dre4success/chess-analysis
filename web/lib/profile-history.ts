import { normaliseUsername } from './chess.ts';

export const PROFILE_HISTORY_KEY = 'tempo.searched-profiles.v1';
export const PROFILE_HISTORY_LIMIT = 100;
type HistoryStorage = Pick<Storage, 'getItem' | 'setItem'>;

/** Only deliberate lookups populate this list; server reviews never seed it. */
export function parseProfileHistory(raw: string | null): string[] {
  try {
    const values: unknown = JSON.parse(raw ?? '[]');
    if (!Array.isArray(values)) return [];
    const names = new Set<string>();
    for (const value of values) {
      if (typeof value !== 'string') continue;
      try {
        names.add(normaliseUsername(value));
      } catch {
        continue;
      }
      if (names.size === PROFILE_HISTORY_LIMIT) break;
    }
    return [...names];
  } catch {
    return [];
  }
}

export function readProfileHistory(
  storage: HistoryStorage | null,
  fallback: string[] = [],
): string[] {
  if (!storage) return fallback;
  try {
    return parseProfileHistory(storage.getItem(PROFILE_HISTORY_KEY));
  } catch {
    return fallback;
  }
}

export function rememberProfile(
  storage: HistoryStorage | null,
  username: string,
  fallback: string[] = [],
): string[] {
  const name = normaliseUsername(username);
  // Merge the latest persisted list so another tab's searches are retained.
  const previous = [...new Set([...readProfileHistory(storage, fallback), ...fallback])];
  const names = [name, ...previous.filter((value) => value !== name)].slice(
    0,
    PROFILE_HISTORY_LIMIT,
  );
  try {
    storage?.setItem(PROFILE_HISTORY_KEY, JSON.stringify(names));
  } catch {
    // Storage restrictions must not prevent looking up a public chess profile.
  }
  return names;
}
