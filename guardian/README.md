# Render Budget Guardian

A Rust module for [ratatui](https://github.com/ratatui/ratatui) that tracks terminal rendering performance and enforces frame budgets.

## Why

TUI apps have a performance problem nobody talks about. Your status bar redraws 80 cells on every tick. Your layout is nested 6 deep. One widget allocates a `String` every frame just to format a number.

None of this shows up in a profiler until the app stutters. Guardian makes it visible before that.

## What it does

**Frame budget enforcement.** You say "16ms per frame" (60fps). Guardian tells you when you blow it.

**Per-widget profiling.** How long did each widget take? How many cells did it touch? What fraction of the frame did it eat?

**Waste detection.** Three anti-patterns, caught automatically:

- **Full redraws for small changes.** A widget writes 2,000 cells when 2 changed. It does this every frame.
- **Deep nesting.** Layouts nested 5+ levels. Each level is a function call, an allocation, and a layout pass.
- **Per-frame allocation.** Widgets that take 50µs+ per cell — the signature of `String::new()` or `Vec::new()` in a hot loop.

**Terminal-friendly reports.** Because you're building a TUI. The output should look good in one.

```
Frame 847: 12.0ms total (⚠ OVER BUDGET, budget: 16ms) 2080 cells
  StatusBar: 8.0ms (67%) for 80 cells — HOG
  Body: 3.0ms (25%) for 2000 cells
  1 findings: 0 critical, 1 warnings, 0 hints
    [warning] StatusBar consumed 67% of frame time
```

## Usage

```rust
use ratatui_guardian::{FrameBudget, RenderProfiler};

let budget = FrameBudget::for_60fps();
let mut profiler = RenderProfiler::new(budget);

// In your render loop:
loop {
    profiler.begin_frame();

    profiler.begin_widget("StatusBar");
    // draw your status bar
    profiler.end_widget(80);

    profiler.begin_widget("Body");
    // draw your body
    profiler.end_widget(2000);

    let total = profiler.end_frame();

    if total > budget.max_render_time {
        println!("{}", profiler.report());
    }
}
```

## API

| Type | Purpose |
|------|---------|
| `FrameBudget` | Configure limits: render time, diff size, max depth |
| `RenderProfiler` | Collect timing data per frame and per widget |
| `WasteDetector` | Identify anti-patterns (usually called via profiler) |
| `ReportFormatter` | Human-readable output via `profiler.report()` |

### FrameBudget

```rust
// Presets
let fast = FrameBudget::for_60fps();   // 16ms, 10K cells, depth 5
let chill = FrameBudget::for_30fps();   // 33ms, 10K cells, depth 5

// Custom
let custom = FrameBudget::new(
    Duration::from_millis(20),  // max render time
    5_000,                       // max diff cells
    3,                           // max widget depth
);
```

### BudgetViolations

The profiler checks three things at `end_frame()`:

- **OverTime** — frame exceeded the time budget
- **DiffTooLarge** — total cells written exceeded the diff budget
- **DepthTooDeep** — widget nesting exceeded the depth limit

### WasteCategories

- **Hog** — widget takes >60% of frame time (>85% = critical)
- **FullRedrawForSmallChange** — widget rewrites everything when only a few cells changed
- **SuspectedAllocation** — render time per cell suggests per-frame allocations

## Running tests

```bash
cd guardian
cargo test
```

## Design notes

Guardian is intentionally standalone. It doesn't depend on ratatui itself — it just needs widget names, timing, and cell counts. You can adopt it incrementally: wrap your biggest widgets first, add more as needed.

The profiler keeps a 120-frame rolling history. Enough to spot patterns, not enough to eat memory.

Waste detection uses heuristics, not hard thresholds. A widget that takes 50µs per cell once is fine. A widget that does it every frame for 3+ frames gets flagged. The goal is signal, not noise.

## License

MIT
