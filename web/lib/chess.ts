import { Chess } from 'chess.js';
export type Pace = 'rapid' | 'blitz' | 'bullet';
export type ApiPlayer = { username: string; rating: number; result: string };
export type ApiGame = {
  url: string;
  pgn: string;
  end_time: number;
  rated: boolean;
  rules: string;
  time_class: string;
  time_control: string;
  white: ApiPlayer;
  black: ApiPlayer;
};
export type Profile = {
  username: string;
  name?: string;
  avatar?: string;
  url?: string;
};
export type PlayerData = {
  profile: Profile;
  games: ApiGame[];
  pace: Pace;
  fetchedAt: string;
  archivesRead: number;
  warning?: string;
};
export type Game = {
  raw: ApiGame;
  id: string;
  pgn: string;
  date: number;
  opponent: string;
  opponentRating: number;
  rating: number;
  colour: 'w' | 'b';
  result: 'win' | 'loss' | 'draw';
  reason: string;
  opening: string;
  eco: string;
  timeControl: string;
  moves: ReturnType<Chess['history']> extends never ? never : import('chess.js').Move[];
};
export type Eval = { type: 'cp' | 'mate'; value: number };
export type Finding = {
  ply: number;
  beforeFen: string;
  afterFen: string;
  actual: string;
  best: string;
  bestUci: string;
  before: Eval;
  after: Eval;
  loss: number | null;
  title: string;
  explanation: string;
  pv: string[];
};
export type GameAnalysis = {
  id: string;
  findings: Finding[];
  source: 'native' | 'rust-browser' | 'verified';
  positions: number;
};
const draws = new Set([
  'agreed',
  'repetition',
  'stalemate',
  'insufficient',
  '50move',
  'timevsinsufficient',
]);
const losses = new Set(['checkmated', 'timeout', 'resigned', 'lose', 'abandoned']);
// HTML pattern uses Unicode sets (v): escape the literal hyphen, not underscore.
export const USERNAME_PATTERN = String.raw`[a-zA-Z0-9_\-]{1,64}`;
export function normaliseUsername(value: string) {
  const user = value.trim().toLowerCase();
  if (!/^[a-z0-9_-]{1,64}$/.test(user))
    throw new Error(
      'Use a Chess.com username with letters, numbers, underscores or hyphens.',
    );
  return user;
}
export function safeGameUrl(value: string) {
  try {
    const u = new URL(value);
    return (
      u.protocol === 'https:' &&
      ['www.chess.com', 'chess.com'].includes(u.hostname) &&
      /^\/game\/(live|daily)\/\d+\/?$/.test(u.pathname) &&
      !u.search
    );
  } catch {
    return false;
  }
}
export function isCompleted(g: ApiGame, user: string, now = Date.now() / 1000) {
  if (
    !g ||
    typeof g.pgn !== 'string' ||
    g.pgn.length > 200000 ||
    !g.white ||
    !g.black ||
    !g.rated ||
    g.rules !== 'chess' ||
    !Number.isFinite(g.end_time) ||
    g.end_time <= 0 ||
    g.end_time > now ||
    !safeGameUrl(g.url)
  )
    return false;
  const w = g.white,
    b = g.black;
  if (
    typeof w.username !== 'string' ||
    typeof b.username !== 'string' ||
    ![w.username.toLowerCase(), b.username.toLowerCase()].includes(user)
  )
    return false;
  return (
    (w.result === 'win' && losses.has(b.result)) ||
    (b.result === 'win' && losses.has(w.result)) ||
    (draws.has(w.result) && draws.has(b.result))
  );
}
export function parseGame(g: ApiGame, user: string): Game | null {
  try {
    if (!isCompleted(g, user)) return null;
    // chess.js rewrites Result from movetext. Check the original header first.
    const originalResults = [...g.pgn.matchAll(/^\[Result "([^"]+)"\]\s*$/gm)];
    const expectedResult =
      g.white.result === 'win' ? '1-0' : g.black.result === 'win' ? '0-1' : '1/2-1/2';
    if (originalResults.length !== 1 || originalResults[0][1] !== expectedResult)
      return null;
    const c = new Chess();
    c.loadPgn(g.pgn);
    const h = c.getHeaders();
    if (
      h.SetUp === '1' ||
      (h.Variant && h.Variant.toLowerCase() !== 'standard') ||
      h.White?.toLowerCase() !== g.white.username.toLowerCase() ||
      h.Black?.toLowerCase() !== g.black.username.toLowerCase() ||
      !h.Termination ||
      /unterminated|ongoing|in progress|unknown/i.test(h.Termination)
    )
      return null;
    const expected =
      g.white.result === 'win' ? '1-0' : g.black.result === 'win' ? '0-1' : '1/2-1/2';
    const marker = g.pgn
      .replace(/\{[^}]*\}/g, '')
      .trim()
      .match(/(1-0|0-1|1\/2-1\/2|\*)\s*$/)?.[1];
    if (h.Result !== expected || marker !== expected) return null;
    const colour = g.white.username.toLowerCase() === user ? 'w' : 'b',
      me = colour === 'w' ? g.white : g.black,
      opp = colour === 'w' ? g.black : g.white;
    const moves = c.history({ verbose: true });
    if (
      !moves.length ||
      moves.length > 600 ||
      !Number.isFinite(me.rating) ||
      !Number.isFinite(opp.rating)
    )
      return null;
    let opening = h.Opening;
    if (!opening && h.ECOUrl) {
      try {
        opening = decodeURIComponent(new URL(h.ECOUrl).pathname.split('/').pop() || '')
          .replace(/-\d.*$/, '')
          .replaceAll('-', ' ');
      } catch {}
    }
    return {
      id: g.url,
      raw: g,
      pgn: g.pgn,
      date: g.end_time,
      opponent: opp.username,
      opponentRating: opp.rating,
      rating: me.rating,
      colour,
      result: me.result === 'win' ? 'win' : draws.has(me.result) ? 'draw' : 'loss',
      reason: me.result,
      opening: opening || 'Unclassified opening',
      eco: h.ECO || '—',
      timeControl: g.time_control,
      moves,
    };
  } catch {
    return null;
  }
}
export function summarise(games: Game[]) {
  const wins = games.filter((g) => g.result === 'win').length,
    draw = games.filter((g) => g.result === 'draw').length;
  const openings = Object.values(
    games.reduce(
      (all, g) => {
        const key = `${g.colour}:${g.opening}`;
        const row = all[key] ?? {
          name: g.opening,
          colour: g.colour,
          games: 0,
          wins: 0,
          draws: 0,
        };
        row.games++;
        row.wins += Number(g.result === 'win');
        row.draws += Number(g.result === 'draw');
        all[key] = row;
        return all;
      },
      {} as Record<
        string,
        {
          name: string;
          colour: 'w' | 'b';
          games: number;
          wins: number;
          draws: number;
        }
      >,
    ),
  ).sort((a, b) => b.games - a.games);
  const colours = (['w', 'b'] as const).map((colour) => {
    const group = games.filter((g) => g.colour === colour);
    return {
      colour,
      games: group.length,
      wins: group.filter((g) => g.result === 'win').length,
    };
  });
  return {
    wins,
    draw,
    loss: games.length - wins - draw,
    winRate: games.length ? Math.round((wins / games.length) * 100) : 0,
    rating: games[0]?.rating ?? null,
    change: games.length > 1 ? games[0].rating - games.at(-1)!.rating : null,
    openings,
    colours,
    timeouts: games.filter((g) => g.reason === 'timeout').length,
  };
}
export function timeLabel(value: string) {
  const [base, increment = '0'] = value.split('+');
  return `${Number(base) / 60}${Number(increment) ? ` + ${increment}` : ' min'}`;
}
export function dateLabel(time: number) {
  return new Date(time * 1000).toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
  });
}
export function evalLabel(e: Eval) {
  return e.type === 'mate'
    ? `${e.value < 0 || Object.is(e.value, -0) ? '−' : ''}M${Math.abs(e.value)}`
    : `${e.value > 0 ? '+' : ''}${(e.value / 100).toFixed(1)}`;
}
