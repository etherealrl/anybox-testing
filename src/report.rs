use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Skipped,
    NotAvailable,
    /// something we just want to see, there's no right answer for it yet
    Noted,
}

impl Outcome {
    const ALL: [Self; 5] = [
        Self::Passed,
        Self::Failed,
        Self::Skipped,
        Self::NotAvailable,
        Self::Noted,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::NotAvailable => "not available",
            Self::Noted => "noted",
        }
    }
}

#[derive(Default)]
pub struct Report {
    body: String,
    counts: [usize; 5],
}

/// seconds since 1970, the same clock the trace lines use, so a check can be found in the trace by its time
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

impl Report {
    pub fn section(&mut self, title: &str) {
        let _ = write!(self.body, "\n## {title}\n\n");
    }

    pub fn line(&mut self, text: &str) {
        let _ = writeln!(self.body, "{text}");
    }

    pub fn check(&mut self, name: &str, outcome: Outcome, detail: &str) {
        self.counts[Outcome::ALL.iter().position(|o| *o == outcome).unwrap()] += 1;
        let _ = write!(self.body, "[{:.3}] {name}: {}", now(), outcome.label());
        if detail.is_empty() {
            self.body.push('\n');
        } else {
            // a multi line detail stays readable under its check
            let _ = writeln!(self.body, ", {}", detail.replace('\n', "\n    "));
        }
    }

    pub fn failed(&self) -> usize {
        self.counts[1]
    }

    pub fn render(&self, header: &str, trace: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{header}");
        let summary: Vec<String> = Outcome::ALL
            .iter()
            .zip(self.counts)
            .map(|(outcome, count)| format!("{count} {}", outcome.label()))
            .collect();
        let _ = writeln!(out, "{}", summary.join(", "));
        out.push_str(&self.body);
        let _ = write!(out, "\n## trace\n\n");
        if trace.is_empty() {
            out.push_str("nothing went over a traced port\n");
        } else {
            out.push_str(trace);
        }
        out
    }
}

#[cfg(test)]
mod tests;
