# RFD 0002 comment: platform

- Date: 2026-10-10
- Lens: iOS/Swift/Xcode/TestFlight constraints and costs
- Read: RFD 2 at `b203b8d`; transcripts 1–3; `SWIFT-APP.md`, `DEVELOPMENT.md`, `docs/swift-architecture/`; `project.pbxproj`, schemes, `Core.swift`, `GainzvilleApp.swift`, `client/client.rs`, `scripts/`. Background-task, iCloud, and TestFlight behavior is from general platform knowledge.

## Verdict
The smallest milestone fits iOS well if it skips background tasks and the ubiquity container. Two cheap project changes come first: a separate dev bundle ID and a launch path that doesn't crash.

## Concerns
- Re: Builds and the dev loop: Debug and Release both use `com.gainzville.Gainzville`. An Xcode run replaces the TestFlight app and opens the same container. That's the "wrong version on my phone" case from transcript 3, and `-wipeDB` would delete real data.
- Re: Automatic migration: `try! makeCore` (`GainzvilleApp.swift:25`) crashes on every launch if a migration fails, and nothing copies the file before migrating.
- Re: Backups to iCloud: the app has no entitlements file. A ubiquity container needs iCloud capability setup, and it uploads when iOS decides to. `BGTaskScheduler` is discretionary, so a weekly job could just not run.
- Re: Backups: copying `gainzville.sqlite` alone can miss rows held in a `-wal` sidecar (`Core.swift` already handles these on wipe).

## Missing
- TestFlight builds expire after 90 days. The data stays, but the app won't open until you ship a new build.
- Build numbers are fixed at `CURRENT_PROJECT_VERSION = 1`, and each upload needs a higher one.
- There's no CI at all (no `.github/`).
- `Documents/` already goes into iCloud device backup. That's an accidental safety net, but restoring it means restoring the whole device.

## Recommendation
- Give Debug a `.dev` bundle ID suffix.
- Export snapshots through Rust `VACUUM INTO` and save them via `fileExporter` to iCloud Drive. This needs no entitlement.
- Automate later by checking "last backup older than N days" when the app comes to the foreground.
- Snapshot before migrating, and show an error screen instead of `try!`.
- Run fixture-migration CI on Linux with `cargo test`. Archive and upload from the Mac by script.

## Can expand on
- Ubiquity container vs Files export, step by step
- File protection classes and how iCloud encrypts the backups
- Macro-free archive/upload script, export-compliance key
- Using the macOS target as a restore/inspection tool
