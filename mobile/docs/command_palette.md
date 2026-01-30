# Command Palette Keyboard Handling

## Requirements
1. `cmd+p` always toggles the palette (focus-independent)
2. When open, palette commands (arrows, escape, enter) always work
3. When open, keyboard events only fire to palette elements (not underlying page)

## Approach: Focus Trap

The palette uses a full-screen overlay (`position: fixed; inset: 0`) with a focus trap:

- **Tab key** is intercepted and prevented from leaving
- **Navigation keys** (up/down/escape/enter) handled by Dioxus `onkeydown`
- **Typing and cursor keys** work natively in the focused input
- **Global toggle** (`cmd+p`) uses a separate JS listener on `document`

With focus trapped inside the palette, all keyboard events naturally route to palette elements.

## Alternatives Considered

### Global JS listener (bubbling phase)
Events propagate to the focused element *before* bubbling to the document-level handler. Can't prevent events from reaching non-palette elements.

### Global JS listener (capture phase)
Capture phase intercepts events *before* they reach targets. Would work, but requires JS containment checks (`palette.contains(e.target)`) to allow typing in the input while blocking events to non-palette elements. Mixes JS DOM logic with Rust state.

### Root wrapper with Dioxus handler
Same problem as bubbling—by the time the wrapper sees the event, the target already received it. Dioxus doesn't expose capture-phase event handlers.

### Centralized Rust handler (manual input management)
Stop all events in capture phase, handle everything in Rust including text input. Requires reimplementing keyboard functionality (typing, backspace, cursor movement). Undesirable complexity.

## Why Focus Trap Works
- Full-screen overlay ensures clicks outside close the palette
- Tab interception prevents focus escape via keyboard
- Native input behavior preserved (no manual text handling)
- Clean separation: navbar handles toggle, palette handles interaction
