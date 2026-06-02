//! # Render Budget Guardian
//!
//! Frame budget tracking and waste detection for ratatui TUI applications.
//!
//! Most TUI apps silently waste render time: full-screen redraws for a 2-cell change,
//! widgets that allocate `String` on every frame, layouts nested 5+ deep.
//! Guardian makes that waste visible.
//!
//! ## Quick start
//!
//! ```rust
//! use ratatui_guardian::{FrameBudget, RenderProfiler, WasteDetector};
//!
//! let budget = FrameBudget::for_60fps();
//! let mut profiler = RenderProfiler::new(budget);
//!
//! // Wrap your widget render calls:
//! profiler.begin_frame();
//!
//! profiler.begin_widget("StatusBar");
//! // ... your widget renders ...
//! profiler.end_widget(80 /* cells written */);
//!
//! profiler.end_frame();
//!
//! let report = profiler.report();
//! println!("{}", report);
//! ```

mod budget;
mod detector;
mod profiler;
mod report;

pub use budget::{BudgetViolation, FrameBudget};
pub use detector::{WasteCategory, WasteDetector, WasteFinding};
pub use profiler::{PerWidgetStats, RenderProfiler};
pub use report::ReportFormatter;
