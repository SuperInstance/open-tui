//! Terminal-friendly report formatting.
//!
//! Produces output like:
//! ```text
//! Frame 847: 12ms total (budget: 16ms)
//!   StatusBar: 8ms (67%) for 80 cells — HOG, full redraw every tick
//!   Body: 3ms (25%) for 2000 cells
//!   2 findings: 1 warning, 1 hint
//! ```

use std::fmt;

use crate::profiler::RenderProfiler;

/// Formatter that wraps profiler data for display.
pub struct ReportFormatter<'a> {
    profiler: &'a RenderProfiler,
}

impl<'a> ReportFormatter<'a> {
    pub(crate) fn new(profiler: &'a RenderProfiler) -> Self {
        Self { profiler }
    }

    /// The underlying profiler (in case you need raw data).
    pub fn profiler(&self) -> &'a RenderProfiler {
        self.profiler
    }
}

impl fmt::Display for ReportFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let frame = self.profiler.last_frame();
        let total = self.profiler.last_frame_time().unwrap_or(std::time::Duration::ZERO);
        let budget = self.profiler.budget();
        let violations = self.profiler.last_violations();
        let findings = self.profiler.last_findings();
        let total_cells = self.profiler.last_frame_total_cells();

        let over_budget = !violations.is_empty();
        let budget_marker = if over_budget { "⚠ OVER BUDGET" } else { "✓" };

        writeln!(
            f,
            "Frame {frame}: {:.1}ms total ({budget_marker}, budget: {}ms) {} cells",
            total.as_secs_f64() * 1000.0,
            budget.max_render_time.as_millis(),
            total_cells,
        )?;

        // Per-widget breakdown (sorted by time descending)
        let history = self.profiler.history();
        if let Some(record) = history.last() {
            let mut widgets: Vec<_> = record.widget_times.iter().collect();
            widgets.sort_by(|a, b| b.1.cmp(&a.1));

            for (name, time, cells) in &widgets {
                let pct = if total.is_zero() {
                    0.0
                } else {
                    time.as_secs_f64() / total.as_secs_f64() * 100.0
                };
                let ms = time.as_secs_f64() * 1000.0;

                // Annotate with findings for this widget
                let annotations: Vec<_> = findings
                    .iter()
                    .filter(|f| match &f.category {
                        crate::detector::WasteCategory::Hog { widget, .. } => widget == name,
                        crate::detector::WasteCategory::FullRedrawForSmallChange { widget, .. } => {
                            widget == name
                        }
                        crate::detector::WasteCategory::SuspectedAllocation { widget, .. } => {
                            widget == name
                        }
                        _ => false,
                    })
                    .collect();

                let annotation_str = if annotations.is_empty() {
                    String::new()
                } else {
                    let labels: Vec<String> = annotations
                        .iter()
                        .map(|a| match &a.category {
                            crate::detector::WasteCategory::Hog { .. } => "HOG".to_string(),
                            crate::detector::WasteCategory::FullRedrawForSmallChange { .. } => {
                                "FULL-REDRAW".to_string()
                            }
                            crate::detector::WasteCategory::SuspectedAllocation { .. } => {
                                "ALLOCATES".to_string()
                            }
                            _ => String::new(),
                        })
                        .collect();
                    format!(" — {}", labels.join(", "))
                };

                writeln!(
                    f,
                    "  {name}: {ms:.1}ms ({pct:.0}%) for {cells} cells{annotation_str}"
                )?;
            }
        }

        // Violations
        for v in violations {
            writeln!(f, "  ⚠ {v}")?;
        }

        // Findings summary
        if !findings.is_empty() {
            let warnings = findings
                .iter()
                .filter(|f| matches!(f.severity, crate::detector::Severity::Warning))
                .count();
            let critical = findings
                .iter()
                .filter(|f| matches!(f.severity, crate::detector::Severity::Critical))
                .count();
            let hints = findings.len() - warnings - critical;

            writeln!(
                f,
                "  {count} findings: {critical} critical, {warnings} warnings, {hints} hints",
                count = findings.len(),
            )?;

            for finding in findings {
                writeln!(f, "    [{}] {}", finding.severity, finding.category)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::FrameBudget;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn report_formats_nicely() {
        let mut p = RenderProfiler::new(FrameBudget::for_60fps());

        p.begin_frame();
        p.begin_widget("StatusBar");
        thread::sleep(Duration::from_millis(2));
        p.end_widget(80);
        p.begin_widget("Body");
        p.end_widget(2000);
        p.end_frame();

        let report = p.report().to_string();
        assert!(report.contains("Frame 1"));
        assert!(report.contains("StatusBar"));
        assert!(report.contains("Body"));
        assert!(report.contains("cells"));
    }

    #[test]
    fn report_with_violations() {
        let budget = FrameBudget::new(Duration::from_millis(1), 10_000, 5);
        let mut p = RenderProfiler::new(budget);

        p.begin_frame();
        p.begin_widget("SlowWidget");
        thread::sleep(Duration::from_millis(5));
        p.end_widget(100);
        p.end_frame();

        let report = p.report().to_string();
        assert!(report.contains("OVER BUDGET"));
    }
}
