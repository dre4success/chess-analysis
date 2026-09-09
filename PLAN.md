# Build plan — chess review backend in Rust

**Status:** authoritative roadmap. Supersedes `RUST_BACKEND_LEARNING_ROADMAP.md`
(archived in `reference/`), merged with its corrections.
**Date:** 2 August 2026

**Implementation update — 5 September 2026:** resumed the unfinished Stage 3
worktree and implemented through the Stage 11 CLI. Automated tests, real Stockfish
regressions and a 12-game acceptance run are recorded in `VALIDATION.md`. Stage 11's
human semantic-label review and practice-change decision remain pending;
Stages 12–14 have not started. The user authorized autonomous
implementation for this continuation, superseding the original typing workflow.

---

## How we work

You type the code. I explain it, generate it, review it, and help read compiler
errors. Ask anything at any point. One stage at a time — don't start the next until
the current one compiles and its tests pass. Commit at the end of each stage.

A stage is done when you can explain: what goes in, what comes out, what invariant it
protects, and how its tests would catch a regression.

**V1 scope decision — verified review only.** V1 requires Stockfish. SPEC §6's
structural-scan fallback is postponed: it's explicitly a graceful degradation, and
building a second analysis path before the primary one works costs more than it
returns. The `Confidence` vocabulary from Stage 1 still exists — it describes how a
_detector_ was validated, independent of mode — so nothing is painted into a corner.

---

## The stages

| #   | Goal                                               |
| --- | -------------------------------------------------- |
| 0   | A Cargo package that builds and tests              |
| 1   | Represent evaluations without magic numbers        |
| 2   | Read a PGN; refuse anything not provably finished  |
| 3   | Write and validate `review.json`                   |
| 4   | Talk UCI to a fake engine                          |
| 5   | Talk UCI to real Stockfish                         |
| 6   | Find candidate mistakes in one game                |
| 7   | Explain one candidate — the `line-opened` detector |
| 8   | The rest of the detector set                       |
| 9   | Rank habits across games                           |
| 10  | Fetch games from chess.com                         |
| 11  | Ship the CLI, then judge whether it's useful       |
| 12  | _Optional_ — local HTTP API for the UI             |
| 13  | _Sketch_ — web UI reading `review.json`            |
| 14  | _Sketch_ — deploy it                               |

Stages 0–3 need no engine and no network. **Stage 11 is a go/no-go**: if the tool
isn't useful, we fix the core rather than adding features. Stages 12–14 are sketched,
not planned — they exist to prove the architecture has a deliberate path to a deployed
product, and get detailed only if Stage 11 passes.

---

## Stage 0 — Ground floor

**Goal:** a Cargo package that builds and tests.

One package with both a library and a binary target — not a workspace. Splitting
crates before the domain settles is ceremony; extracting one later is cheap.

```
chess-analysis/
├── Cargo.toml
└── src/
    ├── lib.rs     # the library — everything real lives here
    └── main.rs    # the CLI — argument parsing and wiring only
```

**You'll learn:** cargo, `lib.rs` vs `main.rs`, `mod`/`pub`, `cargo check`,
`cargo test`, `cargo fmt`, clippy.

**Done when:** `cargo test` passes one deliberately trivial test. `git init`, first
commit. You can explain what `Cargo.toml` does and why there are two targets.

---

## Stage 1 — Evaluations

**Goal:** represent a chess evaluation so the spec's worst bug can't be written.

```rust
pub enum Evaluation {
    Centipawns { value: i32 },
    Mate { winner: ReviewedSide, moves: u16 },
}
```

Two deliberate choices: **mate is a separate variant**, so it can never be serialized
as a centipawn sentinel — SPEC §12's _"you lost 998 points"_ bug. And **the winner is
a field, not a sign**, which removes a class of sign-flip bug at the POV boundary.

You need exactly one escape hatch — ranking findings means comparing a mate against a
centipawn score:

```rust
/// Total ordering across both variants. RANKING ONLY.
/// Never serialize this; never do arithmetic on it.
pub fn ordering_score(&self) -> i32
```

The enum alone doesn't make the sentinel bug impossible — this method reintroduces a
flat integer. Safety comes from it being _one_ named, tested, audited chokepoint
instead of arithmetic scattered everywhere.

**You'll learn:** enums with named fields, derive macros, methods, exhaustive `match`,
unit tests.

**Done when:** mate never appears as a number in JSON. Every comparison case has a
test — cp vs cp, winning mate, losing mate, mate vs cp both directions.

---

## Stage 2 — Games in, unfinished games out

**Goal:** read a PGN, and make analysing an unfinished game a compile error.

```
RawGame → CompletionPolicy → CompletedGame → analysis
```

Analysis functions accept only `CompletedGame`, and the only way to make one is
through a policy. SPEC §3.1's "non-negotiable" boundary becomes `rustc`'s job.

**Policies are per-source** — they produce the same type but need different evidence:

- _chess.com API_ — finished `Result`, matching movetext marker, source metadata
- _local PGN_ — its own documented rules; many valid PGNs have no `Termination`

> **SPEC v2.0 §3.1 was wrong; v2.1 fixes it.** It said reject "any URL matching a
> live/ongoing game pattern" — but chess.com keeps _finished_ games at
> `/game/live/{id}`, including the spec's own §7.2 example. Match `/game/ongoing/`,
> `/play/online`, and query params instead. `Result` and `Termination` are what
> actually prove finishedness.

**You'll learn:** traits, `Result` and `?`, error enums with `thiserror`, modules and
visibility, typestate, `String` vs `&str`.

**Done when:** `Result "*"` is rejected, a mismatched movetext marker is rejected, a
non-standard variant is rejected, and a test shows `RawGame` cannot reach analysis.
Castling, promotion and underpromotion fixtures replay correctly.

---

## Stage 3 — `review.json`

**Goal:** write the canonical output, and refuse to write an invalid one.

`review.json` is the single source of truth; every other artifact renders from it.

```rust
pub fn validate(review: Review) -> Result<ValidatedReview, ValidationError>
```

**A `Result`, not a panic.** This is a validation boundary handling imported data, not
an impossible internal state.

The four invariants from SPEC §7.2, checked before any write:

1. `before_fen` + `actual_uci` reproduces `after_fen`
2. `best_uci` is legal in `before_fen`
3. every PV move is legal in sequence
4. no finding without both FENs and a classification

Write atomically — temp file then rename — so a failed run can't leave a convincing
partial file.

**You'll learn:** serde derive, `#[serde(tag = "type")]`, domain types vs
serialization types, schema versioning.

**Done when:** serialize → deserialize preserves equality. A deliberately corrupted
finding is rejected with a useful error.

---

## Stage 4 — Speak UCI to a fake engine

**Goal:** drive a deterministic stub engine end to end, with no Stockfish involved.

Learning process I/O and protocol parsing is hard enough without also debugging search
behaviour. The stub makes it deterministic, and it stays useful forever — the test
suite never needs Stockfish installed.

Handshake `uci` / `isready` / `quit`, send `position fen …` and `go nodes …`, parse
`info` and `bestmove`.

**You'll learn:** `std::process::Command`, piped stdin/stdout, `BufReader`, protocol
state machines, timeouts, `Drop` for cleanup, traits as test doubles.

**Done when:** tests pass without Stockfish. A timeout can't orphan a child process.
Malformed engine output produces a structured error, not a panic. Mate signs are
correct from the reviewed player's POV.

---

## Stage 5 — Real Stockfish

**Goal:** analyse one position at a fixed node budget, and record what was pinned.

Fixed **nodes**, never depth or wall time — nodes are the only setting that makes two
runs comparable. `Threads: 1`, `Hash: 64`, `MultiPV: 1`, documented hash-clearing.

Record engine version, executable SHA-256, NNUE identity, all UCI options, node
budgets, analysis order, platform and architecture.

**We do not promise byte-identical evaluations across machines or binaries.** We
record enough to explain any difference, and test observed same-machine stability.

**You'll learn:** config validation, paths and platform differences, RAII cleanup,
hashing a file as a stream, newtypes (`Nodes(u64)` can't be passed where a depth was
meant).

**Done when:** the engine always shuts down, including after an error. The best move
is legal in the supplied FEN. Two same-machine runs are compared and the result is
written down honestly.

---

## Stage 6 — Candidates

**Goal:** evaluate every move you made in one game and shortlist the suspicious ones.

Two passes, per SPEC §8. **Pass 1** scans every move at ~150k nodes. **Pass 2**
confirms only the shortlist at ~1M nodes.

**Verdicts come only from pass 2.** At scan budget the _ordering_ of near-boundary
candidates isn't reliable, so pass 1 shortlists and decides nothing. Cap at three
findings per game, applied _after_ confirmation.

**A `Candidate` is its own internal type, not a `Finding`.** It carries legal
before/actual/after positions and structured evaluations — and nothing else. It has no
classification, because nothing has explained it yet, and it never reaches
`review.json`. Only Stage 7+ turns a candidate into a finding by adding evidence.

(Stage 3's finding contract requires a classification, so applying it to candidates
would be a contradiction. Two types, two contracts.)

**Regression test — the winning queen sacrifice.** AUDIT §1.3 caught the prototype
reporting a _winning_ combination as a 4-point blunder, because its material resolver
stopped one ply before the quiet promotion `a1=Q+`. That position belongs here: with
Stockfish authoritative, the pipeline must **not** flag it. It's the cleanest proof
that the engine has replaced the arithmetic that got it wrong.

**You'll learn:** orchestration without shared mutable state, lifetimes around
replayed positions, separating pure calculation from side effects, iterator
collection.

**Done when:** checkmate you delivered is never flagged. Terminal positions don't
reach the engine. Signs are correct for both colours. The `a1=Q+` position produces no
finding. Every candidate has legal positions and structured evals.

---

## Stage 7 — The first detector

**Goal:** explain one candidate — your move uncovered a sliding attack on your own piece.

```rust
fn detect(ctx: &MoveContext) -> Option<ClassificationEvidence>
```

**A detector returns evidence, not a `Finding`.** Only the pipeline can combine
evidence with engine evaluations into a finding — so a detector is structurally
incapable of manufacturing a verdict. That's SPEC §5's core boundary, enforced by
types.

**Done when:** it fires on the spec's `Nf6-e4` example. It does _not_ fire when a
moved queen is simply attacked, and does _not_ fire when the line already existed.
Every synthetic position asserts its own preconditions first — AUDIT §2.3 found _seven
bad test positions versus two real bugs_, so a fixture that doesn't prove its own setup
is a liability.

**You'll learn:** pure functions, pattern matching, table-driven tests, lifetimes on
borrowed positions.

---

## Stage 8 — The rest of the set

**Goal:** a small audited set of explanations, plus an honest fallback.

`line-onto` · `defender-left` · `attacked-piece-ignored` · `capture-cost` ·
`missed-mate` · **`engine-verified-mistake`**

That last one matters: when the engine proves a move was costly but no detector fits,
say so. Admitting the mechanism is unclassified beats inventing a confident story —
and for a tool whose whole pitch is verifiability, it's the only defensible answer.

**Two things from `AUDIT.md`:**

- **`line-opened` must split from `defender-left`** (§2.1). Discriminate on what the
  vacated square was doing: _on the ray_ between attacker and victim → `line-opened`;
  _in the victim's defender set_ → `defender-left`. Different lessons, different fixes
  at the board.
- **SEE arrives here, not earlier** — only as wide as `line-onto` and `capture-cost`
  actually need: capture-promotions, legal recaptures, pins, x-rays. Nothing more.
  **We do not port `resolved_balance()` or quiescence** unless a later stage proves we
  need it; the prototype needed it only because it had no engine.

  _Note the distinction:_ AUDIT §1.3's promotion bug was a **quiescence** failure — it
  missed the quiet promotion `a1=Q+`, a move SEE would never consider anyway, since
  SEE only resolves captures onto one square. That bug's regression test lives in
  Stage 6, against the engine pipeline. Don't let it drive SEE's design.

**Precedence is explicit.** Findings carry a primary `classification` plus
`also_matched: Vec<_>` — a real blunder trips three detectors, something has to choose
which lesson you read, and keeping the losers tells us how much they overlap.

**Done when:** every detector has one positive and two near-miss tests. Sound checks,
ordinary recaptures and checkmate are negative cases. Every candidate gets exactly one
primary label.

---

## Stage 9 — Patterns across games

**Goal:** turn findings into a training plan. SPEC §5.3 — _this is the product_.

**Rank by games affected, not occurrences.** One 80-move game must not outweigh five
separate games. Report the denominator too. Break down by colour, phase, ECO and clock
band. No strong conclusions from tiny groups.

**Clock parsing is done carefully or not at all.** `%clk` is time _remaining_, so
deriving time _spent_ needs the previous clock and the increment. Document what the
annotations mean and test the units before making any claim. Keep `None` rather than
inventing zero — missing data must not distort an average.

**You'll learn:** `HashMap` vs `BTreeMap`, the `entry` API, grouping, stable sorting,
optional data.

**Done when:** five affected games outrank five occurrences in one game. Reruns give
identical rankings and examples. Every example reference resolves to a real finding.

---

## Stage 10 — chess.com

**Goal:** username in, completed games out, cached.

Serial fetching, no parallel bursts. Descriptive `User-Agent` — they block anonymous
clients. Cache closed months immutably; they never change. Handle 404, 429, timeouts
and malformed responses distinctly.

Returns `RawGame` — the Stage 2 guard still runs. Network input is exactly the path
that could carry an in-progress game.

**Blocking HTTP (`ureq`)**, not async. Async waits for Stage 12, where it's the lesson
rather than a tax.

**Done when:** a cached closed month works offline. One failed month doesn't silently
corrupt the review. Tests use recorded fixtures, never the live service.

---

## Stage 11 — Ship it, then judge it

**Goal:** one command produces `review.json` and `digest.md`; a separate command
renders `report.html` from the JSON alone. Then decide whether the thing is any good.

```
chess-review review <username> --last 20 --output review-output
chess-review render review-output/review.json
chess-review validate review-output/review.json
```

`digest.md` is the point of the whole project (SPEC §2) — twenty games as one page of
checked facts you paste into any assistant. Generate it purely from `review.json`.
HTML-escape all imported metadata; player names are attacker-controlled.

**Then the go/no-go.** Review 10–15 real games. Engine verdicts check automatically;
**every semantic label needs your eyes once** — that part is not automatable. Record
false positives and confusing wording.

**Release criteria:** no unfinished game can reach Stockfish. All persisted moves pass
legality invariants. No known false labels remain. **At least one aggregated lesson
actually changes what you plan to practise.**

If that last one fails, we improve the core — not add platforms or UI.

---

## Stage 12 — Optional: local HTTP API

**Goal:** serve reviews over HTTP, _if the UI turns out to need it._

Decide only after Stage 11, when there's a real UI with a real requirement. The
alternatives — UI reads `review.json` directly, or a desktop wrapper — may be enough.

If we do build it: analysis is CPU-bound and blocking, HTTP is async, and
**`spawn_blocking` alone is not the answer** — several concurrent requests would fight
over one Stockfish child process. It needs a single analysis-job queue. Getting that
boundary right is the actual lesson.

**You'll learn:** async/await, tokio, axum, `Arc` for shared state, why blocking work
needs a queue and not just a thread.

---

## Stage 13 — Sketch: the web UI

**Goal:** a browser view of a review.

I build this; it has no say in the backend's design. It consumes `review.json` — either
a static file or Stage 12's API. That constraint is the whole point: if the UI needs
something the JSON can't express, the schema is wrong, not the UI.

Detailed only after Stage 11 passes.

## Stage 14 — Sketch: deployment

**Goal:** the thing runs somewhere other than your laptop.

Known shape, unplanned detail: a single analysis-job queue (one Stockfish process,
many requests), per-job resource and time limits, artifact storage for generated
reviews, Stockfish packaged into the image with its NNUE identity recorded, the Stage 2
completion guard still enforced on every input path, and enough logging to explain any
review after the fact.

SPEC §10's caution applies — _"check the terms before anything public-facing"_ — and
every hosted report must still reproduce locally from the same `review.json`.

---

## Corrections to `SPEC.md`

Applied to `SPEC.md` directly (see its changelog) — a knowingly-wrong canonical schema
plus a correction living elsewhere is two competing sources of truth, and Stage 3 is
where that would bite. The pristine original is preserved outside the repo.

**Fixed in `SPEC.md` v2.1** — factual errors:

| §   | Was                                         | Now                                                                                               |
| --- | ------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| 7.2 | `{"type": "mate", "plies": 3}`              | UCI reports mate in **moves**, not plies. `moves`, plus `winner` recorded explicitly              |
| 3.1 | reject URLs matching a live/ongoing pattern | chess.com stores finished games at `/game/live/{id}` — matching "live" rejected nearly everything |

**Not changed in `SPEC.md`** — these are build scoping, and belong here:

| §   | Spec says                                 | This plan does                                                                          |
| --- | ----------------------------------------- | --------------------------------------------------------------------------------------- |
| 5.2 | 13 detectors                              | 6 + `engine-verified-mistake` fallback; the rest once false-positive rates are measured |
| 5.3 | clock band as one breakdown among several | clock gets its own section — §12 says it was the clearest signal in the source games    |
| 6   | two modes, structural as fallback         | verified only; structural postponed                                                     |

---

## Deliberately postponed

**Structural-scan mode (SPEC §6)** · accounts and public report URLs · Lichess ·
pasted game URLs · the two experimental detectors · training-card promotion rules ·
parallel engine analysis · the large detector taxonomy.

Postponed, not rejected. Reconsider each only once the CLI produces useful reviews.

---

## Still open

Project name · exact crate versions · initial centipawn-loss threshold · node budgets
on your hardware · local-PGN completion policy details · where the renderer lives ·
where config and cache live.

Each gets decided at the stage that needs it.
