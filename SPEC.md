# Product spec v2 — personal chess review tool

**One line:** enter a public chess.com username, get back a report that says *what habit keeps costing you games* — with every claim traceable to a reproducible position.

**Status:** working prototype in `reviewer/`. This spec is for a clean rebuild.
**v2.1 corrections (2 Aug 2026)** — three internal contradictions fixed during Rust
build planning:

- **§7.2 mate evaluations** were specified as `{"type": "mate", "plies": 3}`. UCI
  reports `score mate <y>` in **moves, not plies** — the field was wrong by roughly a
  factor of two. Now `moves`, with `winner` recorded explicitly rather than carried in
  the sign of an integer.
- **§3.1 completed-game guard** said to reject "any URL matching a live/ongoing game
  pattern". chess.com stores *finished* games at `/game/live/{id}` — including this
  spec's own §7.2 example URL — so that rule rejected almost every valid game. Narrowed
  to genuinely-ongoing signals.
- **§7.1 `MoveContext`** typed `eval_before`/`eval_after` as `float | None`, which
  contradicted §7.2's structured evaluations and permitted mate/centipawn mixing inside
  the detector layer. Now `Evaluation | None`. `material` was documented as "exchanges
  resolved"; it is now a plain piece count, with exchange-aware questions answered by
  static exchange evaluation on a named square.

Scoping decisions for the first build (six detectors not thirteen, clock promoted,
structural mode postponed) live in `PLAN.md`, not here — this document describes the
product, the plan describes what gets built first. The pre-correction original is
preserved outside the repo.
**v2 changes:** incorporates two rounds of technical review — corrected the "attacked-and-losing" definition, engine promoted to normal mode, canonical `review.json` with UCI + structured evals + schema versioning, seven new detectors (two experimental), fixed-node two-pass analysis, card churn control, completed-game guard in code, expanded test matrix with verdict/label separation.

---

## 1. What it is, and what it deliberately isn't

**Is:** a post-game coach. Reads finished games, finds *repeated* mistakes, explains each in one sentence, produces a short training card.

**Isn't:**

- **Not a live-game assistant.** Never touches an in-progress board. This keeps it clear of chess.com's Fair Play rules and is non-negotiable.
- **Not an engine-line dump.** Chess.com's Game Review already classifies moves well. Competing there is pointless.
- **Not a SaaS.** No accounts, no billing, no gated features.

**The distinguishing property:** every statement is *mechanically checkable*. Not "you struggle with tactics" but "on move 15 your knight left f6, opening h4–d8 onto your queen — here is the FEN, here is the line." Competitors narrate; this one cites.

---

## 2. Users and the core loop

**A human**, wanting to know what to practise:

```
username → pick a range → report.html → read the three worst moments
```

**An AI assistant**, asked to coach:

```
username → digest.md → paste into any chat → assistant reasons over verified facts
```

The second path is the point. Coaching through an AI today means pasting PGNs one at a time and burning context. A digest turns twenty games into one page of checked facts any assistant can work from — and the analysis outlives any single conversation.

---

## 3. Data source

**chess.com Public API.** Read-only, no auth. Finished games only.

```
GET /pub/player/{username}/games/archives      → list of month URLs
GET /pub/player/{username}/games/{YYYY}/{MM}   → { games: [ { pgn, time_control, url, ... } ] }
```

- **Send a descriptive `User-Agent`.** chess.com blocks anonymous clients.
- **Cache by month.** Closed months never change.
- **Check the terms before anything public-facing.** Personal and open-source use is fine; hosted redistribution needs reading.

Also accept a local `.pgn` and a pasted game URL. Lichess (`/api/games/user/{u}`) is a natural second source — same shape, simpler API.

### 3.1 Completed-game guard — enforced in code

The fair-play boundary must be a code path, not a paragraph in a README. Reject and skip, with a logged reason:

- any PGN whose `Result` is `*`
- any game lacking a confirmed termination (`Termination` absent, or no result marker)
- any URL that explicitly identifies an *ongoing* game — `/game/ongoing/`,
  `/play/online`, or a query parameter valued `live`/`ongoing`/`playing`.
  **Not** `/game/live/{id}`: chess.com keeps *finished* blitz and rapid games at that
  path, including the example URL in §7.2 below. `Result` and `Termination` are what
  prove finishedness; the URL is a weak secondary signal.
- any input arriving through a path that could carry an in-progress position

Fail closed: if it cannot be *proven* finished, do not analyse it. This runs before any engine is opened.

---

## 4. Pipeline

```
fetch → normalise → analyse → review.json → { report.html, digest.md, card.md }
```

`review.json` is the **single source of truth**. Every other output is a rendering of it. This makes reports reproducible, enables a web frontend later, and stops the Markdown becoming an accidental database.

Per user move:

1. **Verdict** — was this a mistake? *Engine only.*
2. **Classification** — *why*? Deterministic pattern matchers.
3. **Evidence** — FEN, ply, clock, best move, principal variation.

**Engine decides *whether*. Classifiers explain *why*. These must never be conflated** — see §5.

---

## 5. Detection taxonomy

### 5.1 Observation vocabulary — never a verdict

The prototype's original definition was wrong: *"attacked by something worth less than it = losing, even when defended."* A bishop attacking a queen usually just gains tempo — the queen moves. Four separate states:

| State | Definition | Verifiable how |
|---|---|---|
| **newly-attacked** | An enemy piece can now attack it that couldn't before. | Purely observable from the board. |
| **hanging** | Attacked, and the static exchange evaluation on that square is negative. | SEE — deterministic, no engine. |
| **materially-losing** | After best play, material is down by ≥2. | Exchange resolution or engine. |
| **tempo-lost** | The piece escapes, but the opponent gained a free move. | Engine only. |

**Only `newly-attacked` may be asserted without an engine.** The others require verification.

*Note for the rebuild:* the prototype already keeps this separation in code — the verdict comes from engine loss or resolved material, and the "loose piece" scan runs **only inside the already-flagged branch**, purely to classify. Keep that structure. The scan is genuinely valuable as a classifier; it is just never a verdict.

### 5.2 Detectors

Each is a pure function `(MoveContext) → Finding | None`.

| Flag | Definition | Confidence |
|---|---|---|
| **line-opened** | A piece I did *not* move became attacked because my move uncovered a line. | verified |
| **line-onto** | The moved piece landed on a square where SEE is negative. | verified |
| **line-blocked** | My move placed a piece in front of my own defender, unguarding something. | verified |
| **defender-left** | The moved piece was the *sole* defender of something subsequently lost. | verified |
| **attacked-piece-ignored** | Opponent's last move attacked a piece of mine; my reply neither moved, defended nor compensated it. | verified |
| **capture-cost** | I captured; after the exchange resolves I'm down material. | verified |
| **automatic-recapture** | I recaptured immediately while a stronger check or capture existed. | verified |
| **forcing-move-missed** | A decisive check or capture existed; I played something quiet. | verified |
| **threat-ignored** | Opponent had mate or a winning capture; my move didn't address it. | verified |
| **missed-mate** | Mate in one or two existed. | verified |
| **conversion-failure** | While ahead ≥2, I declined a safe trade that preserved the win. | **experimental** |
| **opened-diagonal** | A pawn move or exchange exposed a long bishop/queen line toward my own king or material. | structural |
| **check-without-purpose** | A check that gained nothing and helped the opponent. | **experimental** |

**Both experimental detectors ship behind a flag** until their false-positive rate is measured on the regression set.

*`check-without-purpose`* — "gained nothing" needs an eval-delta threshold or it fires on every useful in-between check.

*`conversion-failure`* — declining a queen trade while ahead is often *correct*. Fire only when **all three** hold: (a) a safe trade was genuinely available, (b) the engine confirms that trade preserved the advantage, and (c) the move actually played caused a meaningful evaluation drop. Two out of three is a false positive.

### 5.3 Aggregation — this is the product

A single flagged move is a curiosity. *"You did this in 6 of your last 10 games"* is a training plan.

**Rank patterns by games affected, not raw occurrences.** One 80-move game must not outweigh five separate games. Also aggregate by colour, by ECO, by phase, and by clock band.

---

## 6. Two modes, stated honestly

| Mode | Requires | May say |
|---|---|---|
| **Verified review** | Stockfish present | "This move lost 2.3 pawns. Better was Bxf5." Blunders, alternatives, principal variations. |
| **Structural scan** | Nothing | "This move opened a bishop line toward your rook." Observable facts only. **Never the word blunder.** |

Verified is the normal mode. Structural scan is a graceful fallback, clearly banner-labelled in every output, never presented as evaluation.

---

## 7. Data contracts

### 7.1 `MoveContext` — what a detector receives

Detectors need more than `(board, move)`:

```python
@dataclass
class MoveContext:
    board_before: chess.Board
    board_after:  chess.Board
    move:         chess.Move
    san:          str
    ply:          int
    user_colour:  chess.Color
    eval_before:  Evaluation | None  # structured, user POV — never a bare float
    eval_after:   Evaluation | None
    best_move:    chess.Move | None
    pv:           list[chess.Move]
    material:     int                # user POV, plain piece count, NOT exchange-resolved
    clock_secs:   float | None
    opponent_last_move: chess.Move | None
```

`eval_before`/`eval_after` are the same structured `Evaluation` type as §7.2 — a bare
float here would reintroduce mate/centipawn mixing inside the detector layer, which is
precisely what the structured schema exists to prevent.

`material` is a **plain piece count**, deliberately not exchange-resolved. Detectors
needing exchange-aware numbers — `line-onto`, `capture-cost` — call static exchange
evaluation on the relevant square instead. A single pre-resolved integer on the context
would bake one resolver's answer into every detector and hide which square the claim
was actually about.

### 7.2 `review.json` — canonical output

```json
{
  "schema_version": "1.0",
  "user": "dre4success007",
  "generated": "2026-08-01T18:04:00Z",
  "mode": "verified",
  "engine": {
    "name": "Stockfish", "version": "17",
    "scan_nodes": 150000, "deep_nodes": 1000000
  },
  "games": [{
    "url": "https://www.chess.com/game/live/172386685690",
    "input_pgn_sha256": "9f2c…",
    "date": "2026-08-01", "colour": "black", "result": "win",
    "opponent": {"name": "Anil4766", "rating": 819},
    "time_control": "600", "clock_used_pct": 36,
    "findings": [{
      "ply": 29,
      "before_fen": "r1bqr1k1/pp3pp1/5n1p/3p4/1b1P3B/1B1Q3P/PP3PP1/R2K2NR b - - 2 15",
      "after_fen":  "r1bqr1k1/pp3pp1/7p/3p4/1b1Pn2B/1B1Q3P/PP3PP1/R2K2NR w - - 3 16",
      "actual_uci": "f6e4", "actual_san": "Ne4",
      "best_uci":   "c8e6", "best_san":   "Be6",
      "eval_before": {"type": "cp",   "value": 20,  "pov": "user"},
      "eval_after":  {"type": "cp",   "value": -610, "pov": "user"},
      "classification": "line-opened",
      "explanation": "Your knight on f6 was blocking h4–d8. After Ne4 the bishop attacks your queen.",
      "principal_variation_uci": ["c8e6", "h4g3", "d8d7"],
      "confidence": "verified"
    }]
  }],
  "patterns": [
    {"classification": "line-opened", "games_affected": 4, "occurrences": 5,
     "example_refs": [{"url": "…", "ply": 29}]}
  ]
}
```

**UCI is canonical; SAN is presentation.** Always store both, and **generate SAN from `before_fen` + UCI** rather than trusting the SAN in the imported PGN. SAN is ambiguous without a position and imported PGNs are not always well-formed.

**Evaluations are structured, never bare floats:**

```json
{"type": "cp",   "value": 230, "pov": "user"}
{"type": "mate", "moves": 3, "winner": "user", "pov": "user"}
```

This is what stops mate scores leaking into centipawn arithmetic — the `±999` bug in §12 is unrepresentable in this schema.

**Invariants, asserted at write time:**

1. `before_fen` + `actual_uci` → resulting FEN **equals** `after_fen`.
2. `best_uci` is legal in `before_fen`.
3. Every element of `principal_variation_uci` is legal in sequence from `before_fen`.
4. No finding exists without `before_fen`, `after_fen` and a classification.

---

## 8. Engine strategy — two passes

"Depth 12" is not a budget; depth varies wildly by position. Instead:

**Both passes use a fixed *node* budget, never a depth or time target.** Depth reached varies by position; wall-clock varies by hardware. Nodes are the only setting that makes two runs comparable — and reproducibility is the whole premise.

**Pass 1 — scan.** Every user move at ~150k nodes. Cheap, uniform, identifies candidates.

**Pass 2 — confirm.** Only the largest eval swings and detector-flagged candidates, at ~1M nodes. This is where alternatives and PVs come from.

**Fixed nodes alone do not make a run reproducible.** Stockfish's SMP search is non-deterministic — the same node budget on multiple threads gives different results. Reproducibility requires pinning *all* of:

```json
"engine": {
  "name": "Stockfish", "version": "17",
  "scan_nodes": 150000, "deep_nodes": 1000000,
  "threads": 1, "hash_mb": 64, "multipv": 1,
  "clear_hash_between_positions": true,
  "analysis_order": "game-order"
}
```

`Threads: 1` is the important one. Record the lot in `review.json`; two runs match only when every field does.

**Cap output at three major lessons per game.** More than that and nobody acts on any of them.

This controls cost and concentrates compute where it changes the report. Rough budget: pass 1 ~2–5s/game, pass 2 ~10–20s/game.

---

## 9. The training card — churn control

Regenerating the whole card each run makes it thrash after one unusual game. Split it:

- **`card_base.md`** — hand-maintained. Repertoire, principles, the mantra. Changes only when the player decides.
- **`card_evidence.md`** — generated. Current recurring habits with their evidence.
- **`card.md`** — the two combined.

Rules: **never change priorities on fewer than 10 games**; rank by games-affected; require a pattern to persist across two consecutive runs before promoting it.

---

## 10. Product direction

The v1 spec contradicted itself — "useful for anyone with a username" alongside "should probably stop at the CLI." Resolved:

- **Open-source CLI/library is the core.** Trustworthy, reproducible, no gatekeeping.
- **A thin hosted layer** provides username entry and shareable public reports. No accounts, no billing.
- **Every hosted report must be reproducible locally** from the same `review.json`.

Note the competitive reality: several products already do username-based cross-game coaching ([Chessy](https://chessyapp.com/), [Blunders.ai](https://blunders.ai/), [ChessLens](https://mychesslens.vercel.app/), [Coachess](https://coachess.app/)). The defensible difference here is **verifiability and open source**, not the category.

---

## 11. Testing

| Layer | Contents |
|---|---|
| **Unit** | Small synthetic FENs, one rule each. Every detector: one position that must fire, two near-misses that must not. |
| **Regression** | ~15 full games with expected findings. Verified against an engine, not by hand. |
| **Negative** | Clean games that must produce **zero** findings. Sound sacrifices. Ordinary recapture sequences. Checks and mates that must never be flagged as blunders. |
| **Edge cases** | En passant, castling both sides, promotion, **under**promotion, stalemate, insufficient material, threefold. |
| **Invariants** | Every suggested move legal in its stated FEN. Every `before_fen` + `actual_uci` reproduces `after_fen`. No finding without a FEN. **UCI everywhere; SAN is presentation only.** |

**Verdicts and labels are verified differently — do not conflate them.**

The engine can prove that a move lost evaluation. It **cannot** prove that the right explanation was `line-opened` rather than `defender-left`. So:

| What | Verified by |
|---|---|
| *Was this a mistake?* (the verdict) | Engine, on the ~15 full games. Automatable. |
| *Which pattern was it?* (the label) | Synthetic single-rule positions, plus **human spot-checking** of full-game labels. Not automatable. |

*On test-corpus size:* 50+ hand-verified full games is expensive and the wrong shape — synthetic positions are cheaper and more precise. **Many synthetic + ~15 full games.** But the earlier phrasing "expectations generated by engine and spot-checked" was sloppy: that only covers verdicts. Every semantic label in the regression set still needs a human to look at it once.

**If hosted:** HTML-escape usernames, PGN headers and all imported metadata. Player names are attacker-controlled input.

---

## 12. Pitfalls — every one a bug actually shipped in the prototype

**The horizon bug.** Evaluating material immediately after a capture, before the recapture, makes every normal trade look like a blunder and real blunders look fine. Produced two confidently wrong analyses before it was caught. *Fix: quiescence search, both sides able to stand pat.*

**Captures and checks are not blunders.** A naive "attacked by something cheaper" scan fires on every recapture and every check — including checkmate. First version flagged 7 moves in a won game, 6 of them nonsense. *Fix: judge on resolved material or engine loss, never on a mid-exchange snapshot.*

**Sacrifices look like blunders to a material counter.** A sound attacking sacrifice reads as −3. This alone is why engine mode is normal mode.

**Depth asymmetry.** Comparing a move searched at depth 3 against an alternative at depth 4 produces garbage. Fix depth per comparison.

**Mate scores leak into material arithmetic.** A ±999 sentinel subtracted from a material count yields *"you lost 998 points."* Handle mate as its own category before any arithmetic.

**Clock data is gold and usually ignored.** chess.com PGNs carry `[%clk]` and `[%timestamp]`. Time-per-move exposed the clearest pattern in the source games: the losses came from a session where 1.4 of 10 minutes were used. Parse it, aggregate it, report it.

---

## 13. Open decisions

- **Name.** "Blunderproof" over-promises and **Blunders** is a shipping product. Prefer something describing the mechanism.
- **Lichess support** from day one or later?
- **Where `card_base.md` lives** — repo, or user's own directory?

---

## 14. Reference implementation

`reviewer/` contains a working prototype — *spec-by-example, not production code*:

- `chess_review.py` — fetch, analysis, HTML report; both engine and material paths
- `engine.py` — UCI wrapper, centipawn-loss calculation
- `stub_engine.py` — a fake UCI engine so integration can be tested without Stockfish
- `sample.pgn` / `sample_report.html` — six-game regression set and its output

It reproduces every mistake found by hand across those six games.

**Note:** these files live in this workspace's `reviewer/` folder. A reviewer who can't see them has reviewed the spec, not the implementation — share the folder before asking for an implementation review.
