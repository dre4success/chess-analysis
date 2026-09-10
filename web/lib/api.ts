import type { Pace, PlayerData, GameAnalysis, Game } from './chess.ts';
import { convertFinding, type SavedReview } from './review-data.ts';

export type ReviewSummary = {
  id: number;
  username: string;
  pace: Pace;
  status: 'queued' | 'running' | 'complete' | 'failed' | 'cancelled';
  created_at: string;
  imported_games?: number | null;
  reviewed_games?: number | null;
  first_game_at?: number | null;
  last_game_at?: number | null;
  findings?: number | null;
  progress: { completed: number; total: number; positions: number };
};
export type ReviewPage = {
  reviews: ReviewSummary[];
  offset: number;
  limit: number;
  has_more: boolean;
  total: number;
};
export type ReviewJob = ReviewSummary & {
  message: string;
  error?: string;
  data?: PlayerData;
  review?: SavedReview;
  /** Empty for an initial latest-five review; explicit for a continued batch. */
  target_urls?: string[];
};
export type ReviewRequest = {
  username: string;
  pace: Pace;
  parent_id?: number;
  game_urls?: string[];
};
export async function api<T>(
  path: string,
  signal?: AbortSignal,
  body?: unknown,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    signal,
    ...(body === undefined
      ? {}
      : {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        }),
  });
  let payload;
  try {
    payload = await response.json();
  } catch {
    throw new Error(
      'The review server could not be reached. Your saved reviews will be here when it reconnects.',
    );
  }
  if (!response.ok) {
    const message =
      payload && typeof payload === 'object' && 'error' in payload
        ? payload.error
        : undefined;
    throw new Error(
      typeof message === 'string'
        ? message
        : 'The server could not complete this request.',
    );
  }
  return payload as T;
}

/** Empty browser history must never request the instance's entire collection. */
export async function searchedReviews(
  usernames: string[],
  {
    offset = 0,
    limit = 20,
    signal,
  }: { offset?: number; limit?: number; signal?: AbortSignal } = {},
): Promise<ReviewPage> {
  if (!usernames.length) return { reviews: [], total: 0, has_more: false, offset, limit };
  const query = new URLSearchParams({
    usernames: usernames.join(','),
    offset: String(offset),
    limit: String(limit),
  });
  return api<ReviewPage>(`/reviews?${query}`, signal);
}
export function mappedAnalyses(job: ReviewJob): Record<string, GameAnalysis> {
  return Object.fromEntries(
    (job.review?.games ?? []).map((game) => [
      game.url,
      {
        id: game.url,
        source: 'native' as const,
        positions: 0,
        findings: game.findings.map(convertFinding),
      },
    ]),
  );
}
/** Continue only unfinished games in this job's scope, including after repeated stops. */
export function remainingReviewGames(
  job: ReviewJob | null,
  games: Game[],
  analyses: Record<string, GameAnalysis>,
): Game[] {
  if (!job) return [];
  const targetUrls = job.target_urls?.length
    ? job.target_urls
    : job.data
      ? job.data.games.slice(0, 5).map((g) => g.url)
      : games.slice(0, 5).map((g) => g.id);
  const targets = new Set(targetUrls);
  return games.filter((g) => targets.has(g.id) && !analyses[g.id]);
}
export function running(job: ReviewSummary) {
  return job.status === 'queued' || job.status === 'running';
}
export async function* watchReview(id: number, signal: AbortSignal) {
  while (!signal.aborted) {
    const job = await api<ReviewJob>(`/reviews/${id}`, signal);
    if (signal.aborted) return;
    yield job;
    if (!running(job)) return;
    await new Promise<void>((resolve, reject) => {
      const abort = () => {
        clearTimeout(timer);
        reject(signal.reason);
      };
      const timer = setTimeout(() => {
        signal.removeEventListener('abort', abort);
        resolve();
      }, 1200);
      signal.addEventListener('abort', abort, { once: true });
      if (signal.aborted) abort();
    });
  }
}
