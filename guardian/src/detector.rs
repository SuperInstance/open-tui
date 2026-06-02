//! Waste detection.
//!
//! Identifies common TUI rendering anti-patterns:
//! - Full-screen redraws for tiny changes
//! - Excessively nested layouts
//! - Widgets that appear to allocate on every frame (heuristic)

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use crate::budget::FrameBudget;
use crate::profiler::PerWidgetStats;

/// What kind of waste was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasteCategory {
    /// A widget touched nearly every cell but likely only needed to update a few.
    FullRedrawForSmallChange {
        widget: String,
        cells_written: usize,
        estimated_needed: usize,
    },
    /// Layout nesting exceeds the configured depth limit.
    DeepNesting {
        depth: usize,
        limit: usize,
    },
    /// A widget's render time is suspiciously high for the number of cells,
    /// suggesting per-frame allocation (String, Vec, etc.).
    SuspectedAllocation {
        widget: String,
        render_time_us: u64,
        cells: usize,
    },
    /// A single widget dominates frame time.
    Hog {
        widget: String,
        fraction_percent: u64,
    },
}

impl fmt::Display for WasteCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullRedrawForSmallChange {
                widget,
                cells_written,
                estimated_needed,
            } => write!(
                f,
                "{widget} wrote {cells_written} cells but likely only needed ~{estimated_needed}. \
                 It redraws the full area on every frame."
            ),
            Self::DeepNesting { depth, limit } => {
                write!(f, "layout nested {depth} levels deep (limit: {limit})")
            }
            Self::SuspectedAllocation {
                widget,
                render_time_us,
                cells,
            } => write!(
                f,
                "{widget} took {render_time_us}µs for only {cells} cells — \
                 likely allocating Strings or Vecs every frame"
            ),
            Self::Hog {
                widget,
                fraction_percent,
            } => write!(
                f,
                "{widget} consumed {fraction_percent}% of frame time"
            ),
        }
    }
}

/// A single waste finding for a specific frame.
#[derive(Debug, Clone)]
pub struct WasteFinding {
    pub frame: u64,
    pub category: WasteCategory,
    pub severity: Severity,
}

/// How bad is it, really.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Hint,
    Warning,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Hint => write!(f, "hint"),
            Severity::Warning => write!(f, "warning"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// The waste detector. Stateless between calls — all context comes from profiler data.
pub struct WasteDetector {
    last_findings: Vec<WasteFinding>,
}

impl WasteDetector {
    pub fn new() -> Self {
        Self {
            last_findings: Vec::new(),
        }
    }

    /// Analyze a frame's data and return findings.
    pub fn detect(
        &self,
        frame: u64,
        frame_time: Duration,
        widgets: &[(String, Duration, usize)],
        all_stats: &HashMap<String, PerWidgetStats>,
        budget: &FrameBudget,
    ) -> Vec<WasteFinding> {
        let mut findings = Vec::new();
        let _total_cells: usize = widgets.iter().map(|w| w.2).sum();

        for (name, time, cells) in widgets {
            // Check for hog: widget takes >60% of frame time
            if !frame_time.is_zero() {
                let fraction = time.as_secs_f64() / frame_time.as_secs_f64();
                if fraction > 0.6 {
                    findings.push(WasteFinding {
                        frame,
                        category: WasteCategory::Hog {
                            widget: name.clone(),
                            fraction_percent: (fraction * 100.0) as u64,
                        },
                        severity: if fraction > 0.85 {
                            Severity::Critical
                        } else {
                            Severity::Warning
                        },
                    });
                }
            }

            // Check for full-redraw-for-small-change
            if let Some(stats) = all_stats.get(name) {
                if stats.render_count > 3 && stats.last_was_full_redraw && *cells > 500 {
                    // Heuristic: if the widget consistently does full redraws
                    // and writes many cells, flag it
                    let estimated = (*cells / 10).max(2);
                    findings.push(WasteFinding {
                        frame,
                        category: WasteCategory::FullRedrawForSmallChange {
                            widget: name.clone(),
                            cells_written: *cells,
                            estimated_needed: estimated,
                        },
                        severity: if *cells > 5000 {
                            Severity::Warning
                        } else {
                            Severity::Hint
                        },
                    });
                }
            }

            // Check for suspected allocation: >50µs per cell is suspicious
            let time_us = time.as_micros() as u64;
            if *cells > 0 && time_us > 0 {
                let us_per_cell = time_us / (*cells as u64).max(1);
                if us_per_cell > 50 {
                    findings.push(WasteFinding {
                        frame,
                        category: WasteCategory::SuspectedAllocation {
                            widget: name.clone(),
                            render_time_us: time_us,
                            cells: *cells,
                        },
                        severity: if us_per_cell > 200 {
                            Severity::Warning
                        } else {
                            Severity::Hint
                        },
                    });
                }
            }
        }

        // Deep nesting check (approximated by widget count in a single frame)
        // The real depth tracking happens in begin_widget; this is a cross-check
        if widgets.len() > budget.max_widget_depth * 2 {
            findings.push(WasteFinding {
                frame,
                category: WasteCategory::DeepNesting {
                    depth: widgets.len(),
                    limit: budget.max_widget_depth,
                },
                severity: Severity::Hint,
            });
        }

        findings
    }

    /// Stash findings for later retrieval (called by the profiler at end_frame).
    pub fn stash_findings(&mut self, _frame: u64, findings: Vec<WasteFinding>) {
        self.last_findings = findings;
    }

    /// Retrieve the findings from the last frame.
    pub fn last_findings(&self) -> &[WasteFinding] {
        &self.last_findings
    }
}

impl Default for WasteDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hog() {
        let detector = WasteDetector::new();
        let budget = FrameBudget::for_60fps();
        let mut stats = HashMap::new();
        stats.insert(
            "StatusBar".to_string(),
            PerWidgetStats {
                name: "StatusBar".to_string(),
                total_time: Duration::from_millis(50),
                render_count: 10,
                last_cells: 80,
                peak_time: Duration::from_millis(10),
                prev_cells: 80,
                last_was_full_redraw: false,
            },
        );

        let widgets = vec![
            ("StatusBar".to_string(), Duration::from_millis(12), 80),
            ("Body".to_string(), Duration::from_millis(1), 2000),
        ];

        let findings = detector.detect(
            1,
            Duration::from_millis(14),
            &widgets,
            &stats,
            &budget,
        );

        let hogs: Vec<_> = findings
            .iter()
            .filter(|f| matches!(f.category, WasteCategory::Hog { .. }))
            .collect();
        assert!(!hogs.is_empty());
        assert!(matches!(hogs[0].severity, Severity::Critical));
    }
}
