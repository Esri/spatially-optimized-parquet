//! Renders optional live write counts at the CLI boundary.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spatial::{WriteProgress, WriteReporter};

const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub(crate) struct StdoutWriteReporter {
  state: Arc<Mutex<ReporterState>>,
}

struct ReporterState {
  live: bool,
  highest_count: u64,
  total_count: u64,
  last_rendered_at: Option<Instant>,
}

impl StdoutWriteReporter {
  pub(crate) fn new(live: bool) -> Self {
    Self {
      state: Arc::new(Mutex::new(ReporterState {
        live,
        highest_count: 0,
        total_count: 0,
        last_rendered_at: None,
      })),
    }
  }

  pub(crate) fn finish(&self, rows_written: u64, total_rows: u64) {
    let mut state = self
      .state
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.highest_count = state.highest_count.max(rows_written);
    state.total_count = total_rows;
    Self::render(rows_written, total_rows, true);
  }

  fn render(rows_written: u64, total_rows: u64, finished: bool) {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    Self::render_to(&mut stdout, rows_written, total_rows, finished);
  }

  fn render_to(writer: &mut dyn Write, rows_written: u64, total_rows: u64, finished: bool) {
    if finished {
      let _ = writeln!(writer, "\rWrote {rows_written}/{total_rows} features");
    } else {
      let _ = write!(writer, "\rWrote {rows_written}/{total_rows} features");
    }
    let _ = writer.flush();
  }
}

impl WriteReporter for StdoutWriteReporter {
  fn report(&self, progress: WriteProgress) {
    let mut state = self
      .state
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if progress.rows_written() < state.highest_count {
      return;
    }
    state.highest_count = progress.rows_written();
    state.total_count = progress.total_rows();
    if !state.live {
      return;
    }
    let now = Instant::now();
    if state
      .last_rendered_at
      .is_some_and(|last_rendered_at| now.duration_since(last_rendered_at) < REFRESH_INTERVAL)
    {
      return;
    }
    state.last_rendered_at = Some(now);
    Self::render(state.highest_count, state.total_count, false);
  }
}
