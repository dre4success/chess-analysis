//! A bounded UCI session. Every error poisons the session and reaps the child.
use crate::{
    evaluation::{Evaluation, ReviewSide},
    review::legal_move,
};
use shakmaty::{Chess, Color, EnPassantMode, Position, fen::Fen};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("engine timed out")]
    Timeout,
    #[error("engine closed output or session is no longer usable")]
    Closed,
    #[error("invalid engine output: {0}")]
    Protocol(String),
    #[error("node budget must be positive")]
    ZeroNodes,
    #[error("terminal positions must not be sent to the engine")]
    Terminal,
}
#[derive(Debug, Clone, Copy)]
pub struct Nodes(std::num::NonZeroU64);
impl Nodes {
    pub fn new(n: u64) -> Result<Self, EngineError> {
        Ok(Self(
            std::num::NonZeroU64::new(n).ok_or(EngineError::ZeroNodes)?,
        ))
    }
    pub fn get(self) -> u64 {
        self.0.get()
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub evaluation: Evaluation,
    pub best_uci: String,
    pub pv: Vec<String>,
}
pub trait PositionEngine {
    fn analyse(
        &mut self,
        position: &Chess,
        user: Color,
        nodes: Nodes,
    ) -> Result<Analysis, EngineError>;
}
pub struct UciEngine {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Result<String, std::io::Error>>,
    reader: Option<JoinHandle<()>>,
    timeout: Duration,
    alive: bool,
    pub name: String,
    pub options: BTreeMap<String, String>,
}
impl UciEngine {
    pub fn spawn(path: &Path, timeout: Duration) -> Result<Self, EngineError> {
        Self::command(Command::new(path), timeout)
    }
    pub fn command(mut command: Command, timeout: Duration) -> Result<Self, EngineError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().ok_or(EngineError::Closed)?;
        let (tx, output) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                // Bound individual lines even if a broken process never writes a newline.
                let mut bytes = Vec::new();
                let result = std::io::Read::take(&mut reader, 65_537).read_until(b'\n', &mut bytes);
                match result {
                    Ok(0) => break,
                    Ok(_) if bytes.len() > 65_536 => {
                        let _ = tx.send(Err(std::io::Error::other("UCI line exceeds 64 KiB")));
                        break;
                    }
                    Ok(_) => {
                        if tx
                            .send(
                                String::from_utf8(bytes)
                                    .map(|s| s.trim().to_owned())
                                    .map_err(std::io::Error::other),
                            )
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        break;
                    }
                }
            }
        });
        let mut engine = Self {
            child,
            input,
            output,
            reader: Some(reader),
            timeout,
            alive: true,
            name: String::new(),
            options: BTreeMap::new(),
        };
        engine.send("uci")?;
        let deadline = Instant::now() + timeout;
        loop {
            let line = engine.line(deadline)?;
            if line == "uciok" {
                break;
            }
            if let Some(name) = line.strip_prefix("id name ") {
                engine.name = name.to_owned();
            }
            if let Some(option) = line.strip_prefix("option name ")
                && let Some((name, rest)) = option.split_once(" type ")
            {
                engine.options.insert(name.into(), rest.into());
            }
        }
        engine.ready()?;
        Ok(engine)
    }
    fn send(&mut self, text: &str) -> Result<(), EngineError> {
        if !self.alive {
            return Err(EngineError::Closed);
        }
        let input = self.input.as_mut().ok_or(EngineError::Closed)?;
        writeln!(input, "{text}")?;
        input.flush()?;
        Ok(())
    }
    fn line(&self, deadline: Instant) -> Result<String, EngineError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(EngineError::Timeout)?;
        match self.output.recv_timeout(remaining) {
            Ok(line) => Ok(line?),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(EngineError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(EngineError::Closed),
        }
    }
    fn ready(&mut self) -> Result<(), EngineError> {
        self.send("isready")?;
        let deadline = Instant::now() + self.timeout;
        while self.line(deadline)? != "readyok" {}
        Ok(())
    }
    pub fn configure(&mut self) -> Result<(), EngineError> {
        let result = (|| {
            for (name, value) in [("Threads", "1"), ("Hash", "64"), ("MultiPV", "1")] {
                if !self.options.contains_key(name) {
                    return Err(EngineError::Protocol(format!(
                        "missing required option {name}"
                    )));
                }
                self.send(&format!("setoption name {name} value {value}"))?;
            }
            if !self.options.contains_key("Clear Hash") {
                return Err(EngineError::Protocol("missing Clear Hash option".into()));
            }
            self.ready()
        })();
        if result.is_err() {
            self.shutdown();
        }
        result
    }
    fn search(
        &mut self,
        position: &Chess,
        user: Color,
        nodes: Nodes,
    ) -> Result<Analysis, EngineError> {
        if position.is_game_over() {
            return Err(EngineError::Terminal);
        }
        self.send("setoption name Clear Hash")?;
        self.ready()?;
        self.send(&format!(
            "position fen {}",
            Fen::from_position(position, EnPassantMode::Legal)
        ))?;
        self.send(&format!("go nodes {}", nodes.get()))?;
        let deadline = Instant::now() + self.timeout;
        let mut latest = None;
        loop {
            let line = self.line(deadline)?;
            if line.starts_with("info ") {
                if let Some(info) = parse_info(&line, position.turn() == user)? {
                    latest = Some(info);
                }
            } else if let Some(rest) = line.strip_prefix("bestmove ") {
                let best = rest
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| EngineError::Protocol(line.clone()))?;
                legal_move(position, best).map_err(EngineError::Protocol)?;
                let (evaluation, pv) =
                    latest.ok_or_else(|| EngineError::Protocol("no exact score with PV".into()))?;
                if pv.first().map(String::as_str) != Some(best) {
                    return Err(EngineError::Protocol("bestmove does not match PV".into()));
                }
                let mut replay = position.clone();
                for uci in &pv {
                    let mv = legal_move(&replay, uci).map_err(EngineError::Protocol)?;
                    replay.play_unchecked(mv);
                }
                return Ok(Analysis {
                    evaluation,
                    best_uci: best.to_owned(),
                    pv,
                });
            }
        }
    }
    fn shutdown(&mut self) {
        if self.alive {
            let _ = self.send("quit");
            self.input.take();
            // kill + wait guarantees cleanup even when the engine ignores quit.
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.alive = false;
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
}
impl PositionEngine for UciEngine {
    fn analyse(
        &mut self,
        position: &Chess,
        user: Color,
        nodes: Nodes,
    ) -> Result<Analysis, EngineError> {
        let result = self.search(position, user, nodes);
        if result.is_err() {
            self.shutdown();
        }
        result
    }
}
impl Drop for UciEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn parse_info(
    line: &str,
    user_turn: bool,
) -> Result<Option<(Evaluation, Vec<String>)>, EngineError> {
    let words: Vec<_> = line.split_whitespace().collect();
    if words.get(1) == Some(&"string")
        || words.contains(&"lowerbound")
        || words.contains(&"upperbound")
    {
        return Ok(None);
    }
    if let Some(i) = words.iter().position(|w| *w == "multipv")
        && words.get(i + 1) != Some(&"1")
    {
        return Ok(None);
    }
    let Some(i) = words.iter().position(|w| *w == "score") else {
        return Ok(None);
    };
    let bad = || EngineError::Protocol(line.into());
    let n: i32 = words
        .get(i + 2)
        .ok_or_else(bad)?
        .parse()
        .map_err(|_| bad())?;
    let evaluation = match words.get(i + 1) {
        Some(&"cp") => Evaluation::centipawns(if user_turn {
            n
        } else {
            n.checked_neg().ok_or_else(bad)?
        }),
        Some(&"mate") => Evaluation::mate(
            if (n > 0) == user_turn {
                ReviewSide::User
            } else {
                ReviewSide::Opponent
            },
            u16::try_from(n.unsigned_abs()).map_err(|_| bad())?,
        ),
        _ => return Err(bad()),
    };
    let Some(p) = words.iter().position(|w| *w == "pv") else {
        return Ok(None);
    };
    Ok(Some((
        evaluation,
        words[p + 1..].iter().map(|w| (*w).into()).collect(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mate_and_cp_pov_and_malformed_output() {
        assert_eq!(
            parse_info("info score mate -2 pv e2e4", false)
                .unwrap()
                .unwrap()
                .0,
            Evaluation::mate(ReviewSide::User, 2)
        );
        assert_eq!(
            parse_info("info score mate 2 pv e2e4", false)
                .unwrap()
                .unwrap()
                .0,
            Evaluation::mate(ReviewSide::Opponent, 2)
        );
        assert_eq!(
            parse_info("info score cp 20 pv e2e4", false)
                .unwrap()
                .unwrap()
                .0,
            Evaluation::centipawns(-20)
        );
        assert!(parse_info("info score cp garbage pv e2e4", true).is_err());
        assert!(
            parse_info("info score cp 20 lowerbound pv e2e4", true)
                .unwrap()
                .is_none()
        );
    }
    #[cfg(unix)]
    fn fake(body: &str) -> UciEngine {
        let mut cmd = Command::new("python3");
        cmd.args(["-u", "-c", &format!("import sys,time\nfor line in sys.stdin:\n line=line.strip()\n if line=='uci': print('uciok',flush=True)\n elif line=='isready': print('readyok',flush=True)\n elif line=='quit': break\n elif line.startswith('go '):\n  {body}\n")]);
        UciEngine::command(cmd, Duration::from_millis(500)).unwrap()
    }
    #[test]
    #[cfg(unix)]
    fn fake_search_and_timeout_reaps_child() {
        let mut engine = fake(
            "print('info depth 1 score cp 20 pv e2e4 e7e5',flush=True); print('bestmove e2e4',flush=True)",
        );
        let result = engine
            .analyse(&Chess::default(), Color::White, Nodes::new(100).unwrap())
            .unwrap();
        assert_eq!(result.best_uci, "e2e4");
        let mut engine = fake("time.sleep(10)");
        assert!(matches!(
            engine.analyse(&Chess::default(), Color::White, Nodes::new(100).unwrap()),
            Err(EngineError::Timeout)
        ));
        assert!(engine.child.try_wait().unwrap().is_some());
    }
    #[test]
    #[cfg(unix)]
    fn illegal_engine_pv_is_rejected_and_reaped() {
        let mut engine = fake(
            "print('info score cp 20 pv e2e4 e2e3',flush=True); print('bestmove e2e4',flush=True)",
        );
        assert!(matches!(
            engine.analyse(&Chess::default(), Color::White, Nodes::new(1).unwrap()),
            Err(EngineError::Protocol(_))
        ));
        assert!(engine.child.try_wait().unwrap().is_some());
    }
}
