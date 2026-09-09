import { isCompleted, normaliseUsername, type ApiGame, type Pace } from '@/lib/chess';
const LIMIT = 40,
  MAX_ARCHIVES = 8,
  MAX_BYTES = 12000000;
async function read(url: string, user: string) {
  const response = await fetch(url, {
    headers: {
      'User-Agent': `Tempo/1.0 (post-game review; Chess.com username: ${user})`,
      Accept: 'application/json',
    },
    signal: AbortSignal.timeout(18000),
  });
  if (response.status === 404)
    throw Object.assign(
      new Error(
        'That Chess.com profile could not be found. Check the spelling and try again.',
      ),
      { status: 404 },
    );
  if (response.status === 429)
    throw Object.assign(
      new Error('Chess.com is busy. Please wait a minute, then try again.'),
      { status: 429 },
    );
  if (!response.ok)
    throw new Error(
      'Chess.com could not send your games right now. Please try again shortly.',
    );
  const reader = response.body?.getReader();
  if (!reader) throw new Error('Chess.com returned an empty response.');
  let size = 0,
    text = '',
    decoder = new TextDecoder();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > MAX_BYTES)
        throw new Error(
          'This archive is too large to load safely. Try a different player.',
        );
      text += decoder.decode(value, { stream: true });
    }
    text += decoder.decode();
    return JSON.parse(text);
  } finally {
    await reader.cancel().catch(() => {});
  }
}
export async function GET(
  request: Request,
  { params }: { params: Promise<{ username: string }> },
) {
  let user: string;
  try {
    user = normaliseUsername((await params).username);
  } catch (e) {
    return Response.json({ error: (e as Error).message }, { status: 400 });
  }
  const pace = new URL(request.url).searchParams.get('pace') ?? 'rapid';
  if (!['rapid', 'blitz', 'bullet'].includes(pace))
    return Response.json({ error: 'Choose rapid, blitz or bullet.' }, { status: 400 });
  try {
    const root = `https://api.chess.com/pub/player/${user}`;
    const profile = await read(root, user);
    if (typeof profile.username !== 'string')
      throw new Error('Chess.com returned an incomplete profile. Please try again.');
    const archiveData = await read(`${root}/games/archives`, user);
    if (!Array.isArray(archiveData.archives))
      throw new Error('Chess.com returned an incomplete archive list.');
    const archivePattern = new RegExp(
      `^https://api\\.chess\\.com/pub/player/${user}/games/\\d{4}/\\d{2}$`,
      'i',
    );
    const archives = archiveData.archives
      .filter((x: unknown) => typeof x === 'string' && archivePattern.test(x))
      .sort()
      .reverse();
    let games: ApiGame[] = [],
      archivesRead = 0;
    for (const archive of archives.slice(0, MAX_ARCHIVES)) {
      const month = await read(archive, user);
      if (!Array.isArray(month.games))
        throw new Error('Chess.com returned an incomplete month. Please try again.');
      games.push(
        ...month.games.filter(
          (g: ApiGame) => g.time_class === pace && isCompleted(g, user),
        ),
      );
      archivesRead++;
      if (games.length >= LIMIT) break;
    }
    games = Array.from(new Map(games.map((g) => [g.url, g])).values())
      .sort((a, b) => b.end_time - a.end_time)
      .slice(0, LIMIT);
    return Response.json(
      {
        profile: { username: profile.username, name: profile.name },
        games,
        pace: pace as Pace,
        archivesRead,
        fetchedAt: new Date().toISOString(),
        warning:
          games.length < LIMIT && archives.length > MAX_ARCHIVES
            ? 'Searched the eight most recent active months. Older games are outside this review.'
            : undefined,
      },
      {
        headers: {
          'Cache-Control': 'private, max-age=300',
          'X-Content-Type-Options': 'nosniff',
        },
      },
    );
  } catch (error) {
    const e = error as Error & { status?: number };
    return Response.json(
      {
        error:
          e.name === 'TimeoutError'
            ? 'Chess.com took too long to respond. Please try again.'
            : e.message || 'Your games could not be loaded.',
      },
      { status: e.status ?? 502 },
    );
  }
}
