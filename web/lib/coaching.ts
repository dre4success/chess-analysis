import type { Game, GameAnalysis, Finding } from './chess.ts';
export const lessons: Record<string, { title: string; action: string; prompt: string }> =
  {
    'attacked-piece-ignored': {
      title: 'Notice what their move changed.',
      action:
        'After their move, name the newly attacked piece before choosing your reply.',
      prompt: 'Which of your pieces did the last move put under pressure?',
    },
    'line-opened': {
      title: 'Look behind the piece you move.',
      action: 'Trace the bishop, rook and queen lines your move will uncover.',
      prompt: 'What line becomes open when this piece leaves its square?',
    },
    'line-onto': {
      title: 'Give your destination a second look.',
      action:
        'Check enemy captures on your destination square, then count the recaptures.',
      prompt: 'Can your opponent profitably capture on your destination?',
    },
    'defender-left': {
      title: 'Keep your defenders connected.',
      action: 'Before moving a defender, identify the pieces that depend on it.',
      prompt: 'Which piece loses its protection if this defender moves?',
    },
    'capture-cost': {
      title: 'Finish the exchange in your head.',
      action: 'Follow captures and recaptures until the position settles.',
      prompt: 'What remains after the whole exchange?',
    },
    'missed-mate': {
      title: 'Start your search with checks.',
      action: 'Look at forcing checks before committing to a quieter move.',
      prompt: 'Which checks leave your opponent the fewest replies?',
    },
  };
export type Theme = {
  classification: string;
  title: string;
  action: string;
  prompt: string;
  affected: number;
  reviewed: number;
  examples: { game: Game; finding: Finding }[];
};
export function themesFor(
  games: Game[],
  analyses: Record<string, GameAnalysis>,
): Theme[] {
  const reviewed = games.filter((g) => analyses[g.id]);
  const groups = new Map<string, Theme>();
  for (const game of reviewed) {
    for (const finding of analyses[game.id].findings) {
      const classification = finding.classification ?? 'engine-verified-mistake';
      const lesson = lessons[classification];
      if (!lesson) continue;
      const group = groups.get(classification) ?? {
        classification,
        ...lesson,
        affected: 0,
        reviewed: reviewed.length,
        examples: [],
      };
      group.examples.push({ game, finding });
      groups.set(classification, group);
    }
  }
  return [...groups.values()]
    .map((g) => ({
      ...g,
      affected: new Set(g.examples.map((e) => e.game.id)).size,
      examples: g.examples.toSorted(
        (a, b) => teachingScore(b.finding) - teachingScore(a.finding),
      ),
    }))
    .sort(
      (a, b) =>
        b.affected - a.affected ||
        b.examples.length - a.examples.length ||
        a.classification.localeCompare(b.classification),
    );
}
/** Prefer a learnable decision in a competitive position over a late collapse. */
export function studyPriority(games: Game[], analyses: Record<string, GameAnalysis>) {
  const themes = themesFor(games, analyses);
  const moments =
    themes[0]?.affected >= 2
      ? [...themes[0].examples]
      : games.flatMap((game) =>
          (analyses[game.id]?.findings ?? []).map((finding) => ({ game, finding })),
        );
  return moments.sort((a, b) => teachingScore(b.finding) - teachingScore(a.finding))[0];
}
function teachingScore(f: Finding) {
  const competitive = f.before.type === 'cp' && f.before.value > -300 ? 10000 : 0;
  const explained = f.classification && lessons[f.classification] ? 1000 : 0;
  return competitive + explained + Math.min(f.loss ?? 800, 1500);
}
