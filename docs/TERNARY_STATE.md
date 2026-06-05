# Ternary State Management for Terminal UIs

*How Open-TUI and Open-Terminal can use ternary state machines for responsive, predictable interfaces.*

---

## The Problem Terminal UIs Have

Every terminal UI manages state: focused/unfocused, loading/loaded, visible/hidden, connected/disconnected.

Most TUIs use booleans for each of these, resulting in combinatorial explosion:

```rust
struct AppState {
    is_connected: bool,
    is_loading: bool,
    has_focus: bool,
    is_visible: bool,
}
// 16 possible states, but only ~5 are meaningful
// The rest are contradictions waiting to happen
```

---

## The Ternary Solution

Group related binary flags into ternary states:

### Connection: Connected / Degraded / Disconnected
- `+1` → Connected and responsive
- `0` → Connected with latency/errors (degraded)
- `-1` → Disconnected

### Focus: Active / Background / Hidden
- `+1` → Pane has focus
- `0` → Pane is visible but unfocused
- `-1` → Pane is hidden

### Loading: Loaded / Loading / Error
- `+1` → Data loaded successfully
- `0` → Loading in progress
- `-1` → Load failed

---

## Example: A Ternary Terminal

```rust
use ternary_types::Ternary;

struct TerminalPane {
    id: String,
    connection: Ternary,  // -1=disconnected, 0=degraded, +1=connected
    focus: Ternary,       // -1=hidden, 0=background, +1=focus
    data_state: Ternary,  // -1=error, 0=loading, +1=loaded
}

impl TerminalPane {
    /// Is this pane actionable?
    fn is_actionable(&self) -> bool {
        // A pane is actionable only if connected AND loaded AND visible
        self.connection.is_positive() 
            && self.data_state.is_positive()
            && !self.focus.is_negative()  // not hidden
    }
    
    /// What color should the border be?
    fn border_style(&self) -> &'static str {
        match (self.connection, self.data_state) {
            (Ternary::Positive, Ternary::Positive) => "green",   // healthy
            (Ternary::Positive, Ternary::Neutral)  => "yellow",  // loading
            (Ternary::Neutral, _)                  => "yellow",  // degraded
            (_, Ternary::Negative)                 => "red",     // error
            _                                       => "gray",   // disconnected
        }
    }
}
```

---

## State Transitions

Ternary state machines for TUIs have clean, predictable transitions:

```
Disconnected (-1)  →  Connecting (0)  →  Connected (+1)
                                            ↓
                                     Degraded (0)  →  Connected (+1)
                                            ↓
                                    Disconnected (-1)  [reconnect loop]
```

No impossible states. No contradictory flags. Every combination of `(connection, focus, data)` is meaningful.

---

## See Also

- **[From Binary to Ternary](https://github.com/SuperInstance/ternary-cookbook/blob/master/guides/FROM_BINARY.md)** — full migration guide
- **[ternary-state](https://github.com/SuperInstance/ternary-state)** — dedicated state machine crate
- **[pincher](https://github.com/SuperInstance/pincher)** — reflex runtime for embedding ternary logic in applications
