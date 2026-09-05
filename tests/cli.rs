use std::process::Command;
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chess-review"))
}
#[test]
fn unfinished_input_is_rejected_before_engine_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    let pgn = dir.path().join("live.pgn");
    std::fs::write(
        &pgn,
        "[White \"a\"]\n[Black \"b\"]\n[Result \"*\"]\n\n1. e4 *",
    )
    .unwrap();
    let output = cli()
        .args(["review", "a", "--pgn"])
        .arg(&pgn)
        .args(["--engine", "/does/not/exist", "--output"])
        .arg(dir.path().join("out"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("engine was not opened"), "{error}");
    assert!(!dir.path().join("out").exists());
}
#[test]
#[cfg(unix)]
fn fake_engine_review_validate_and_render_end_to_end() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let pgn = dir.path().join("finished.pgn");
    std::fs::write(
        &pgn,
        "[White \"a\"]\n[Black \"<script>\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0",
    )
    .unwrap();
    let engine = dir.path().join("fake-engine");
    std::fs::write(&engine,r#"#!/usr/bin/env python3
import sys
white=True
for line in sys.stdin:
 line=line.strip()
 if line=='uci':
  print('id name Test Engine')
  for name in ['Threads','Hash','MultiPV']: print('option name '+name+' type spin default 1 min 1 max 64')
  print('option name Clear Hash type button')
  print('uciok',flush=True)
 elif line=='isready': print('readyok',flush=True)
 elif line.startswith('position fen '): white=' w ' in line
 elif line.startswith('go '):
  move='d2d4' if white else 'e7e5'
  print('info score cp '+('0' if white else '400')+' pv '+move,flush=True)
  print('bestmove '+move,flush=True)
 elif line=='quit': break
"#).unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = dir.path().join("out");
    let result = cli()
        .args(["review", "a", "--pgn"])
        .arg(&pgn)
        .arg("--engine")
        .arg(&engine)
        .arg("--output")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json = out.join("review.json");
    assert!(cli().arg("validate").arg(&json).status().unwrap().success());
    std::fs::remove_file(engine).unwrap(); // rendering must not need an engine
    assert!(cli().arg("render").arg(&json).status().unwrap().success());
    let html = std::fs::read_to_string(out.join("report.html")).unwrap();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    let review = chess_review::review::read(&json).unwrap();
    assert_eq!(review.as_review().games[0].findings.len(), 1);
    assert_eq!(
        std::fs::read_to_string(out.join("digest.md")).unwrap(),
        chess_review::render::digest(&review)
    );
}
#[test]
fn recorded_source_fixture_passes_completion_guard() {
    let month: chess_review::fetch::Month =
        serde_json::from_slice(include_bytes!("fixtures/rapid-12.json")).unwrap();
    assert_eq!(month.games.len(), 12);
    for game in month.games {
        chess_review::fetch::verify(&game, 2_000_000_000).unwrap();
    }
}

#[test]
fn recorded_review_preserves_verdicts_patterns_and_all_legality_invariants() {
    let r: chess_review::review::Review =
        serde_json::from_slice(include_bytes!("fixtures/rapid-12-review.json")).unwrap();
    let validated = chess_review::review::validate(r).unwrap();
    let r = validated.as_review();
    assert_eq!(r.games.len(), 12);
    assert_eq!(r.patterns, chess_review::patterns::aggregate(&r.games));
    for game in &r.games {
        assert!(game.findings.len() <= 3);
        for finding in &game.findings {
            assert!(
                chess_review::analysis::loss(finding.eval_before, finding.eval_after)
                    >= u64::from(r.engine.threshold_cp)
            );
            assert_ne!(
                finding.best_uci.as_deref(),
                Some(finding.actual_uci.as_str())
            );
        }
    }
    let trade = r
        .games
        .iter()
        .find(|g| g.url.ends_with("173956430012"))
        .unwrap()
        .findings
        .iter()
        .find(|f| f.ply == 52)
        .unwrap();
    assert_eq!(
        trade.classification,
        chess_review::review::Classification::EngineVerifiedMistake
    );
    assert!(
        !trade
            .also_matched
            .contains(&chess_review::review::Classification::LineOnto)
    );
}
