import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import type { Pace } from '@/lib/chess';
export default function RatingPanel({
  graph,
  pace,
  change,
}: {
  graph: { index: number; rating: number; date: string; opponent: string }[];
  pace: Pace;
  change: number | null;
}) {
  return (
    <section className="panel rating-panel">
      <div className="section-heading">
        <h2>Your rating journey</h2>
        <span className="subtle-chip chip-pace">{pace}</span>
      </div>
      <p className="panel-subtitle">
        One point per game. Progress rarely moves in a straight line.
      </p>
      <div className="rating-chart">
        {graph.length > 1 ? (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={graph} margin={{ top: 20, right: 12, bottom: 5, left: 0 }}>
              <defs>
                <linearGradient id="ratingFill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--ink-2)" stopOpacity={0.16} />
                  <stop offset="100%" stopColor="var(--ink-2)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid
                vertical={false}
                stroke="var(--border)"
                strokeDasharray="4 4"
              />
              <XAxis
                dataKey="index"
                tickLine={false}
                axisLine={false}
                minTickGap={70}
                height={36}
                tick={{ fontSize: 'var(--t-small)', fill: 'var(--ink-3)' }}
                tickFormatter={(index) => graph[index - 1]?.date ?? ''}
              />
              <YAxis
                domain={['dataMin - 15', 'dataMax + 15']}
                tickLine={false}
                axisLine={false}
                tick={{ fontSize: 'var(--t-small)', fill: 'var(--ink-3)' }}
                allowDecimals={false}
              />
              <Tooltip
                content={({ active, payload }) =>
                  active && payload?.[0] ? (
                    <div className="chart-tooltip">
                      <strong>{payload[0].payload.rating} rating</strong>
                      <span>
                        {payload[0].payload.date} · vs {payload[0].payload.opponent}
                      </span>
                    </div>
                  ) : null
                }
              />
              <Area
                type="linear"
                dataKey="rating"
                stroke="var(--ink-2)"
                strokeWidth={2.5}
                fill="url(#ratingFill)"
                activeDot={{
                  r: 5,
                  fill: 'var(--accent)',
                  stroke: 'var(--surface)',
                  strokeWidth: 3,
                }}
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        ) : (
          <div className="empty-chart">Your next games will start to show a trend.</div>
        )}
      </div>
      <div className="chart-foot">
        <span>Game ratings from Chess.com</span>
        <span>
          {change === null
            ? '1 recorded rating'
            : `${change >= 0 ? '+' : ''}${change} across this sample`}
        </span>
      </div>
    </section>
  );
}
