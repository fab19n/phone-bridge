# PhoneBridge — Product Requirements (DRAFT v0.2)

**Status:** DRAFT. Owner answered Part D on 2026-11-01 (see §0); segment sequencing and wedge still to confirm. · **Date:** 2026-11-01 · **Covers:** REVIEW A1, Part D; DEVELOPMENT_PLAN Stage 0 exit gate ("PRD signed off").
Every place marked **DECISION NEEDED:** must be filled in or approved before Stage 2 starts.

## 0. Owner decisions (2026-11-01)

| # | Question | Decision |
|---|---|---|
| Q1 | Target users | **All three segments (A, B, C).** Proposed sequencing to confirm: build the MVP around B's needs, which also serves A; add C's needs after audio (§3) |
| Q2 | Wedge feature | **Pending.** Owner asked what a wedge is; the explanation and recommendation are in §4 |
| Q3 | Business model | **Free for now.** Freemium/paid decided later. Still open: open-source or closed-source free (§8) |
| Q4 | Core promise | **Mirroring + control** (not one-way casting) |
| Q5 | Platforms | **Windows only for now; macOS is on the roadmap** (post-1.0). Keep OS-specific code behind modules (§6) |
| Q6 | Companion app in MVP | **Agreed: dropped from the MVP.** Trade-offs recorded in §5.1 |

## 1. Competitive landscape (checked 2026-11-01)

| Product | Price | Platform | Key features | Weaknesses (our opening) | Source |
|---|---|---|---|---|---|
| **scrcpy** v5.0.1 (2026-10-08) | Free, Apache-2.0 | Win / macOS / Linux desktop; Android 5.0+ (API 21); audio needs Android 11+ (API 30); camera mirroring needs Android 12+ | USB/TCP mirror + control, nothing installed on the phone, audio, recording, camera mirroring, virtual displays, physical keyboard and mouse simulation (HID) | CLI only. No onboarding for USB debugging; settings are flags; errors are hard for non-developers to read | [README.md @ v5.0.1](https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/README.md) (requirements); [releases API, tag v5.0.1](https://api.github.com/repos/Genymobile/scrcpy/releases/tags/v5.0.1) (date) |
| **Microsoft Phone Link** | Free, preinstalled on Windows | Windows ↔ Android over the same Wi-Fi network (Bluetooth is used for iPhone) | Notifications, SMS, calls, photos. "Phone screen" mirroring on Android 10+ devices with Link to Windows installed, in select markets: Samsung, HONOR, OnePlus, OPPO, Realme, ASUS ROG Phone 8, vivo, iQOO, Xiaomi 14T series, Surface Duo/Duo 2. "Apps" is a separate feature; the page's intro names Samsung, HONOR, OPPO and ASUS (Link to Windows preinstalled) | Pixel, Motorola and other phones without Link to Windows get no mirroring. Android needs the PC and phone on the same Wi-Fi, so latency is variable. Needs a Microsoft account (work/school accounts not supported) | [supported devices](https://support.microsoft.com/en-us/windows/apps/phonelink/supported-devices-for-phone-link-experiences); [requirements and setup](https://support.microsoft.com/en-us/topic/phone-link-requirements-and-setup-cd2a1ee7-75a7-66a6-9d4e-bf22e735f9e3) (Wi-Fi, account) |
| **Vysor** | Free tier; Pro $2.50/mo, $10/yr or $40 lifetime; Enterprise licensing | Win / macOS / Linux / browser; Android + iOS | Mirror + control, "Go Wireless", "High Quality Mirroring", Vysor Share | Best features are behind Pro, but the official page doesn't show which ones (its feature table doesn't render). Third-party reviews say the free tier is low quality, shows ads, and paywalls wireless and file drag-and-drop (UNVERIFIED: no primary source) | [vysor.io](https://www.vysor.io/) |
| **AirDroid Cast** | Free (LAN cast only); Basic $19.99/yr or $2.49/mo (USB cast); Standard $29.99/yr or $3.49/mo (remote control); Team $359.99/yr (yearly only) | Receivers: Windows, macOS, web browser, Android / Android TV / smart TVs, iOS; Android, iOS, PC and browser senders | Cross-network casting, AirPlay receiver, up to 5 wireless casts at once | **Control is paywalled** (Standard tier). Needs an account and a phone app. USB is paid-only (Basic+) and casts at most 1 Android + 1 iOS device at a time | [pricing](https://www.airdroid.com/pricing/airdroid-cast/); [AirDroid Cast](https://www.airdroid.com/cast/) (receivers) |
| **Samsung DeX for PC** (discontinued) | Free | Windows + Galaxy only | DeX for PC is **no longer supported on Galaxy devices with One UI 7 or later** (One UI 6 keeps it, without technical support). Standalone DeX on an external monitor is unaffected | Samsung redirects users to Link to Windows / Phone Link, which does mirror on Galaxy. Samsung lists what is lost: USB connection between phone and PC, full-screen app window, Android desktop on the PC screen | [Samsung notice](https://www.samsung.com/ph/support/apps-services/changes-to-the-way-you-connect-samsung-galaxy-mobile-devices-to-pcs/) |
| **QtScrcpy** v4.2.1 (2026-09-19) | Free, Apache-2.0 | Win x64/x86, macOS (x64/arm64), Ubuntu x64 AppImage | scrcpy-based. Group control of many phones at once, game key mapping, recording, drag-and-drop APK/file | Qt UI aimed at power users. Ships its own scrcpy-server, built from the project's source tree (the README doesn't say whether it is modified) | [GitHub README](https://github.com/barry-ran/QtScrcpy); [releases API](https://api.github.com/repos/barry-ran/QtScrcpy/releases/latest) |
| **Escrcpy** v3.3.1 (2026-10-10) | Free now; v4.0 will **move to a new paid licence/billing model** | Electron; **Windows** (x64/arm64, installer + portable), macOS (x64/arm64), Linux (deb / AppImage / Flatpak) | scrcpy GUI, batch/group control, AI "Copilot" and MCP automation (bring your own API key) | A direct Windows competitor. Electron footprint. Licensing in flux; old purchases don't carry over | [releases API, tag v3.3.1](https://api.github.com/repos/viarotel-org/escrcpy/releases/latest) |
| **Other scrcpy wrappers** | Mostly free/OSS | Mixed | Small scrcpy GUIs and launchers listed as alternatives (e.g. guiscrcpy, SCRCPY+, AndroLaunch) | No significant new commercial entrant found (UNVERIFIED: search-based, not exhaustive; the source gives no dates) | [alternativeto](https://alternativeto.net/software/scrcpy) |

**Takeaway:** Low-latency USB mirroring and control is free (scrcpy) and commoditised, and Escrcpy and QtScrcpy already ship Windows GUIs on top of it. The gaps are (a) a **Windows-native, zero-config experience that guides non-developers through USB debugging**, (b) **free control without an account**, which AirDroid Cast charges for (Standard tier), and (c) **Pixel, Motorola and other phones without Link to Windows**, which Phone Link leaves out. DeX for PC ended with One UI 7. Samsung users were moved to Phone Link, which mirrors on Galaxy over Wi-Fi only, with no USB connection.

## 2. Problem
Windows users with Android phones can't get a fast, reliable, any-brand way to see and control their phone on the PC without a CLI tool (scrcpy), a brand lock-in (Phone Link), or a subscription (AirDroid Cast for control, Vysor Pro for its premium features).

## 3. Target users: all three (Q1). **CONFIRM:** sequencing below

| Segment | Pros | Cons |
|---|---|---|
| **A. Developers / QA / support** | Already have USB debugging on, so onboarding risk is near zero. Value latency, multi-device and scripting. Will pay for team features | scrcpy already serves them well and for free. Small market; GUI is a weak differentiator |
| **B. Everyday Windows users (non-Samsung)** | Large market; Phone Link leaves them out. A polished UX is a real advantage | USB-debugging onboarding is the funnel killer (REVIEW A3). Expect low willingness to pay and high support load |
| **C. Streamers / presenters / educators** | Need low latency, a clean capture window, audio and OBS integration. Some will pay | Niche. Needs audio, a capture window and wireless soon after MVP |

**Proposed sequencing.** Serving all three is the goal, but every design trade-off (defaults, onboarding, UI density) needs one tie-breaker segment:
- **MVP (Stages 1–7): design for B.** Guided onboarding and safe defaults help everyone. A already has USB debugging on, so A gets the same product with less friction; add a compact "advanced" panel (bitrate, codec, max size) for A.
- **Post-MVP: C.** Their must-haves (audio, borderless capture window, OBS presets) build on the scrcpy audio the server already supports.
- **Later: A's team features** (multi-device dashboard, batch actions), which only make sense once a business model exists.

## 4. Wedge feature (Q2): **DECISION NEEDED**

*A wedge is the one thing PhoneBridge does clearly better than the free alternatives, the reason someone switches from scrcpy or Phone Link. Mirroring and control themselves are not a wedge, because scrcpy already does them for free; the wedge is what gets users in the door and that we make excellent first.*

**Recommendation: #1 onboarding, with #4's quick wins as the supporting set.** It fits sequencing B-first, it's where every competitor is weak, and it's mostly desktop UI work rather than protocol risk.
1. **Guided zero-config onboarding (→ B):** a desktop wizard per manufacturer (Samsung, Pixel, Xiaomi, OnePlus, Motorola), live detection of "unauthorized" state, first mirror in under 3 minutes. Nobody does this well.
2. **Multi-device dashboard and batch actions (→ A):** see and control N phones, run APK install, shell scripts and screenshots across all of them. Competes with QtScrcpy/Escrcpy, so it must be more polished and Windows-native.
3. **Stream-ready output (→ C):** a borderless capture window or virtual camera, audio passthrough, OBS-friendly presets, a latency HUD.
4. **"Phone Link for everyone else" (→ B):** clipboard sync, drag-and-drop file/APK, screen-off-while-mirroring, all free and with no account. Cheap to build (REVIEW A4) but easy to copy.

## 5. MVP scope: mirroring + control, no companion (Q4, Q6 approved)
- Mirror and control (not one-way "casting") of any ADB-authorised Android 5.0+ device **over USB**, using the unmodified scrcpy-server pinned at v5.0.1.
- **No companion app** (REVIEW A2). Pairing relies on ADB's RSA prompt.
- Desktop onboarding guide, clipboard sync both ways, drag-and-drop APK install / file push, screen-off and stay-awake.
  - APK install and file push are **not server features**. In scrcpy they run on the client through adb (`sc_adb_install` / `sc_adb_push` in `app/src/file_pusher.c` @ v5.0.1). After a push, the client sends a scan-file control message so the media library picks the file up. Files go to `/sdcard/Download/` by default (`--push-target`, `doc/control.md` @ v5.0.1). PhoneBridge's Rust client must reimplement all of this.
  - Stay-awake only works while the device is plugged in; over TCP/IP it does nothing. Screen-off is best-effort: the physical POWER button always turns the screen back on (`doc/device.md` @ v5.0.1).
- Windows 10/11 x64 only.

### 5.1 Trade-offs of dropping the companion app (Q6)
| We gain | We give up (until the companion returns in Stage 8) |
|---|---|
| Nothing to install on the phone: plug in, approve the USB-debugging prompt, mirror | **Phone-only features:** notifications, SMS and call handling on the PC. These need an app on the phone. Phone Link has them, so B users may compare unfavourably |
| One approval prompt (ADB's RSA) instead of two; less onboarding to explain | **Clipboard only while mirroring.** scrcpy syncs the clipboard only while the server runs; a companion could sync in the background |
| ~1 week less in Phase 1, no Play Store / sideload / Android 12–14 background-service work | **Wi-Fi discovery and pairing.** Wireless (post-MVP) needs ADB's own wireless pairing (Android 11+, pairing code) instead of a branded flow |
| No home-made pairing crypto to get wrong (REVIEW B11) | **No branded phone presence:** no app icon or status notification on the phone saying "Connected to PC" |
| Works on any Android 5.0+ device the scrcpy server supports | **USB debugging stays mandatory.** The companion couldn't remove that requirement either, since scrcpy needs ADB, so this isn't a loss but it remains the biggest onboarding hurdle |

**When to bring it back:** when a roadmap feature needs it, most likely notifications/SMS (to compete with Phone Link for B users) or background clipboard sync.
## 6. Non-goals (MVP)
iOS. macOS and Linux (macOS is on the roadmap after 1.0, Q5; keep adb/process/packaging code behind OS-specific modules so the port doesn't need a rewrite). Wi-Fi/remote-over-internet. Notifications, SMS and calls (these need the companion). Root features. Cloud accounts. Audio forwarding (unless segment C is chosen). Game key mapping.

## 7. Success metrics (from REVIEW A1; **DECISION NEEDED:** confirm targets)
| Metric | Target | Measured by |
|---|---|---|
| Glass-to-glass latency, USB, 1080p60 | **≤ 70 ms p50 / ≤ 120 ms p95** | Phase 0E (240 fps camera) on the test matrix |
| Install → first mirror | **< 3 min** (median, new user, USB debugging off at start) | Moderated usability test, n ≥ 5 per segment |
| Crash-free sessions | **> 99 %** | Crash reporting (needs a telemetry decision, §8) |

## 8. Business model (Q3): **free for now**; freemium/paid later

Still open: **open-source free** (trust, contributions, but competitors can copy it and a later paid tier is harder to explain) vs **closed-source free** (keeps options open for freemium). Either way, budget for a code-signing certificate before public release, because SmartScreen warns on unsigned installers even for free apps.

Options for later:
| Option | Implication |
|---|---|
| **Free / OSS** (Apache-2.0, same as scrcpy) | Easiest adoption and community trust; no revenue. Code signing still costs money |
| **Freemium** (free mirror + control; paid multi-device, recording, streaming tools) | Must not paywall control: AirDroid Cast charging for it (Standard tier) is our opening. Needs licensing, an update channel and opt-in telemetry |
| **Paid one-time** (e.g. ~$10–20) | Simple. Hard to justify against free scrcpy unless the wedge is strong |
| **B2B / per-seat** (segment A teams) | Highest revenue per user; needs device-fleet features and support commitments |

## 9. Open questions (REVIEW Part D)
1. ~~Who is the primary user?~~ All three; **confirm** the sequencing in §3.
2. **DECISION NEEDED:** What is the wedge? (Recommendation in §4.)
3. ~~Free, freemium, or paid?~~ Free for now. **Open:** open-source or closed-source?
4. ~~Casting or mirroring + control?~~ Mirroring + control.
5. ~~Windows-only?~~ Windows only for now; macOS on the roadmap.
6. ~~Drop the companion from the MVP?~~ Yes (trade-offs in §5.1).

**Approval:** Product owner ______________ · Date ________ · Chosen segment ___ · Chosen wedge ___ · Model ___
