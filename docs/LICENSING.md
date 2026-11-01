# PhoneBridge: third-party licensing obligations (Windows distribution)

**Status:** research notes for DEVELOPMENT_PLAN Stage 0 ("Check licensing: scrcpy Apache-2.0 NOTICE; ADB redistribution terms") and the "Licensing" bullet of REVIEW.md B15.
**Researched:** 2026-11-01. Pinned scrcpy: **v5.0.1** (server asset `scrcpy-server-v5.0.1`, SHA-256 `764eb6f7…bc536`). Pinned platform-tools: **r37.0.1** (the version scrcpy v5.0.1 bundles).

> **Not legal advice.** The author is not a lawyer. This document collects primary-source text and separates **Fact** (what a cited source says) from **Interpretation** (our reading). Get a qualified lawyer to review it before any commercial release, especially §3 (ADB) and §9 (open questions).
>
> Labels used below:
> - **Fact:** quoted or closely paraphrased from the cited URL, as read on 2026-11-01.
> - **Interpretation:** our reasoning. It may be wrong.
> - **UNVERIFIED:** believed true but not confirmed from a primary source in this pass.

---

## 1. What PhoneBridge ships (inventory)

| Component | How it reaches the user | Licence (headline) | Section |
|---|---|---|---|
| `scrcpy-server` v5.0.1 (unmodified JAR; pushed to the phone at runtime) | Bundled in the installer as a resource | Apache-2.0 | §2 |
| `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll` (platform-tools r37.0.1) | Bundled in the installer (REVIEW B4/B10, plan test 0H) | Android SDK License Agreement + open-source components | §3 |
| Tauri v2 runtime + Rust crate graph (tauri, wry, tao, webview2-com, windows-rs, tokio, serde, rand, tracing, thiserror, …) | Compiled into `phonebridge.exe` | Mostly MIT / Apache-2.0; the full graph must be scanned | §4 |
| `WebView2LoaderStatic.lib` (via `webview2-com-sys`) | Statically linked into `phonebridge.exe` | Microsoft WebView2 SDK licence (BSD-3-Clause-style) | §5 |
| WebView2 Runtime | **Not shipped** by default (Tauri `downloadBootstrapper`), or optionally embedded | Microsoft Software License Terms | §5 |
| React 19, react-dom, scheduler, `@tauri-apps/api`, Vite runtime helpers | Bundled JS in the app's web assets | MIT (React); MIT/Apache-2.0 (Tauri JS) | §6 |
| NSIS installer stub + Tauri NSIS plugins | The `*-setup.exe` itself | zlib/libpng (NSIS) + CPL-1.0 (default LZMA module); MIT/Apache-2.0 (Tauri) | §7 |
| MSVC C runtime | Statically linked into `phonebridge.exe` by tauri-build (default `staticVCRuntime`); the UCRT is part of Windows 10/11 | Nothing to redistribute for PhoneBridge itself | §7 |

DevDependencies (eslint, typescript, vitest, vite itself, `@tauri-apps/cli`) are build tools. They are **not** redistributed, so they create no notice obligations, except for any helper code Vite/Rollup injects into the bundle. Interpretation.

---

## 2. scrcpy-server (Apache-2.0)

### Facts
- scrcpy is "Licensed under the Apache License, Version 2.0". Copyright lines in the repo LICENSE appendix and README: `Copyright (C) 2018 Genymobile` and `Copyright (C) 2018-2026 Romain Vimont`.
  Sources: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/LICENSE ; https://github.com/Genymobile/scrcpy (README "Licence").
- **No `NOTICE` file exists at the v5.0.1 tag.** `https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/NOTICE` returns HTTP 404, and the repo root listing (LICENSE, README.md, FAQ.md, build files, app/, server/, doc/, release/ …) shows none.
- The server has **no third-party runtime dependencies**. `server/build.gradle @ v5.0.1` declares only `testImplementation 'junit:junit:4.13.2'`. It compiles against the Android framework, which is on the device and is not bundled.
  Source: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/server/build.gradle
- The release asset is a bare file named `scrcpy-server-v5.0.1` (no `.jar` extension). Its SHA-256 `764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536` is listed in `SHA256SUMS.txt` (signed with `SHA256SUMS.txt.asc`).
  Source: https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/SHA256SUMS.txt
- Apache-2.0 §4 (redistribution) requires you to: (a) give recipients a copy of the License; (b) mark modified files with prominent notices; (c) retain copyright, patent, trademark and attribution notices in the Source form; (d) if the Work includes a NOTICE file, include its attribution notices. §6 grants no trademark rights beyond describing the origin of the Work. §3 grants a patent licence, which terminates if you start patent litigation over the Work.
  Source: https://www.apache.org/licenses/LICENSE-2.0

### Obligations for PhoneBridge (interpretation)
1. **Ship the Apache-2.0 licence text** and the two scrcpy copyright lines in `THIRD_PARTY_NOTICES` (and in the in-app About → Licences screen).
2. **NOTICE (§4(d)) does not apply today**, because v5.0.1 has no NOTICE file. Re-check on every scrcpy version bump. If a NOTICE file appears, its contents must be reproduced.
3. **Modifications (§4(b)):** PhoneBridge ships the server **unmodified**, and the SHA-256 is pinned (REVIEW B15). Nothing to mark. If we ever patch or rebuild the server, we must mark the changed files and should rename the artifact so nobody mistakes it for upstream.
4. **Our Rust client:** it is hand-written from the protocol (doc/develop.md, the server sources). If any code is *ported* from scrcpy's C client (`app/src`, also Apache-2.0), keep the attribution header on those files and list scrcpy in the notices. Either way, crediting scrcpy is cheap and recommended. Whether a protocol-compatible clean-room client is a derivative work is a lawyer question (§9, Q5). Apache-2.0 is permissive, so the practical risk is low.
5. **Trademark (§6):** do not use "scrcpy" or "Genymobile" in the product name or logo. Nominative use ("uses scrcpy-server by Genymobile / Romain Vimont") in About/README is the normal practice.
6. **Source offer:** Apache-2.0 has **no** source-distribution requirement. A link to the upstream tag is courtesy, not an obligation.

---

## 3. ADB / Android SDK Platform-Tools

This is the component with the most legal uncertainty.

### 3.1 Facts: the governing terms
- The platform-tools download page requires accepting the **"Android Software Development Kit License Agreement"** (dated **April 28, 2026**) via a checkbox before each download. The page has **no statement specific to redistributing adb or fastboot**. Latest revision: **37.0.1 (July 2026)**.
  Source: https://developer.android.com/tools/releases/platform-tools
- Android SDK License Agreement (same text at https://developer.android.com/studio/terms, dated April 28, 2026):
  - **§1.1** The SDK "specifically including the Android system files, packaged APIs, and Google APIs add-ons" is licensed under the Agreement.
  - **§3.1** grants a "limited, worldwide, royalty-free, non-assignable, non-exclusive, and non-sublicensable license to use the SDK solely to develop applications for compatible implementations of Android."
  - **§3.2** "You may not use this SDK to develop applications for other platforms (including non-compatible implementations of Android) or to develop another SDK. You are of course free to develop applications for other platforms, including non-compatible implementations of Android, provided that this SDK is not used for that purpose."
  - **§3.3** "Google reserves all rights not expressly granted to you."
  - **§3.4** "You may not use the SDK for any purpose not expressly permitted by the License Agreement. Except to the extent required by applicable third party licenses, you may not copy (except for backup purposes), modify, adapt, **redistribute**, decompile, reverse engineer, disassemble, or create derivative works of the SDK or any part of the SDK."
  - **§3.5** "Use, reproduction and distribution of components of the SDK licensed under an open source software license are governed solely by the terms of that open source software license and not the License Agreement."
  - **§9.3** Google may terminate on breach (and other grounds). **§14.7** California law, Santa Clara County courts.

### 3.2 Facts: the open-source status of the adb components
- **adb source** (`platform/packages/modules/adb`) contains `MODULE_LICENSE_APACHE2` and a `NOTICE` file. The `NOTICE` (main) is the Apache License 2.0 text with the line "Copyright (c) 2006-2009, The Android Open Source Project".
  Sources: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/ ; https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/NOTICE
- **AdbWinApi.dll / AdbWinUsbApi.dll** source lives in `platform/development/host/windows/usb/` (`adb_winapi_test/`, `api/`, `winusb/`, `android_winusb.inf`, `NOTICE`, `readme.txt`). Its `NOTICE` (main) is the full Apache License 2.0 text (Version 2.0, January 2004), with only the appendix placeholder copyright line and no named holder. So the AdbWinApi/AdbWinUsbApi sources are Apache-2.0 at main.
  Sources: https://android.googlesource.com/platform/development/+/refs/heads/main/host/windows/usb/ ; https://android.googlesource.com/platform/development/+/refs/heads/main/host/windows/usb/NOTICE
- **Linked libraries:** in `adb/Android.bp` (main), `adb` is a `cc_binary_host` using `adb_binary_host_defaults`. Its `target.windows` block sets `enabled: true`, `shared_libs: ["AdbWinApi"]` and `required: ["AdbWinUsbApi"]`, and removes nothing from the common list. (The `target` block has only `darwin` and `windows` entries.) The common `static_libs` are: `libadb_crypto`, `libadb_host_protos`, `libadb_pairing_auth`, `libadb_pairing_connection`, `libadb_protos`, `libadb_sysdeps`, `libadb_tls_connection`, `libandroidfw`, `libapp_processes_protos_full`, `libbase`, `libbrotli`, `libcrypto`, `libcrypto_utils`, `libcutils`, `libdiagnose_usb`, `libfastdeploy_host`, `liblog`, `liblz4`, `libmdnssd`, `libopenscreen-discovery`, `libopenscreen-platform-impl`, `libprotobuf-cpp-full`, `libssl`, `libusb`, `libutils`, `libz`, `libziparchive`, `libzstd`. Third-party (non-AOSP-Apache) ones among these: BoringSSL (`libcrypto`/`libssl`), brotli, lz4, zstd, protobuf, mDNSResponder (`libmdnssd`), Chromium Open Screen (`libopenscreen-*`, BSD-3-Clause-style), zlib (`libz`), libusb.
  Source: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/Android.bp
- **This is `main`, not the shipped release.** AOSP `packages/modules/adb` has `platform-tools-*` tags, but the newest visible in the refs listing is `platform-tools-35.0.2`. No 36.x/37.x tag was found, so r37.0.1's exact build files could not be read. The 37.0.1 release notes say the `openscreen` mDNS backend was deleted and that on Windows "`libadbusb`" was introduced "to replace `libusb`" (legacy path via `ADB_USB_LEGACY=1`). So the r37.0.1 static-lib set may differ from main. The authoritative list for what we ship is the zip's own `NOTICE.txt` (§8.1).
  Sources: https://android.googlesource.com/platform/packages/modules/adb/+refs ; https://developer.android.com/tools/releases/platform-tools (37.0.1 notes)
- **libusb** in AOSP (`platform/external/libusb`) carries `MODULE_LICENSE_LGPL` and `COPYING`. Its `Android.bp` (main) license module `external_libusb_license` declares `SPDX-license-identifier-GPL`, `-LGPL`, `-LGPL-2.1`, `-LGPL-3.0` and `-MIT`, with `license_text: ["COPYING", "NOTICE"]`. `libusb_defaults` sets `host_supported: true`, and `target.windows` has `enabled: true` with sources `libusb/os/events_windows.c`, `threads_windows.c`, `windows_common.c`, `windows_usbdk.c`, `windows_winusb.c`.
  Sources: https://android.googlesource.com/platform/external/libusb/+/refs/heads/main/ ; https://android.googlesource.com/platform/external/libusb/+/refs/heads/main/Android.bp
  **Fact (main):** libusb is built for Windows and listed in the common `static_libs` of `adb_binary_host_defaults`, so a main-branch Windows `adb.exe` statically links LGPL libusb. **UNVERIFIED:** whether the r37.0.1 build still links it (see the `libadbusb` release note above; "replace" with a legacy fallback suggests it may still be linked). Treat it as present unless the r37.0.1 `NOTICE.txt` shows otherwise (see §9, Q3). The GPL kind declared by the license module needs a file-level check to see which files it covers and whether any are compiled into adb.
- **UNVERIFIED:** the platform-tools zip ships a `NOTICE.txt` (and `source.properties`) at its root that aggregates the open-source notices. This is from memory. The zip was not downloaded, per the task rules.

### 3.3 Facts: precedent
- scrcpy's own Windows release **bundles adb**. `app/deps/adb_windows.sh @ v5.0.1` downloads `https://dl.google.com/android/repository/platform-tools_r37.0.1-win.zip` (SHA-256 `45f4d63113e895ebde0c90f194099a4676b6ac653bd28d54314a9e022bbc1a99`) and extracts exactly `adb.exe`, `AdbWinApi.dll` and `AdbWinUsbApi.dll`. `release/build_windows.sh @ v5.0.1` copies them into the release zip, together with scrcpy's own `LICENSE` (as `LICENSE.txt`). **The script does not extract `NOTICE.txt`**, so scrcpy does not appear to ship Google's notices.
  Sources: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/app/deps/adb_windows.sh ; https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/release/build_windows.sh
- `doc/windows.md @ v5.0.1` says that for WinGet, "ADB and other dependencies will be installed alongside scrcpy". For Chocolatey and Scoop it lists `choco install adb` / `scoop install adb` "if you don't have it yet".
  Source: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/doc/windows.md
- Bundling platform-tools is widespread among open-source and commercial Android desktop tools. **Precedent is not permission.** We found no published Google statement that authorises third-party redistribution of platform-tools binaries, and none that forbids it beyond §3.4.

### 3.4 Interpretation
- Two readings conflict:
  - **(A) Restrictive:** platform-tools as downloaded is "the SDK or any part of" it. §3.4 opens with "You may not use the SDK for any purpose not expressly permitted by the License Agreement", then forbids redistribution. §3.1 permits use only "to develop applications for compatible implementations of Android". Running adb inside an end-user screen-mirroring tool is arguably not a permitted purpose, and shipping it is redistribution.
  - **(B) Open-source:** adb, AdbWinApi and AdbWinUsbApi are built from AOSP sources marked Apache-2.0. Under §3.5, their "use, reproduction and distribution … are governed **solely**" by the open-source licence. Redistribution is then allowed under Apache-2.0 §4, with licence text and NOTICE, plus LGPL obligations for libusb (statically linked at main, §3.2).
- Reading (B) is the one the ecosystem relies on, and it is plausible. But: (i) §3.5 talks about *components licensed under* an OSS licence, and Google's *binaries* are distributed under the SDK LA wrapper; (ii) the statically linked parts carry other licences (BoringSSL: OpenSSL/ISC-style; mDNSResponder: Apache-2.0; zstd/lz4/brotli: BSD/MIT; protobuf: BSD-3; Open Screen: BSD-3-Clause-style; zlib: Zlib; libusb: LGPL-2.1+). Each needs its notice. This list comes from `adb_binary_host_defaults` at main and must be re-checked against the r37.0.1 zip's `NOTICE.txt`. This is a lawyer question (§9, Q1).
- **§3.2**: its second sentence ("You are of course free to develop applications for other platforms … provided that this SDK is not used for that purpose") makes clear that building a Windows app is allowed in itself. The real question is whether shipping or running adb inside PhoneBridge counts as "using the SDK" for a purpose §3.1 does not permit (§3.4 first sentence). We read this as low risk under (B), but list it (§9, Q2).

### 3.5 Risk-reducing options (engineering, ordered by legal safety)
1. **Build adb from AOSP source ourselves.** Then it is unambiguously Apache-2.0 (plus dependency licences), and the SDK LA never attaches. Cost: an AOSP host build for Windows is heavy.
2. **Don't bundle; use the user's adb.** REVIEW B10 option (a) already has PhoneBridge talk to an existing adb server over TCP 5037. Ship without adb, detect an existing server or adb on PATH (B10 option (c)), and otherwise prompt the user to install platform-tools (or run `winget install Google.PlatformTools`, UNVERIFIED package id) and accept Google's terms themselves. Cost: onboarding friction.
3. **Download at first run from `dl.google.com`.** Show Google's SDK LA and require the user to accept it (as the platform-tools page does), verify the pinned SHA-256, then extract the three files. We never redistribute. Cost: network needed. Showing the terms in-app may itself need review.
4. **Bundle (status quo in the plan, as scrcpy does)** with full notices (Apache-2.0 + AOSP NOTICE contents + libusb LGPL text and relinking material or a written offer; libusb is statically linked at main, §3.2). This is the most common practice and has the most residual uncertainty.

Recommendation (interpretation): design for (2), with (3) as the fallback, for a commercial release. (4) is acceptable for a free/open beta if counsel agrees.

**Plan impact (interpretation).** The current plan assumes adb is bundled: DEVELOPMENT_PLAN Stage 1 test 0H ("`adb.exe` starts with its DLLs"), the Stage 2 `adb/` fallback that "spawns the bundled adb only to *start* a server", REVIEW B4 (DLL layout) and B10 option (a). Under option (2), the TCP-5037 client has no binary of its own to start a server. So: 0H and B4 apply only to an optional bundled build; the Stage 2 fallback must start the user's `adb.exe` (PATH, or `ANDROID_HOME`/`platform-tools`) or the copy downloaded at first run (option 3); and a test for the "no adb installed" onboarding path must be added.

---

## 4. Tauri and the Rust crate graph

### Facts
- Tauri's code is licensed "MIT or MIT/Apache 2.0 where applicable". The repo has `LICENSE-APACHE-2.0`, `LICENSE-MIT` and `LICENSE.spdx`. The Tauri **logo** is CC-BY-NC-ND, separate from the code licence. Source: https://github.com/tauri-apps/tauri
- `webview2-com` / `webview2-com-sys` (used by wry on Windows) are MIT. "By default this crate uses the `WebView2LoaderStatic.lib` static library." Source: https://github.com/wravery/webview2-rs
- Direct dependencies in `app/src-tauri/Cargo.toml`: tauri, tauri-build, serde, serde_json, tokio, rand, thiserror, tracing, tracing-subscriber. The transitive graph (windows-rs, tao, wry, muda, etc.) is much larger.

### Obligations (interpretation)
- MIT requires the copyright and permission notice in "all copies or substantial portions". A compiled binary counts, so every MIT crate's notice goes into `THIRD_PARTY_NOTICES`. Apache-2.0 crates need the licence text, plus any NOTICE files they carry.
- Expect a few non-MIT/Apache licences in a Tauri graph: `Unicode-3.0` (unicode-ident), possibly `BSD-3-Clause`, `Zlib`, `ISC`, `MPL-2.0`, `CC0-1.0`. **UNVERIFIED** for our exact lockfile. MPL-2.0 would add file-level source-availability duties for modified MPL files only.
- Don't use the Tauri logo in PhoneBridge branding (CC-BY-NC-ND forbids commercial and derivative use).

### Tooling
- **cargo-about** (MIT/Apache-2.0, EmbarkStudios): `cargo install --locked --features cli cargo-about`, then `cargo about init` (creates `about.toml` + `about.hbs`), then `cargo about generate about.hbs > THIRD_PARTY_NOTICES.rust.html`. List accepted licences in `about.toml` so the build fails on anything new. Sources: https://github.com/EmbarkStudios/cargo-about ; https://embarkstudios.github.io/cargo-about/
- **cargo-deny** (`cargo deny check licenses`) as a CI gate with an allow-list (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0, MPL-2.0 after review). Deny GPL/AGPL/LGPL in the Rust graph. Interpretation: an LGPL static link from a Rust crate is awkward to comply with.

---

## 5. Microsoft WebView2

### Facts
- **Runtime availability:** "The Evergreen WebView2 Runtime will be included as part of the Windows 11 operating system." "The vast majority of Windows 10 devices have the WebView2 Runtime installed already." Microsoft recommends checking for it and installing it if missing. Source: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution (updated 2026-10-02).
  Note: Tauri's docs say that on Windows 10 (April 2018 release or later) and Windows 11, "the WebView2 runtime is distributed as part of the operating system". That is stronger than Microsoft's own wording, so rely on Microsoft's. Source: https://v2.tauri.app/distribute/windows-installer/
- **Distribution modes:** Microsoft explicitly allows you to "download the bootstrapper and package it with your WebView2 app", to include the Evergreen Standalone Installer "in your app installer", or to package Fixed Version binaries (over 250 MB). Same source.
- **Tauri `webviewInstallMode`:** `downloadBootstrapper` (default, 0 MB, needs internet), `embedBootstrapper` (~1.8 MB), `offlineInstaller` (~127 MB), `fixedVersion` (~180 MB), `skip`. Source: https://v2.tauri.app/distribute/windows-installer/
- **WebView2 Runtime licence** (https://developer.microsoft.com/microsoft-edge/api/eula/webview2, no version/date on the page). The licence has 15 sections. §2 "Distributable Code": §2(a) you may copy and distribute the object code, and permit your distributors to do so. §2(b) requirements: (i) your app must add significant primary functionality; (ii) distributors and end users must agree to terms that protect the code and Microsoft at least as much as this agreement, **including the notices under §9** ("Required Notices to End Users"); (iii) all code must be acquired directly from Microsoft; (iv) you must **indemnify, defend and hold Microsoft harmless** from claims related to the distribution or use of your applications, except claims based solely on the unmodified distributable code. §2(c) restrictions: (i) no use of Microsoft trademarks suggesting origin or endorsement; (ii) do not modify or distribute its source code so it becomes subject to a copyleft-style licence; (iii) do not provide Microsoft download sites or shortcuts that give third parties access to the software. §4: do not remove or modify Microsoft notices, and do not offer the software stand-alone. §5 "H.264/AVC VIDEO STANDARD": "This product includes H.264/AVC codec technology. MPEG LA LLC requires this notice:" (followed by the MPEG LA notice).
- `WebView2LoaderStatic.lib` is linked into our exe by `webview2-com-sys` by default (see §4). The WebView2 SDK NuGet package (`Microsoft.Web.WebView2`, licence as published for 1.0.4314.35) is under a BSD-3-Clause-style licence: "Copyright (C) Microsoft Corporation. All rights reserved."; binary redistribution must reproduce the copyright notice, conditions and disclaimer in the documentation or other materials; Microsoft's name may not be used to endorse derived products. Source: https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4314.35/License
  **UNVERIFIED:** which SDK version the `webview2-com-sys` release we lock embeds (no `Cargo.lock` is committed yet). The licence is per version, so record it once the lockfile exists.

### Obligations (interpretation)
- With the default `downloadBootstrapper`, we redistribute **no** Microsoft runtime binaries. The user's machine fetches them from Microsoft under Microsoft's terms. That is the lowest-obligation option. **Keep the default.**
- If we switch to `embedBootstrapper`, `offlineInstaller` or `fixedVersion`, all of §2(b) and §2(c) apply. In particular: our **EULA must include terms protecting Microsoft's code** at least as protective as Microsoft's, including the §9 end-user notices (and the §5 H.264/AVC notice); we must obtain the files directly from Microsoft; we must not hand third parties Microsoft download sites or shortcuts; and **we take on an indemnity to defend and hold Microsoft harmless** from claims about distributing or using PhoneBridge (§2(b)(iv)). The indemnity is the commercially heaviest term and is a reason to keep the default.
- Include the WebView2 SDK licence text (BSD-3-Clause-style, Microsoft copyright line above) in `THIRD_PARTY_NOTICES` for the statically linked loader.

---

## 6. Frontend (React etc.)

### Facts / interpretation
- React, react-dom and scheduler are MIT (Meta Platforms, Inc. and affiliates). `@tauri-apps/api` is MIT/Apache-2.0 (Tauri repo licence above). **UNVERIFIED** in this pass. Check each `node_modules/*/package.json` `license` field and LICENSE file via the tools below.
- Vite (production build) concatenates and minifies these packages. Minifiers usually keep `/*! … */` and `@license` comments, but a full notice file is still required, because MIT asks for the full notice and not just a banner.
- **Tooling:**
  - `rollup-plugin-license` (or `vite-plugin-license`) in `vite.config.ts` emits a notices file covering **only modules actually bundled**. That is more accurate than scanning `node_modules`.
  - `npx license-checker-rseidelsohn --production --json` (a maintained fork of the original `license-checker`, which is unmaintained) for an audit of production dependencies. **UNVERIFIED:** current maintenance status of both.
  - Gate CI on an allow-list (e.g. `--onlyAllow "MIT;Apache-2.0;ISC;BSD-2-Clause;BSD-3-Clause;0BSD"`).

---

## 7. Installer and toolchain

- **NSIS:** the generated `*-setup.exe` contains the NSIS stub and one compression module. Fact: NSIS source, plug-ins etc. are under zlib/libpng "with the exception of the compression modules"; the bzip2 module is under the bzip2 licence; "The lzma compression module for NSIS is licensed under the Common Public License version 1.0", with a special exception that lets other code link to it without falling under CPL (the LZMA files themselves stay CPL). Source: https://nsis.sourceforge.io/License. Fact: Tauri's `NsisCompression` default is `Lzma` ("LZMA (default)"). Source: https://docs.rs/tauri-utils/latest/tauri_utils/config/enum.NsisCompression.html. Interpretation: with the default setting our installer contains CPL-1.0 object code, and CPL object-code distribution has conditions (disclaimers, a statement that source is available, keeping notices). Either include the NSIS licence page text (with the CPL/LZMA terms) in `THIRD_PARTY_NOTICES`, or set `bundle.windows.nsis.compression` to `"zlib"` to stay under zlib/libpng only. Tauri's NSIS plugins (`nsis_tauri_utils`) are MIT/Apache-2.0 (UNVERIFIED).
- **MSVC CRT:** nothing to ship for PhoneBridge itself. tauri-build links the VC runtime statically by default (`build > windows > staticVCRuntime`, via the `static_vcruntime` crate). Verified 2026-11-02: the release `phonebridge.exe` imports only the Windows UCRT API sets (`api-ms-win-crt-*.dll`, part of Windows 10/11) and no `vcruntime140.dll`. adb.exe and its DLLs are Google's binaries: check their imports separately if we bundle them.
- **Build-tool licence (commercial):** compiling with Visual Studio Build Tools / Community is subject to Microsoft's VS licence terms. Community edition has organisational-size limits for commercial use. **UNVERIFIED:** current thresholds. Confirm before a company build machine or CI runner is used for commercial releases (§9, Q8).

---

## 8. Concrete checklist

### 8.1 Files to ship (in the install dir, and reachable from Help → About → Third-party licences)
- [ ] `LICENSE.txt`: PhoneBridge's own licence/EULA. For a commercial EULA, if any WebView2 distributable is embedded (§5): add the Microsoft-protective clause, the WebView2 licence §9 end-user notices and §5 H.264/AVC notice, and budget for the §2(b)(iv) indemnity of Microsoft. Also add a clause that third-party components are under their own licences.
- [ ] `THIRD_PARTY_NOTICES.txt` (or `.html`), a single merged file containing:
  - [ ] scrcpy: Apache-2.0 full text + "Copyright (C) 2018 Genymobile / Copyright (C) 2018-2026 Romain Vimont" + "scrcpy-server v5.0.1, unmodified, https://github.com/Genymobile/scrcpy/releases/tag/v5.0.1".
  - [ ] Android platform-tools (if bundled): Apache-2.0 text + the contents of platform-tools' `NOTICE.txt` (or AOSP adb `NOTICE` with "Copyright (c) 2006-2009, The Android Open Source Project" + `development/host/windows/usb/NOTICE`) + LGPL-2.1 text for libusb plus relinking material or a written offer (statically linked at main; drop only if the r37.0.1 `NOTICE.txt` shows it absent) + the notices of statically linked libs (BoringSSL, zstd, lz4, brotli, protobuf, mDNSResponder, Open Screen (BSD-3-Clause-style), zlib). This list is from `adb_binary_host_defaults` at main; re-check it against the r37.0.1 zip.
  - [ ] Rust crates: output of `cargo about generate`.
  - [ ] JS bundle: output of `rollup-plugin-license`.
  - [ ] Microsoft WebView2 SDK licence (static loader): BSD-3-Clause-style text with "Copyright (C) Microsoft Corporation. All rights reserved."
  - [ ] NSIS licence page text including the CPL-1.0 terms for the LZMA module (required with Tauri's default LZMA compression; with `compression: "zlib"` only the zlib/libpng notice is needed) + Tauri NSIS plugin notices.
- [ ] Keep `adb.exe`, `AdbWinApi.dll` and `AdbWinUsbApi.dll` **byte-identical** to the pinned platform-tools r37.0.1 zip (SHA-256 of the zip: `45f4d631…1a99`). Record each file's SHA-256 in the repo.
- [ ] Keep `scrcpy-server` byte-identical to SHA-256 `764eb6f7…bc536`.
- [ ] Copy the platform-tools `NOTICE.txt` from the zip into the build (scrcpy's script does not, so don't copy that gap).

### 8.2 Build / CI
- [ ] `app/src-tauri/about.toml` + `about.hbs`. A CI step regenerates the Rust notices and fails if the output differs from the committed file.
- [ ] `cargo deny check licenses` with an allow-list (§4).
- [ ] Vite: `rollup-plugin-license` writes `dist/THIRD_PARTY_NOTICES.js.txt`. A CI allow-list check on production npm dependencies.
- [ ] A merge script concatenates the scrcpy + adb + Rust + JS + Microsoft sections into the final `THIRD_PARTY_NOTICES.txt`, which is added to `bundle.resources` in `tauri.conf.json`.
- [ ] Keep `bundle.windows.webviewInstallMode` at `downloadBootstrapper` (default) unless counsel approves an embedded mode.
- [ ] On each scrcpy or platform-tools bump: re-check for a NOTICE file in scrcpy, re-read the platform-tools page and the SDK LA date, and update the hashes.

### 8.3 Product / branding
- [ ] Don't use "scrcpy", "Genymobile", "Android" (other than descriptive use per Google brand guidelines, UNVERIFIED URL: https://developer.android.com/distribute/marketing-tools/brand-guidelines), "Microsoft Edge" or the Tauri logo in the product name or icon.
- [ ] About screen: "Uses scrcpy-server (Apache-2.0) by Genymobile and Romain Vimont; Android Debug Bridge (AOSP, Apache-2.0); Tauri (MIT/Apache-2.0)."

---

## 9. Open questions for a lawyer

1. **Platform-tools redistribution:** does SDK LA §3.5 (open-source components governed "solely" by their OSS licence) let a third party redistribute Google-built `adb.exe` / `AdbWinApi.dll` / `AdbWinUsbApi.dll` commercially, despite §3.4's general "may not … redistribute"? Is the widespread practice (scrcpy, package managers) relevant to risk?
2. **SDK LA §3.1/§3.2/§3.4 scope:** building a Windows app is itself allowed (§3.2 second sentence: "You are of course free to develop applications for other platforms … provided that this SDK is not used for that purpose"). The question is whether shipping or running adb inside a consumer Windows app counts as "using the SDK" for a purpose outside §3.1 ("solely to develop applications for compatible implementations of Android"), given §3.4's "You may not use the SDK for any purpose not expressly permitted by the License Agreement". Does that matter if the end user, not us, accepts the SDK LA (option 3.5-(3))?
3. **LGPL libusb inside adb.exe:** libusb is statically linked into the Windows adb binary at AOSP main (§3.2; r37.0.1 unconfirmed). If we bundle adb (option 4), what must we provide (LGPL-2.1 §6: object files for relinking, or a written offer) when we redistribute Google's binary unmodified? Does shipping adb.exe and its DLLs as a separate executable (not linked into PhoneBridge) keep PhoneBridge itself outside LGPL scope? (We believe yes, since it is a separate process, but this needs confirming.)
4. **First-run download (option 3):** if PhoneBridge downloads platform-tools from `dl.google.com` and has the user click "I agree" to the SDK LA in our UI, is that a valid acceptance? Does automating the download conflict with Google's terms of service for that endpoint?
5. **Clean-room client:** is a Rust client written from scrcpy's protocol documentation and server source a derivative work of scrcpy (Apache-2.0)? Interpretation: even if it is, Apache-2.0 only needs attribution. Confirm.
6. **Pushing the server JAR to the user's phone** is distribution to the user. Is in-app/About attribution enough, or must the licence also be shown on the device? (We believe the desktop notice is enough.)
7. **WebView2:** if we ever embed the bootstrapper or a Fixed Version runtime, what exact EULA language satisfies the WebView2 licence §2(b)(ii) (including the §9 notices)? Is the §2(b)(iv) obligation to indemnify, defend and hold Microsoft harmless acceptable, and how is it scoped or insured? Which text version applies (the licence page has no date or version)?
8. **Toolchain licence:** can our CI (GitHub `windows-latest` runner images with Visual Studio preinstalled) be used to build commercial releases under Microsoft's VS licence terms? Do we need VS Professional / Build Tools seats?
9. **Codec patents:** PhoneBridge ships no video decoder. Decoding uses WebView2/Chromium WebCodecs and the OS (Media Foundation), and encoding happens on the phone via MediaCodec. Confirm that we incur no H.264/H.265/AV1 patent-pool obligations. (If a WebView2 runtime is ever embedded, its licence §5 carries an MPEG LA H.264/AVC notice that must be passed on.) Does that change if we add an FFmpeg fallback (DEVELOPMENT_PLAN Stage 1 test 0B fallback)? FFmpeg is LGPL/GPL, and shipping it brings codec patent exposure.
10. **Trademarks:** is "PhoneBridge for Android" or a similar phrase acceptable under Google's Android brand guidelines? Does the name "PhoneBridge" conflict with existing marks? (No clearance search was done.)
11. **Export/sanctions and privacy** (screen content passes over USB only) are out of scope here but should be raised in the same review.

---

## 10. Sources (read 2026-11-01)

- scrcpy LICENSE @ v5.0.1: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/LICENSE
- scrcpy NOTICE @ v5.0.1 (404, none): https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/NOTICE
- scrcpy README / repo root: https://github.com/Genymobile/scrcpy
- scrcpy server build.gradle @ v5.0.1: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/server/build.gradle
- scrcpy SHA256SUMS @ v5.0.1: https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/SHA256SUMS.txt
- scrcpy adb bundling @ v5.0.1: https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/app/deps/adb_windows.sh ; https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/release/build_windows.sh ; https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/doc/windows.md
- Apache License 2.0: https://www.apache.org/licenses/LICENSE-2.0
- Android SDK License Agreement (Apr 28, 2026): https://developer.android.com/studio/terms
- Platform-tools release page: https://developer.android.com/tools/releases/platform-tools
- AOSP adb: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/ and `/Android.bp`
- AOSP Windows USB API (AdbWinApi/AdbWinUsbApi): https://android.googlesource.com/platform/development/+/refs/heads/main/host/windows/usb/
- AOSP libusb: https://android.googlesource.com/platform/external/libusb/+/refs/heads/main/
- WebView2 distribution: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- WebView2 Runtime licence: https://developer.microsoft.com/microsoft-edge/api/eula/webview2
- Tauri licence: https://github.com/tauri-apps/tauri
- Tauri Windows installer / webviewInstallMode: https://v2.tauri.app/distribute/windows-installer/
- webview2-rs (static loader): https://github.com/wravery/webview2-rs
- cargo-about: https://github.com/EmbarkStudios/cargo-about ; https://embarkstudios.github.io/cargo-about/
- AOSP adb NOTICE and refs: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/NOTICE ; https://android.googlesource.com/platform/packages/modules/adb/+refs
- AOSP Windows USB NOTICE: https://android.googlesource.com/platform/development/+/refs/heads/main/host/windows/usb/NOTICE
- AOSP libusb Android.bp: https://android.googlesource.com/platform/external/libusb/+/refs/heads/main/Android.bp
- WebView2 SDK licence (NuGet 1.0.4314.35): https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4314.35/License
- NSIS licence: https://nsis.sourceforge.io/License ; Tauri NSIS compression default: https://docs.rs/tauri-utils/latest/tauri_utils/config/enum.NsisCompression.html
