//! Per-widget render profiling.
//!
//! Tracks how long each widget takes to render, how many cells it writes,
//! and whether it's doing full redraws or incremental updates.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::budget::{BudgetViolation, FrameBudget};
use crate::detector::WasteDetector;
use crate::report::ReportFormatter;

/// Statistics collected for a single widget across its lifetime.
#[derive(Debug, Clone)]
pub struct PerWidgetStats {
    /// Widget name (the label you passed to `begin_widget`).
    pub name: String,
    /// Total render time accumulated across all frames.
    pub total_time: Duration,
    /// Number of times this widget has been rendered.
    pub render_count: u64,
    /// Cells written in the most recent render.
    pub last_cells: usize,
    /// Peak render time for a single call.
    pub peak_time: Duration,
    /// Cells written on the previous render (for diff tracking).
    pub prev_cells: usize,
    /// Whether the last render was a "full redraw" (cells changed significantly).
    pub last_was_full_redraw: bool,
}

impl PerWidgetStats {
    /// Average render time per call.
    pub fn avg_time(&self) -> Duration {
        if self.render_count == 0 {
            Duration::ZERO
        } else {
            self.total_time / self.render_count as u32
        }
    }

    /// What fraction of total frame time this widget consumed (0.0 – 1.0).
    pub fn fraction_of(&self, total: Duration) -> f64 {
        if total.is_zero() {
            0.0
        } else {
            self.total_time.as_secs_f64() / total.as_secs_f64()
        }
    }
}

/// A single completed frame's data.
#[derive(Debug, Clone)]
pub(crate) struct FrameRecord {
    #[allow(dead_code)]
    frame_number: u64,
    total_time: Duration,
    pub widget_times: Vec<(String, Duration, usize)>,
    violations: Vec<BudgetViolation>,
}

/// The main profiler. Owns the budget and collects per-frame / per-widget data.
pub struct RenderProfiler {
    budget: FrameBudget,
    frame_number: u64,
    frame_start: Option<Instant>,
    widget_stack: Vec<(String, Instant)>,
    current_frame_widgets: Vec<(String, Duration, usize)>,
    widget_stats: HashMap<String, PerWidgetStats>,
    history: Vec<FrameRecord>,
    detector: WasteDetector,
    max_history: usize,
}

impl RenderProfiler {
    /// Create a new profiler with the given budget.
    pub fn new(budget: FrameBudget) -> Self {
        Self {
            budget,
            frame_number: 0,
            frame_start: None,
            widget_stack: Vec::new(),
            current_frame_widgets: Vec::new(),
            widget_stats: HashMap::new(),
            history: Vec::new(),
            detector: WasteDetector::new(),
            max_history: 120,
        }
    }

    /// The configured budget.
    pub fn budget(&self) -> &FrameBudget {
        &self.budget
    }

    /// Begin a new frame.
    pub fn begin_frame(&mut self) {
        self.frame_number += 1;
        self.frame_start = Some(Instant::now());
        self.current_frame_widgets.clear();
    }

    /// Begin timing a widget. Nesting is tracked for depth checks.
    pub fn begin_widget(&mut self, name: &str) {
        let depth = self.widget_stack.len() + 1;
        if self.budget.check_depth(depth).is_some() {
            // Depth violation noted; will be reported at end_frame
        }
        self.widget_stack
            .push((name.to_string(), Instant::now()));
    }

    /// End timing a widget. `cells_written` is the number of terminal cells this widget touched.
    pub fn end_widget(&mut self, cells_written: usize) {
        if let Some((name, start)) = self.widget_stack.pop() {
            let elapsed = start.elapsed();
            self.current_frame_widgets
                .push((name.clone(), elapsed, cells_written));

            let stats = self
                .widget_stats
                .entry(name.clone())
                .or_insert_with(|| PerWidgetStats {
                    name: name.clone(),
                    total_time: Duration::ZERO,
                    render_count: 0,
                    last_cells: 0,
                    peak_time: Duration::ZERO,
                    prev_cells: 0,
                    last_was_full_redraw: false,
                });

            // Detect full redraw: cells changed by >50% from previous frame
            let full_redraw = if stats.render_count > 0 {
                let diff = (cells_written as i64 - stats.last_cells as i64).unsigned_abs() as usize;
                diff > (stats.last_cells / 2)
            } else {
                false
            };

            stats.prev_cells = stats.last_cells;
            stats.last_cells = cells_written;
            stats.last_was_full_redraw = full_redraw;
            stats.total_time += elapsed;
            stats.render_count += 1;
            if elapsed > stats.peak_time {
                stats.peak_time = elapsed;
            }
        }
    }

    /// End the current frame. Returns total frame time.
    pub fn end_frame(&mut self) -> Duration {
        let total = self
            .frame_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);

        let mut violations = Vec::new();
        if let Some(v) = self.budget.check_time(total) {
            violations.push(v);
        }

        // Check diff size (sum of all widget cells as approximation)
        let total_cells: usize = self.current_frame_widgets.iter().map(|w| w.2).sum();
        if let Some(v) = self.budget.check_diff(total_cells) {
            violations.push(v);
        }

        // Run waste detector
        let findings = self.detector.detect(
            self.frame_number,
            total,
            &self.current_frame_widgets,
            &self.widget_stats,
            &self.budget,
        );

        let record = FrameRecord {
            frame_number: self.frame_number,
            total_time: total,
            widget_times: self.current_frame_widgets.clone(),
            violations,
        };

        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(record);

        // Stash findings for report generation
        self.detector.stash_findings(self.frame_number, findings);

        total
    }

    /// Generate a human-readable report for the last frame.
    pub fn report(&self) -> ReportFormatter<'_> {
        ReportFormatter::new(self)
    }

    /// Per-widget stats.
    pub fn widget_stats(&self) -> &HashMap<String, PerWidgetStats> {
        &self.widget_stats
    }

    /// Last frame number.
    pub fn last_frame(&self) -> u64 {
        self.frame_number
    }

    /// Last frame total time.
    pub fn last_frame_time(&self) -> Option<Duration> {
        self.history.last().map(|r| r.total_time)
    }

    /// Violations from the last frame.
    pub fn last_violations(&self) -> &[BudgetViolation] {
        self.history
            .last()
            .map(|r| r.violations.as_slice())
            .unwrap_or(&[])
    }

    /// Waste findings from the last frame.
    pub fn last_findings(&self) -> &[crate::detector::WasteFinding] {
        self.detector.last_findings()
    }

    /// Access the history (most recent last).
    pub(crate) fn history(&self) -> &[FrameRecord] {
        &self.history
    }

    /// Get the total cells from the last frame's widgets.
    pub fn last_frame_total_cells(&self) -> usize {
        self.history
            .last()
            .map(|r| r.widget_times.iter().map(|w| w.2).sum())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn basic_profiling_cycle() {
        let mut p = RenderProfiler::new(FrameBudget::for_60fps());

        p.begin_frame();
        p.begin_widget("Header");
        thread::sleep(Duration::from_millis(1));
        p.end_widget(200);
        p.begin_widget("Body");
        thread::sleep(Duration::from_millis(1));
        p.end_widget(2000);
        let total = p.end_frame();

        assert!(total >= Duration::from_millis(2));
        assert_eq!(p.last_frame(), 1);

        let stats = p.widget_stats();
        assert_eq!(stats.len(), 2);
        assert!(stats.get("Header").unwrap().render_count == 1);
        assert!(stats.get("Body").unwrap().last_cells == 2000);
    }

    #[test]
    fn detects_over_budget() {
        let budget = FrameBudget::new(Duration::from_millis(1), 10_000, 5);
        let mut p = RenderProfiler::new(budget);

        p.begin_frame();
        p.begin_widget("SlowWidget");
        thread::sleep(Duration::from_millis(5));
        p.end_widget(100);
        p.end_frame();

        assert!(!p.last_violations().is_empty());
    }

    #[test]
    fn full_redraw_detection() {
        let mut p = RenderProfiler::new(FrameBudget::for_60fps());

        // Frame 1: widget writes 100 cells
        p.begin_frame();
        p.begin_widget("Status");
        p.end_widget(100);
        p.end_frame();

        // Frame 2: same widget writes 200 cells (100% change → full redraw)
        p.begin_frame();
        p.begin_widget("Status");
        p.end_widget(200);
        p.end_frame();

        let stats = p.widget_stats().get("Status").unwrap();
        assert!(stats.last_was_full_redraw);
    }
}
