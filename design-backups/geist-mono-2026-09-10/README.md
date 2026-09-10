# Geist Mono typography backup

Saved before switching Tempo to Fira Code on 10 September 2026. This is the readable Geist Mono design approved by Dre: 16px primary text, 14px supporting text, larger chart labels and controls.

The active app keeps the Geist Mono dependency and font declarations as a fallback. To switch fonts without undoing later layout work, set `--font-ui` in `web/app/globals.css` to `'Geist Mono', 'SFMono-Regular', Consolas, monospace`, then rebuild the frontend/container.

These snapshots preserve the exact typography/layout at the time of the switch:

- `globals.css` → `web/app/globals.css`
- `main.tsx` → `web/selfhost/main.tsx`
- `rating-panel.tsx` → `web/components/rating-panel.tsx`

Only restore all three snapshots when intentionally reverting later layout changes too.
