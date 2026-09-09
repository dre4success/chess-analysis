import type { Pace, PlayerData, GameAnalysis } from './chess.ts';
import { convertFinding, type SavedReview } from './review-data.ts';

export type ReviewSummary = {
  id: number;
  username: string;
  pace: Pace;
  status: 'queued' | 'running' | 'complete' | 'failed' | 'cancelled';
  created_at: string;
  progress: { completed: number; total: number; positions: number };
};
export type ReviewJob = ReviewSummary & {
  message: string;
  error?: string;
  data?: PlayerData;
  review?: SavedReview;
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
