# Hashlark adaptive UI study

Open `index.html` directly for the interactive app or `review.html` for the side-by-side design study. No build, packages, remote fonts, or network requests are needed. To serve locally from the repository root:

```powershell
python -m http.server 4178 --bind 127.0.0.1 --directory docs/ui-mock
```

Then visit http://127.0.0.1:4178/review.html. `index.html?selected=1` opens the populated detail example. The normal entry has no selected result and gives the list the full width.

## Fold7-specific correction (29 September 2026)

The original 840×730 frame in the study was a generic breakpoint preview, not a measured Galaxy Z Fold7 viewport. The study now shows a 750 px width model and links to `fold7.html`, which lets reviewers compare rotation, browser chrome, and density scenarios without silently stretching the embedded app.

- [Samsung's specifications](https://news.samsung.com/global/samsung-galaxy-z-fold7-raising-the-bar-for-smartphones) list an 8.0-inch inner screen at 1968×2184 physical pixels in portrait, and a 6.5-inch cover screen at 1080×2520.
- [Published Fold7 measurements](https://effectiveviewport.com/devices/samsung-galaxy-z-fold7-unfolded/) report a 750×832 CSS-pixel screen at DPR 2.625; Chrome content is 749×654 with browser controls expanded. The publisher's [device catalog](https://effectiveviewport.com/devices/) identifies the Fold7 measurements as physical-device readings. These are the publisher's measurements, not measurements of the user's phone.
- At 420 logical dpi, the inner display converts to approximately 750×832 dp; rotated, approximately 832×750. These match the model already used in `WindowShapeTest.kt`. Display zoom can change the logical density; physical PPI must not be substituted for the OS's logical density. See [Android's density guidance](https://developer.android.com/training/multiscreen/screendensities).
- The connected `hashlark_fold` emulator reports **2208×1840 px at 420 dpi**, or about **841×701 dp**. Despite its name, that is not the Fold7's panel geometry. No physical Fold7 is connected. Nothing was changed on the emulator.
- With the mock's current 840 px split threshold, **both 750-wide portrait and 832-wide landscape use separate list/detail screens**. This avoids compressed columns but does not establish a simultaneous two-pane Fold7 design.
- Forcing the current rail/gutters into 750 dp would leave only approximately `750 − 80 − 52 − 16 − 310 = 292 dp` for the list before scrollbars. Two panes on this width need a more compact composition, not smaller text. The native app used to enable two panes from 600 dp; that rule has been replaced (see "Native implementation" below).

The native-app previews reserve an illustrative 52 dp vertically for system UI; actual insets/taskbar/keyboard need measurement on the phone. The 360/480 dpi profiles are sensitivity tests, not claimed Samsung default presets. `fold7.html` distinguishes these assumptions from browser measurements. The Android app now implements these rules; the desktop app is unchanged.

The follow-up browser check exercised both list and selected detail at 750×780, 832×698, 749×654, 832×575, 656×676, 875×918, and 411×908. No app document overflow occurred and the primary detail action was visible without scrolling in every selected state. Only the 875-wide sensitivity case used two panes; the other six used full-screen details. The 750-wide list measured 603 px after the browser scrollbar, rail, and margins. The validation page's selection and orientation controls were also exercised. This is a browser-model check, not native Fold7 sign-off.

## Native implementation (Android, 29 September 2026)

The Compose app now follows this study. The 600 dp two-pane rule is gone; the decision lives in `WindowShape.planPanes()` (`apps/android/app/src/main/kotlin/io/github/ashxtrem/hashlark/ui/WindowShape.kt`) and is unit-tested in `WindowShapeTest`.

| Window (dp, not physical pixels) | Navigation | Result selection |
| --- | --- | --- |
| Below 600 wide | Bottom bar | Full-screen details; the bar is hidden while they show |
| 600–839 | 80 dp rail | Full-screen details |
| 840–1199 | 80 dp rail | Beside the results, only if both panes still fit (below) |
| 1200 and wider | 220 dp sidebar | Beside the results, only if both panes still fit |
| Height under 480 | Rail | Full-screen details, category strip hides while scrolling |
| Tabletop posture | None | Details above the hinge, controls and keyboard below it |

"Fits" means: window width, minus navigation, minus side insets, leaves at least `360 × text scale + 32` dp for the list and `310 × text scale + 32` dp for the details (text scale never counts below 1). Text is not shrunk to make it fit; at 1.3× a 1024 dp window still splits, at 2.0× not even 1280 dp does. Next to a vertical fold, the list must fit left of the hinge and the details right of it; a hinge with a gap is never covered, and a flat fold (Fold7) has no gutter.

Consequences for the Galaxy Z Fold7 (about 750 × 832 dp inner portrait, 832 × 750 landscape at 420 dpi): both use the list → full-screen details flow. Before a result is opened the list uses the whole width, with no empty details pane. Back, the toolbar arrow and the system gesture return to the same list position with the opened row still marked. Folding or resizing keeps the selection, but never reopens details that were closed.

Also implemented natively: search controls above the results workspace (labelled field, clear, IME Search, submit/stop, a Filters button outside the scrolling category strip with a faded edge cue), a visible sort control and provider status in the results header, a filter sheet (below 600 dp) or dialog with Apply / Reset / Cancel that only applies on Apply, two-line filenames with labelled size, seeds (`Seeds unknown` versus `Seeds 0`) and source count, a single save action per row with the rest in the details and in TalkBack's actions menu, and a green/warm-neutral theme with dark mode and dynamic colour.

### What has and has not been verified

- Verified by automated tests: the pane rules above, selection and Back behaviour, state across resizing and folding, and rendered layouts (Robolectric, `LayoutMatrixTest`) at 320, 360, 411 (cover), 750 × 832 and 832 × 750 (Fold7 inner), 839, 840, 1024, 1280, 800 × 360 (short landscape), 375 and 250 (split-screen), and font scales 1.3 and 2.0. Pictures go to `apps/android/app/build/screenshots/`; a selection is kept in `docs/ui-mock/native-screenshots/` (Robolectric renders with zero system insets, not device screenshots).
- Not verified: a physical Fold7, a real hinge or posture change, the soft keyboard, TalkBack, system gestures, and display zoom or taskbar insets on a device. The emulator profile `hashlark_fold` (2208 × 1840 px at 420 dpi, about 841 × 701 dp) could not be booted on the workstation used for this change (not enough disk space); note that at 841 dp wide it would show two panes by the rules above, which does not make it a Fold7 profile. Robolectric renders have zero system insets.

## Assessment of the supplied screenshot

- Splitting at the existing 600 dp threshold takes width away from search and filenames, even before a result is selected. The empty right pane adds little value.
- Search, categories, filters, and provider status compete inside the left pane. A control appears cut off at the edge without a strong scrolling cue.
- Filenames and tiny metadata form a visually uniform stack. Repeated download and overflow icons carry similar visual weight to the result itself.
- Dashes, arrows, counts, and ages require interpretation. Unknown seeders must remain distinguishable from zero seeders.
- A foldable composition should not simply shrink onto a bar phone or cover screen.

## Proposed behavior

The search field and category strip sit above the results workspace. Before selection, the list uses the available content width. Selection opens a detail pane only when both panes have usable space. Search and navigation stay consistent across widths; filters live in a dialog that becomes a bottom sheet on phones.

| Available window width | Navigation | Detail destination |
| --- | --- | --- |
| 320–599 | Five-item bottom navigation | Full-screen detail |
| 600–839 | 80 px rail | Full-screen detail |
| 840–1199 | 80 px rail | Supporting pane after selection |
| 1200+ | 208 px sidebar | Supporting pane after selection |
| Any width with height under 480 | Compact spacing | Full-screen detail |

The browser breakpoints are proposed design values, not physical device detection. In Compose, subtract navigation, gutters, and hinge bounds first. Require about 360 dp for the list and 310 dp for details; fall back to one pane when those cannot fit. Do not unconditionally force two panes for book posture. Tabletop posture needs a separate native composition and is not simulated here.

## Component handoff

- `SearchScreen.kt`: move common search controls above the list/detail area. Do not allocate an unselected detail placeholder. Preserve query, filters, selected identity, and list scroll across resizing, folding, navigation, and process restoration.
- `WindowShape.kt`: separate “can fit two useful panes” from nominal window width and physical posture. Consider both width and height. At increased font scale, prefer a single pane earlier.
- `ResultRow.kt` / `ResultsList.kt`: preserve the real filename, clamp to two lines in compact rows, and expose the complete name in details and accessibility semantics. Use a selected background plus a leading edge. Keep size, labelled seed count, and source count; remove age before shrinking text. Use one save action; put the primary client handoff in details.
- `ResultDetails.kt`: full filename, category and publication, primary client action, save, copy magnet, key facts, sources, and expandable technical details. Preserve the existing capability-driven `.torrent`, sharing, and link availability in implementation; those secondary actions are not all represented in this search-focused study. A full-screen detail replaces bottom navigation and supports native Back.
- `Filters.kt`: accessible modal sheet on compact windows; dialog on larger windows. Sort remains visible in the list header. Filters apply on explicit submission; Cancel leaves current filters alone.
- Provider connections in the prototype are sample states. Production must distinguish still searching, success, no results, unavailable providers, cancellation, and partial failure. Keep streamed results usable and do not show success merely because a request finished.

## Visual tokens

| Token | Value |
| --- | --- |
| Canvas | `#f5f6f2` |
| Surface | `#ffffff` |
| Primary text | `#202e29` |
| Secondary text | `#69736d` |
| Primary action | `#205b46` |
| Navigation surface | `#143b32` |
| Selected result | `#edf5e9` |
| Accent | `#c9e7a8` |
| Border | `#e2e7df` |
| Spacing | 4, 8, 12, 16, 24, 32 |
| Surface corners | 13–16 px |
| Main touch targets | 48 px minimum |

Use native typography and dynamic font scaling in Compose. The HTML uses system fonts and compact browser sizes to explore hierarchy, not fixed sp values for the Android implementation. Secondary metadata should remain legible at the user's selected text size; let row heights grow. Test contrast with the final Android palette, including dark mode and dynamic color before adoption.

## Working interactions and scope

- Local search (`vim`, `ubuntu`, or any matching substring), category filters, sort, seeded-only/provider filters, and no-results/reset states.
- Full-width unselected list, selected wide detail, compact full-screen detail, Escape/back button, and browser Back from a clicked result.
- Save/unsave; Favourites persists under a prototype-specific localStorage key when storage is available. History is session-only. Provider toggles affect local filtering.
- Search, Favourites, History, Providers, and an informational Settings preview.
- Keyboard focus styling, labels, modal focus containment, and safe-area CSS. Ctrl/Cmd+K focuses search.
- Client launch, magnet copy, and provider website actions show explicit preview feedback. No torrents are downloaded or opened, and no real provider data is queried.
- Sample seed counts, publication dates, file types, provider associations, and filenames are illustrative. The sample deliberately includes long names, unknown seeders, zero seeders, and multiple sources.

The desktop app is unchanged. A CSS preview cannot verify native hinge avoidance, the soft keyboard, TalkBack, system gesture handling, text scaling, or actual Android performance.

## Browser verification

Checked both unselected and selected states at 320×700, 360×800, 390×844, 600×800, 768×800, 839×730, 840×730, 1024×800, 1280×900, and 740×360. No document-level horizontal overflow was found. At 840 px, the rendered list and details measured approximately 367 and 310 px respectively; below that breakpoint the detail fills the window. Short landscape also uses full-screen details.

Exercised browser Back, `ubuntu` search (one result), unmatched search and reset, seed sorting, seeded-only filtering (six of eight `vim` results), saving and Favourites, and provider navigation. Inspected rendered wide, narrow, detail, and bottom-sheet states; browser logs showed no warnings or errors. JavaScript passes `node --check`.
