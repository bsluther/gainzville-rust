# Visual / behavioral verification of the Swift app

Notes captured while building the attribute action bar (GV-36). The goal is to
*see* a SwiftUI change rendered on a real simulator — fonts, colors, SF Symbols,
layout, separators — rather than trusting that "it builds" or eyeballing source.
This is the seed for a future, more automated verification skill.

## The pipeline that worked

All commands run from `swift-app/`.

1. **Build for the simulator — force arm64.**
   ```bash
   xcodebuild -project Gainzville.xcodeproj -target Gainzville \
     -sdk iphonesimulator -configuration Debug \
     ARCHS=arm64 ONLY_ACTIVE_ARCH=YES build 2>&1 \
     | grep -E '(error:|BUILD SUCCEEDED|BUILD FAILED)'
   ```
   Gotchas:
   - Use `-target`, **not** `-scheme` (scheme + destination hits a platform-resolution
     bug, per SWIFT-APP.md). `-derivedDataPath` *requires* a scheme, so don't use it —
     let it default to `swift-app/build/`.
   - Without `ARCHS=arm64 ONLY_ACTIVE_ARCH=YES` the build defaulted to **x86_64** and
     failed at link with `found architecture 'arm64', required architecture 'x86_64'`
     against `libgv_ffi.a`. The FFI xcframework has an `ios-arm64-simulator` slice;
     forcing arm64 (we're on Apple Silicon) makes them match.

2. **Pick a simulator whose iOS ≥ the app's deployment target.**
   The app's min OS is iOS 26.x. Installing onto an iOS 18.x sim fails with
   *"Requires a Newer Version of iOS."* List devices and pick a 26.x one:
   ```bash
   xcrun simctl list devices available     # find an iOS 26.x iPhone (e.g. iPhone 17 Pro)
   xcrun simctl boot <UDID>
   open -a Simulator
   ```

3. **Install, launch, screenshot.**
   ```bash
   xcrun simctl terminate booted com.gainzville.Gainzville 2>/dev/null
   xcrun simctl install booted build/Debug-iphonesimulator/Gainzville.app
   xcrun simctl launch booted com.gainzville.Gainzville
   sleep 4
   xcrun simctl io booted screenshot /tmp/shot.png
   ```
   Then read `/tmp/shot.png` to inspect the result.

## The "replica scaffold" technique

The component under test (a picker-sheet action bar) is reached only after several
taps: create entry → expand → expand Time → tap the Duration pill. `simctl` has **no
tap command**, and the app launches with an empty log, so driving to the real screen
is impractical.

Workaround: temporarily present the component directly from a screen that shows on
launch, with a faithful copy of its real container, then screenshot and **revert**:

```swift
// TEMPORARY — added to LogView body, reverted after screenshotting.
.sheet(isPresented: .constant(true)) {
    VStack(spacing: GvSpacing.md) {
        AttributeSheetBar(title: "Outcome", kind: .select, onDismiss: {})
        ScrollView { VStack { ForEach(["Sent","Flash","Onsight"], id: \.self) { Text($0).padding() } } }
    }
    .presentationDetents([.medium])
}
```

`.constant(true)` forces it open immediately — no navigation needed. This is how the
in-sheet bar's title size, separators, darker strip, icons, and dismiss-glyph color
were all confirmed.

## Pros

- **Real rendering.** Actual SwiftUI layout, design tokens, SF Symbols, and detents on
  device — catches things a successful build does not (e.g. the bar collapsing in a
  free `VStack` because a horizontal `ScrollView` was vertically greedy; only visible
  once rendered).
- **Fast to reach.** `.constant(true)` shows the component on launch; no tap automation.
- **Isolates the component**, so you reason about one thing.

## Cons / weaknesses (why this is a starting point, not the answer)

- **It's a *fake* instance, not the real path.** The real sheet is reached via
  `EntryView → TemporalAttribute → DurationPickerPill → platformPopover`, with real
  data and environment objects. The replica reproduces structure, not that path —
  a discrepancy there wouldn't be caught. (Here the replica was structurally identical,
  so confidence was high, but that's a judgment call each time.)
- **Source editing + revert risk.** You mutate app source for the scaffold and must
  remember to remove it; easy to leave debug code behind.
- **Static only — no behavior.** Screenshots verify appearance, not interaction
  (taps, debounce, focus changes, keyboard show/hide duplication). `simctl` can't tap.
- **Empty seed state.** No data on launch, so real navigation paths need many manual
  steps to reach.
- **Eyeballing, not asserting.** Visual judgment from one screenshot; near-black /
  near-equal colors can't be trusted by eye (cf. the macOS window-background lesson —
  sample pixels when contrast is marginal).
- **Slow loop.** A full `xcodebuild` per iteration.

## Ideas for the eventual skill

- A **dev harness route** (gated by a launch arg, like the existing DB-wipe arg) that
  presents any component/sheet directly — removes the edit-source-then-revert step.
- **Seed data via launch arg** so the *real* nested paths are reachable for screenshots.
- **UI automation** (XCUITest target, or `idb`/AppleScript clicks) to drive the real
  path and verify *behavior*, not just appearance.
- **Snapshot testing** (e.g. pointfreeco/swift-snapshot-testing) for regression.
- **Pixel sampling** helper for verifying near-black colors instead of trusting the eye.
