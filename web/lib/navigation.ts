import type { StudyPosition } from './study.ts';
export type StudioRoute = {
  view: 'welcome' | 'overview' | 'review' | 'library';
  reviewId?: number;
  example?: boolean;
  gameId?: string;
  position?: StudyPosition;
};
const integer = (value: string | null, max: number) => {
  if (!value || !/^\d+$/.test(value)) return undefined;
  const n = Number(value);
  return Number.isSafeInteger(n) && n <= max ? n : undefined;
};
export function readRoute(search: string): StudioRoute {
  const q = new URLSearchParams(search);
  const reviewId = integer(q.get('review'), Number.MAX_SAFE_INTEGER) || undefined;
  const example = q.get('example') === '1';
  const gameId = q.get('game');
  const validGame = gameId && /^\d+$/.test(gameId) ? gameId : undefined;
  const view =
    q.get('view') === 'library'
      ? 'library'
      : validGame && (example || reviewId)
        ? 'review'
        : example || reviewId
          ? 'overview'
          : 'welcome';
  const ply = integer(q.get('ply'), 600);
  const mode = q.get('mode');
  const position =
    ply === undefined
      ? undefined
      : {
          ply,
          mode: ['before', 'played', 'better', 'refutation'].includes(mode ?? '')
            ? (mode as StudyPosition['mode'])
            : ('game' as const),
          findingPly: integer(q.get('finding'), 600),
          linePly: integer(q.get('line'), 1000),
        };
  return { view, reviewId, example: example || undefined, gameId: validGame, position };
}
export function gameKey(url: string) {
  return url.split('/').filter(Boolean).at(-1) ?? '';
}
export function routeUrl(route: StudioRoute) {
  const q = new URLSearchParams();
  if (route.reviewId) q.set('review', String(route.reviewId));
  else if (route.example) q.set('example', '1');
  if (route.view === 'library') q.set('view', 'library');
  if (route.view === 'review' && route.gameId) {
    q.set('game', route.gameId);
    if (route.position) {
      const p = route.position;
      q.set('ply', String(p.ply));
      q.set('mode', p.mode);
      if (p.findingPly !== undefined) q.set('finding', String(p.findingPly));
      if (p.linePly !== undefined) q.set('line', String(p.linePly));
    }
  }
  return q.size ? `/?${q}` : '/';
}
