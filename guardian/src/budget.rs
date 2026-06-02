//! Frame budget configuration.
//!
//! A `FrameBudget` defines the performance constraints for a single render frame.
//! The most common budget is 16ms (60fps), but you can tune each limit independently.

use std::fmt;
use std::time::Duration;

/// Hard limits for a single render frame.
///
/// Create one via [`FrameBudget::for_60fps`] or [`FrameBudget::for_30fps`],
/// then pass it to [`crate::RenderProfiler`].
#[derive(Debug, Clone)]
pub struct FrameBudget {
    /// Maximum allowed render time per frame.
    pub max_render_time: Duration,
    /// Maximum number of cells a single diff may touch (approximate terminal width × height).
    pub max_diff_cells: usize,
    /// Maximum widget nesting depth before we flag it.
    pub max_widget_depth: usize,
}

impl FrameBudget {
    /// Budget tuned for 60fps (16.67ms per frame).
    pub fn for_60fps() -> Self {
        Self {
            max_render_time: Duration::from_millis(16),
            max_diff_cells: 10_000, // ~130×75 terminal, conservative
            max_widget_depth: 5,
        }
    }

    /// Budget tuned for 30fps (33ms per frame).
    pub fn for_30fps() -> Self {
        Self {
            max_render_time: Duration::from_millis(33),
            max_diff_cells: 10_000,
            max_widget_depth: 5,
        }
    }

    /// Budget with custom limits.
    pub fn new(max_render_time: Duration, max_diff_cells: usize, max_widget_depth: usize) -> Self {
        Self {
            max_render_time,
            max_diff_cells,
            max_widget_depth,
        }
    }

    /// Check a render duration against this budget.
    pub fn check_time(&self, elapsed: Duration) -> Option<BudgetViolation> {
        if elapsed > self.max_render_time {
            Some(BudgetViolation::OverTime {
                budget: self.max_render_time,
                actual: elapsed,
            })
        } else {
            None
        }
    }

    /// Check a diff cell count against this budget.
    pub fn check_diff(&self, cells: usize) -> Option<BudgetViolation> {
        if cells > self.max_diff_cells {
            Some(BudgetViolation::DiffTooLarge {
                budget: self.max_diff_cells,
                actual: cells,
            })
        } else {
            None
        }
    }

    /// Check a widget depth against this budget.
    pub fn check_depth(&self, depth: usize) -> Option<BudgetViolation> {
        if depth > self.max_widget_depth {
            Some(BudgetViolation::DepthTooDeep {
                budget: self.max_widget_depth,
                actual: depth,
            })
        } else {
            None
        }
    }
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self::for_60fps()
    }
}

/// A single budget violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetViolation {
    OverTime {
        budget: Duration,
        actual: Duration,
    },
    DiffTooLarge {
        budget: usize,
        actual: usize,
    },
    DepthTooDeep {
        budget: usize,
        actual: usize,
    },
}

impl fmt::Display for BudgetViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OverTime { budget, actual } => write!(
                f,
                "frame took {}ms (budget: {}ms)",
                actual.as_millis(),
                budget.as_millis()
            ),
            Self::DiffTooLarge { budget, actual } => {
                write!(f, "diff touched {actual} cells (budget: {budget})")
            }
            Self::DepthTooDeep { budget, actual } => {
                write!(f, "widget depth {actual} (budget: {budget})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_fps_budget() {
        let b = FrameBudget::for_60fps();
        assert_eq!(b.max_render_time, Duration::from_millis(16));
        assert_eq!(b.max_widget_depth, 5);
    }

    #[test]
    fn detects_over_time() {
        let b = FrameBudget::for_60fps();
        let v = b.check_time(Duration::from_millis(20));
        assert!(v.is_some());
        assert!(matches!(v.unwrap(), BudgetViolation::OverTime { .. }));
    }

    #[test]
    fn within_budget_is_ok() {
        let b = FrameBudget::for_60fps();
        assert!(b.check_time(Duration::from_millis(10)).is_none());
        assert!(b.check_diff(100).is_none());
        assert!(b.check_depth(3).is_none());
    }
}
