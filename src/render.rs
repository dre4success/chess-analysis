use crate::{
    evaluation::{Evaluation, ReviewSide},
    patterns::lesson,
    review::{Classification, ValidatedReview},
};
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn md(text: &str) -> String {
    let mut s = String::new();
    for ch in text.chars() {
        if "\\`*_{}[]|".contains(ch) {
            s.push('\\');
        }
        if ch == '\n' || ch == '\r' {
            s.push(' ');
        } else {
            s.push(ch);
        }
    }
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
pub fn label(c: Classification) -> String {
    serde_json::to_value(c)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
pub fn evaluation(e: Evaluation) -> String {
    match e {
        Evaluation::Centipawns { value, .. } => format!("{:+.2}", f64::from(value) / 100.0),
        Evaluation::Mate { winner, moves, .. } => format!(
            "{} mate in {moves}",
            if winner == ReviewSide::User {
                "your"
            } else {
                "opponent"
            }
        ),
    }
}
pub fn digest(validated: &ValidatedReview) -> String {
    let r = validated.as_review();
    let mut out = format!(
        "# Chess review: {}\n\n{} completed games · {} · verified with {}. Evaluations are from your point of view.\n\n",
        md(&r.user),
        r.games.len(),
        md(&r.generated),
        md(&r.engine.version)
    );
    if r.games.len() < 10 {
        out.push_str("Small sample: treat these as observations, not established habits.\n\n");
    }
    out.push_str("## Practice priorities\n\n");
    if r.patterns.is_empty() {
        out.push_str("No mistakes met the configured confirmation threshold. This does not prove perfect play.\n\n");
    }
    for p in r.patterns.iter().take(3) {
        out.push_str(&format!(
            "- **{}**: {}/{} games, {} occurrences. {}\n",
            label(p.classification),
            p.games_affected,
            r.games.len(),
            p.occurrences,
            lesson(p.classification)
        ));
    }
    out.push_str("\n## Checked examples\n\n");
    for p in r.patterns.iter().take(3) {
        for example in p.example_refs.iter().take(2) {
            if let Some((g, f)) = r.games.iter().find(|g| g.url == example.url).and_then(|g| {
                g.findings
                    .iter()
                    .find(|f| f.ply == example.ply)
                    .map(|f| (g, f))
            }) {
                out.push_str(&format!("- {} · ply {} · {} → better {} · {} → {}. {}\n  FEN: `{}`\n  Best line (up to 10 plies): `{}`\n", md(&g.url), f.ply, md(&f.actual_san), md(f.best_san.as_deref().unwrap_or("unknown")), evaluation(f.eval_before), evaluation(f.eval_after), md(&f.explanation), f.before_fen, f.principal_variation_uci.iter().take(10).cloned().collect::<Vec<_>>().join(" ")));
            }
        }
    }
    let clocks: Vec<_> = r.games.iter().filter_map(|g| g.clock_used_pct).collect();
    out.push_str(&format!(
        "\nClock use available for {}/{} games",
        clocks.len(),
        r.games.len()
    ));
    if !clocks.is_empty() {
        out.push_str(&format!(
            ": mean {:.1}% of base time plus earned increments spent",
            clocks.iter().sum::<f64>() / clocks.len() as f64
        ));
    }
    out.push_str(". Missing clocks are unknown. Phase bands use ply count, not a positional endgame classifier.\n\nEngine verdicts are checked; semantic labels still require human spot-checking before release. All findings and breakdowns are in review.json.\n");
    out
}
fn board(fen: &str) -> String {
    let mut out =
        String::from("<table class=board aria-label=\"Chess position, White at bottom\">");
    let chars = "KQRBNPkqrbnp";
    let pieces: Vec<_> = "♔♕♖♗♘♙♚♛♜♝♞♟".chars().collect();
    for (r, rank) in fen
        .split_whitespace()
        .next()
        .unwrap_or("")
        .split('/')
        .enumerate()
    {
        out.push_str("<tr>");
        let mut file = 0;
        for c in rank.chars() {
            let count = c.to_digit(10).unwrap_or(1);
            for _ in 0..count {
                let piece = chars.find(c).map(|i| pieces[i]).unwrap_or(' ');
                out.push_str(&format!(
                    "<td class=\"{}\">{piece}</td>",
                    if (r + file) % 2 == 0 { "light" } else { "dark" }
                ));
                file += 1;
            }
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}
pub fn html(validated: &ValidatedReview) -> String {
    let r = validated.as_review();
    let mut out = String::from(
        "<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>Chess review</title><style>body{font:17px/1.55 system-ui;margin:0;background:#f4f1e9;color:#202a26}main{max-width:1060px;margin:auto;padding:32px}h1{font-size:42px}article,section{background:white;padding:24px;border-radius:12px;margin:20px 0}code{overflow-wrap:anywhere;font-size:13px}.boards{display:flex;gap:24px;flex-wrap:wrap}.board{border-collapse:collapse;font-size:27px;line-height:1}.board td{width:35px;height:35px;text-align:center}.light{background:#eee3c9}.dark{background:#9aad91}summary{cursor:pointer}small{color:#58655e}a{color:#165e47}table.stats{border-collapse:collapse;width:100%}.stats td,.stats th{text-align:left;padding:8px;border-bottom:1px solid #ddd}@media(max-width:550px){main{padding:12px}article{padding:14px}h1{font-size:30px}}</style><main>",
    );
    out.push_str(&format!("<h1>Review for {}</h1><p>{} completed games · {} · {}</p><p>Evaluations are from your point of view. Labels need human spot-checking before release.</p>", escape(&r.user), r.games.len(), escape(&r.generated), escape(&r.engine.version)));
    if r.games.len() < 10 {
        out.push_str("<p>Small sample: these are observations, not established habits.</p>");
    }
    out.push_str("<section><h2>Practice priorities</h2>");
    if r.patterns.is_empty() {
        out.push_str("<p>No mistakes met the confirmation threshold.</p>");
    }
    for p in &r.patterns {
        out.push_str(&format!("<h3>{}: {}/{} games</h3><p>{}</p><details><summary>Breakdowns (small groups are descriptive only)</summary><table class=stats><tr><th>Group</th><th>Games affected / observed</th><th>Occurrences</th></tr>", label(p.classification), p.games_affected, r.games.len(), lesson(p.classification)));
        for b in &p.breakdowns {
            out.push_str(&format!(
                "<tr><td>{}: {}</td><td>{}/{}</td><td>{}</td></tr>",
                escape(&b.dimension),
                escape(&b.value),
                b.games_affected,
                b.games_reviewed,
                b.occurrences
            ));
        }
        out.push_str("</table></details>");
    }
    out.push_str("</section>");
    for game in &r.games {
        out.push_str(&format!(
            "<section><h2>{} · vs {}</h2><p>{} · {:?} · {:?}</p>",
            escape(&game.date),
            escape(&game.opponent.name),
            escape(&game.url),
            game.colour,
            game.result
        ));
        if game.findings.is_empty() {
            out.push_str("<p>No confirmed findings.</p>");
        }
        for f in &game.findings {
            out.push_str(&format!("<article><h3>Ply {}: {} · {}</h3><p>{}</p><p>Evaluation {} → {}. Better: <strong>{}</strong>.</p><div class=boards><div><p>Before</p>{}</div><div><p>After {}</p>{}</div></div><details><summary>Reproducible evidence</summary><p>Before FEN: <code>{}</code></p><p>Actual UCI: <code>{}</code></p><p>After FEN: <code>{}</code></p><p>Best line from before: <code>{}</code></p><p>Also matched: {}</p></details></article>", f.ply, escape(&f.actual_san), label(f.classification), escape(&f.explanation), evaluation(f.eval_before), evaluation(f.eval_after), escape(f.best_san.as_deref().unwrap_or("unknown")),board(&f.before_fen),escape(&f.actual_san),board(&f.after_fen),escape(&f.before_fen),escape(&f.actual_uci),escape(&f.after_fen),escape(&f.principal_variation_uci.join(" ")),f.also_matched.iter().map(|c|label(*c)).collect::<Vec<_>>().join(", ")));
        }
        out.push_str("</section>");
    }
    out.push_str("<footer>Generated entirely from validated review.json. No network or engine is used to render this report.</footer></main></html>");
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renderers_escape_imported_metadata_and_work_from_round_tripped_json() {
        let mut r = crate::review::tests::sample_review();
        r.user = "<script>alert(1)</script>".into();
        r.games[0].opponent.name = "<img src=x onerror=alert(1)>".into();
        let r = crate::review::validate(
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap(),
        )
        .unwrap();
        let output = html(&r);
        assert!(!output.contains("<script>"));
        assert!(!output.contains("<img"));
        assert!(output.contains("&lt;script&gt;"));
        assert!(digest(&r).contains("line-opened"));
    }
    #[test]
    fn board_expands_empty_squares_and_preserves_orientation() {
        let output = board("7k/P7/8/8/8/8/8/7K w - - 0 1");
        assert_eq!(output.matches("<tr>").count(), 8);
        assert_eq!(output.matches("<td ").count(), 64);
        assert!(output.find('♚').unwrap() < output.find('♙').unwrap());
        assert!(output.find('♙').unwrap() < output.find('♔').unwrap());
    }
}
