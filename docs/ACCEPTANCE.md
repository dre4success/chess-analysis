# Stage 11 acceptance

Technical implementation reaches the CLI. Release acceptance is **pending**.
Optional API/UI/hosting work has not started.

## Recorded evidence

The 2026-09-05 acceptance run uses 12 completed rated rapid games from
`dre4success007`, recorded in `tests/fixtures/rapid-12.json` and `.pgn`. Online
fetch populated the cache; analysis then ran offline at 150,000 / 1,000,000 nodes,
200 cp threshold, with Stockfish 18 and the digest recorded in `VALIDATION.md`.
The output lives in `review-output/acceptance-2026-09-05/` (ignored generated data).

A separate regression replays the original winning queen-sacrifice game. It checks
both budgets directly at 27…Qxf1+ before applying the three-finding cap, then runs
the full candidate pipeline. This prevents the cap from hiding a false positive.

The first real run uncovered an interrupted-iteration UCI bound/PV mismatch.
Regression coverage now keeps the last exact score and its matching PV together,
while separately protecting the final UCI best move from condemnation. Reviewing
its output also caught an equal rook trade mislabeled `line-onto`; landing and
capture-cost evidence now subtracts the material gained by the actual capture.

## Human review still needed

Open `review-output/acceptance-2026-09-05/report.html` and inspect each semantic
label against the two boards. Record any incorrect or confusing explanation by
URL and ply in the table below. Engine-confirmed evaluation loss alone does not
prove that the semantic label identifies the correct cause.

| Game URL / ply | Label correct? | Wording clear? | Correction |
|---|---|---|---|
| Pending human review | | | |

Then record one concrete practice change supported by the recurring examples.
A fallback label is a detector gap, not a diagnosed habit. Groups with only a few
games should be treated as observations. If no lesson is useful, improve the core
before adding an API, UI or deployment.

**Practice change:** pending Dre's review.

**Go/no-go:** pending; no release approval is inferred from passing tests.

## Visual verification limit

HTML generation, board dimensions, escaping and JSON-only rendering are tested.
A browser screenshot could not be obtained: no browser is connected and native
computer-use permissions are unavailable in this session. Visual layout still
needs a browser check. The report is self-contained and uses no network assets.
