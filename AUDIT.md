# Prototype audit — detector logic and exchange resolution

**Scope, as agreed:** not "does the prototype implement v2" (it doesn't, and that's expected). The question is whether the detector logic and exchange resolution are **trustworthy enough to carry into the rebuild**.

**Method:** synthetic positions probing each rule, plus adversarial cases against the resolver. Test scripts are `reviewer/audit_exchange.py` and `reviewer/audit_detectors.py`.

**Headline:** two genuine soundness bugs in the resolver, one genuine design flaw in the classifier. The rest holds up. Notably, **most of my initial "failures" were bad test positions, not bad code** — worth stating because it's the failure mode an auditor is most likely to fall into here.

---

## 1. Exchange resolution

### 1.1 BUG — `quiesce()` offers stand-pat while in check

**Severity: high. Fix before reuse.**

```
FEN  6kb/8/8/8/8/3n4/8/R3K3 w - - 0 1
     White Ra1 + Ke1   ·   Black Kg8, Bh8, Nd3

white in check:       True
white captures:       NONE
a1 rook attacked by:  h8   defended by: NOBODY

quiesce() balance:    -1
truth (Ke2, Bxa1):    -6      ← wrong by 5 points
```

The resolver's first act is `stand = material(b, me)`, returned whenever no captures exist. That models "the side to move can decline to capture" — correct in a quiet position, **false when in check**, where moving is compulsory and may lose material.

This is reachable in the live path: `best_reply_material` pushes each opponent move then calls `quiesce` at depth 0. Any opponent move that gives check lands in exactly this state.

**Fix:** if `board.is_check()`, do not stand pat — search all evasions, not just captures. See also §1.3: the same narrow move-generation set causes a second bug.

### 1.2 Cases that pass

| Case | Result |
|---|---|
| Plain hanging piece | correct |
| Even trade offered, no captures | correct |
| Pinned attacker (can't legally capture) | correct — `legal_moves` handles it |
| En passant | correct — `is_capture()` includes ep |
| Capture-promotion | correct — values the promotion |
| Checkmate / terminal | correct — ±999 sentinel |
| X-ray defenders revealed mid-exchange | correct — legal moves regenerated each ply |

### 1.3 BUG — promotions are invisible to the resolver

**Severity: high. Upgraded from "known limitation" after a live false positive.**

I originally logged this as a standard quiescence limitation worth documenting rather than fixing. That was wrong. Tested against a real game, it **flagged the winning move as a 4-point blunder**:

```
position after 27.Na3, material level (W+0), black pawn on a2

27...Qxf1+  28.Kxf1        → W+4   ← resolver stops here, reports a 4-point loss
            28...a1=Q+     → W−4   ← the actual point: Black is up four

quiesce() at the stopping point: −4 for Black
```

`a1=Q+` is a **promotion, not a capture**, so quiescence never generates it. The resolver stops one ply short of the entire idea and inverts the verdict — calling the best move of the game a blunder.

This matters more than a footnote suggests: pawn-promotion combinations are exactly the tactic an improving player most needs credit for, and this class of move is precisely where the tool will be confidently wrong.

**Fix:** quiescence must generate **captures *and* promotions** (and, per §1.1, all evasions when in check). Three move classes, not one.

### 1.4 A trap for the auditor

`quiesce()` returns the **material balance**, not the gain from a capture. Four of my ten first-pass expectations were wrong because I read it as a delta. Any rebuild should name this function unambiguously — `resolved_balance()` rather than anything suggesting a score.

---

## 2. Detector precision

### 2.1 DESIGN FLAW — `line-opened` conflates two distinct mistakes

**Severity: medium. Splits cleanly; v2 already anticipates it.**

Same start position, two different errors, one label:

```
3qk3/8/5n2/8/7B/8/8/4K3 b

A) Nd5  → "left your queen on d8 attacked by h4"    [line-opened]
   The knight was BLOCKING the bishop's path.        ← genuine line-opened

B) Qc7  → "left your knight on f6 attacked by h4"    [line-opened]
   The queen was DEFENDING the knight.               ← actually defender-left
```

The detector asks only *"did something of mine become loose that I didn't move?"* — which is true in both cases and does not distinguish uncovering a line from abandoning a guard.

These are different lessons. "Your knight was blocking something" and "your queen was the only defender" require different fixes at the board. v2's taxonomy already separates `line-opened` from `defender-left`; the rebuild must implement the discrimination:

- **line-opened** — the moved piece vacated a square *on the ray* between attacker and victim.
- **defender-left** — the moved piece was in the victim's defender set before and not after.

Both can be true at once; pick a primary by which relationship the vacated square satisfies.

### 2.2 Precision on clean probes

| Probe | Result |
|---|---|
| Knight leaves, bishop hits queen | fires — correct |
| Knight leaves, nothing behind it | silent — correct |
| Rook steps onto a bishop diagonal | fires — correct |
| Rook moves to a safe square | silent — correct |
| Mate in one available, quiet move played | fires — correct |

No false positives on captures, checks or recaptures — the fix that removed the earlier 7-flags-in-a-won-game noise is holding.

### 2.3 A second trap for the auditor

Three of my "near-miss" positions were constructed wrong:

- a pawn on b7 does **not** defend b8 (pawns capture diagonally *forward*)
- a pawn on d7 does **not** defend d5
- my "no mate available" control had `Ra8#` on the board

**Every synthetic test position needs its own assertion of the property it claims to test** — assert the defender set, assert no mate exists — otherwise the test proves nothing and produces false audit findings. This is a requirement for the rebuild's test suite, not just advice.

---

## 3. Migration map

### Keep — correct, reusable

- **Quiescence structure** (both sides may stand pat, recursion regenerates legal moves) — sound apart from §1.1
- **Verdict/classification separation** — verdict from engine loss or resolved material; the loose-piece scan runs only inside the already-flagged branch. This is v2's core architecture, already present
- **`loose()` as a classifier vocabulary** — the outgunned/undefended distinction is useful; it just must never be a verdict
- **Mate handled as its own category** before material arithmetic
- **Clock parsing** (`[%timestamp]` → seconds) — small and correct

### Rewrite — sound idea, obsolete shape

- **`quiesce()`** — fix check handling, rename to something that doesn't read as a score
- **`analyse()`** — currently emits an ad-hoc dict; must emit `review.json` findings with `after_fen`, UCI, structured evals
- **`loose()` return type** — collapse to the four-state vocabulary (newly-attacked / hanging / materially-losing / tempo-lost), with SEE for `hanging`
- **Engine limits** — depth-based; must become fixed-node with `Threads=1` pinned
- **HTML renderer** — should render `review.json`, not analysis state

### Discard

- **Material-only verdict path as a default.** Fine as `--structural` fallback; must never label a move a mistake
- **`kind="drop"` generic bucket** — an unclassified finding is a detector gap, not a category

### Experimental — plausible, insufficiently tested

- **`forced_mate()` gated on "opponent has a check"** — cheap and effective, but the gate is a heuristic and may miss quiet mating nets
- **`threshold` as a material integer** — becomes meaningless once the engine is authoritative; centipawn loss replaces it

---

## 4. Verdict

The prototype's **architecture survives the audit**; its **arithmetic has one hole**.

Three changes are required before any of this logic is carried forward:

1. **`quiesce()` must not stand pat in check** (§1.1) — a real, reachable soundness bug
2. **`quiesce()` must generate promotions** (§1.3) — it inverted the verdict on a winning queen sacrifice, calling it a 4-point blunder
3. **`line-opened` must split from `defender-left`** (§2.1) — the labels are the product; conflated labels give the wrong lesson

Both resolver bugs are the same root cause: **the move-generation set inside quiescence is too narrow.** It needs captures, promotions, and all evasions when in check.

Everything else is either directly reusable or a mechanical port to the v2 contracts.

**One meta-finding worth carrying into the test suite:** in this audit, bad test positions produced more false findings than the code did — seven of my probes were wrong versus two genuine defects. Synthetic tests must assert their own preconditions, or the suite becomes a source of confident errors rather than a defence against them.
