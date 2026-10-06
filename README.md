# Rinx

English | [简体中文](README.zh-CN.md)

A native Matrix messenger from [Upstream Labs](https://github.com/upstreamlabs), with a WeChat-style interface, English/Chinese support, Moments, and scoped mini apps.

---

## 🏆 Hackathon Entry: Writing Studio

This repository is our entry for the Agentic App Hackathon. The submission app is **Writing Studio**.

- 📖 App docs: [apps/writing-studio/README.md](apps/writing-studio/README.md)
- 📸 Screenshots: [apps/writing-studio/bundle/screenshots/](apps/writing-studio/bundle/screenshots/)
- 🎬 Demo video: [apps/writing-studio/bundle/基础功能演示.mp4](apps/writing-studio/bundle/基础功能演示.mp4)
- 🏷️ Submission tag: `v1.0.0`

---

Rinx is an independent continuation of [`OctoSense-org/robrix2`'s `wechat-ui` branch](https://github.com/OctoSense-org/robrix2/tree/wechat-ui), starting at `16913e0c6f0397b4ed1272b7dcd421935521d5f3`. That branch's commit history is preserved here. Development now lives on this repository's **`main`** branch.

## Features

- Chats, Contacts, Discover, and Me, backed by Matrix messaging and encrypted rooms.
- Room discovery, optional Spaces, per-room search and attachments, hidden chats, and message forwarding.
- Matrix-backed Moments, separate from self-DM file transfer.
- A Markdown article editor with images, covers, themes, full preview, and publish/retract workflows.
- Native HTML/CSS article previews through [makepad-html](https://github.com/OctoSense-org/makepad-html) and Blitz, rendered into Makepad without a WebView.
- Mini-app consent scoped to the signed-in user, plus optional Hagency integration.

The renderer remains experimental: grayscale/sepia filters have known failures; arbitrary WeChat HTML import and complete WeChat compatibility are not claimed. See the [integration evidence](lab/article-html-integration/README.md) and [mini-app authority design](docs/adr/0002-octoscript-mini-app-authority.md).

## Built-in apps

Rinx bundles native and OctoScript apps through `system-apps.json`. The native article editor lives under `apps/article-editor/`; both deployments share its catalog and host-service contracts. See the [app development guide](apps/README.md) and [ADR 0008](docs/adr/0008-rinx-system-app-catalog.md).

## Build and run

Install Rust and CMake. The repository pins Rust 1.98.0. On macOS:

```sh
brew install cmake
git clone https://github.com/upstreamlabs/Rinx.git
cd Rinx
cargo run --locked --features agent_chat
```

The executable is `rinx`, and the macOS app is `Rinx.app`. HTML/CSS preview is enabled by default. Hagency is compiled with `agent_chat` and must also be enabled in Settings → Preferences.

Rinx needs a Matrix homeserver supporting native Sliding Sync. Enter account credentials directly in the app. On Linux, install the native dependencies listed in the [inherited build guide](docs/robrix-upstream-readme.md#building--running-robrix-on-desktop); use `rinx` wherever that historical guide names the package or executable `robrix`. Mobile packaging scripts have been renamed for Rinx but require your own signing configuration and device validation.

HTTP(S) link previews follow Robrix's native card implementation: the homeserver's
`/_matrix/client/v1/media/preview_url` endpoint supplies the title, description,
and thumbnail. The homeserver must permit URL previews. If Palpo returns
`403 M_FORBIDDEN` with `URL is not allowed to be previewed`, add the desired domains
to its [URL-preview configuration](https://github.com/palpo-im/palpo/blob/v0.4.0/palpo-example.toml):

```toml
[url_preview]
domain_explicit_allowlist = ["github.com", "example.org"]
```

Apply that configuration on the homeserver, then restart Rinx to clear failed
preview requests cached by open timelines. Failed requests are logged as
`Homeserver link preview failed`; the original URL remains usable.

Chat links and preview cards open in an in-app web reader on macOS, iOS, and
Android. On desktop, the reader opens in a separate Rinx window so the conversation
remains usable; on mobile, it opens as an in-app panel. Its SVG toolbar provides
Close, Back, Forward, Reopen link, and Open in browser, with hover labels. Closing
the reader leaves the chat open. Each opened chat link gets its own tab, preserving
its page, scroll position, and navigation history when switching tabs. A tab's
close button closes only that tab; closing the last tab closes the reader.
The header identifies the selected tab's original opened link, and Reopen link
returns to it. The embedded reader uses
the platform browser engine and does not depend on the homeserver preview API.

Markdown attachments (`.md`, `.markdown`, or a Markdown MIME type) have an Open
button alongside Download and Share. They use Matrix's authenticated media
download, including decryption, and open as native document tabs in the same
reader. Shared article cards also open there. Documents keep their own scroll
position; their HTTP links open web tabs. The article editor remains separate.

Open reader tabs and the selected tab are saved per account and restored after
restarting Rinx. Closing a tab or the reader explicitly removes it from the saved
session. Restored webpages load their original chat links; browser navigation
history and scroll positions are retained only while Rinx stays running.
Markdown and article tabs reload through Matrix, without saving document bodies
in the reader session file.

```sh
cargo test --locked --features agent_chat --lib
cargo test --locked --manifest-path crates/article-core/Cargo.toml
python3 tools/wechat-ux/check_i18n.py
```

The main CI workflow builds and tests the native app and portable article core.
The inherited multi-platform build and license-refresh workflows are available
by manual dispatch; release signing and publishing require Rinx-specific secrets.

## Data and compatibility

Rinx has its own application identity, `org.octosense.rinx`, and its own default data directory. It does not automatically migrate an existing Robrix login or profile. `RINX_DATA_DIR` selects an absolute path for an isolated profile; legacy `ROBRIX_DATA_DIR` remains a fallback for existing test tooling. `RINX_DATA_DIR` takes precedence.

Existing `rs.robius.robrix.*` Matrix event types are retained so shared articles, mini apps, forwarded messages, and Moments remain interoperable. The app's sign-in callback uses `rinx://login`.

## 致敬 Robrix · Acknowledgements

Rinx 致敬 [Robrix](https://github.com/project-robius/robrix)、[Robrix2](https://github.com/Project-Robius-China/robrix2)、Kevin Boos、Project Robius 及所有贡献者。感谢他们为原生 Rust Matrix 客户端打下的基础。

Rinx builds on their work and on [Makepad](https://github.com/makepad/makepad), [Robius](https://github.com/project-robius), [Matrix Rust SDK](https://github.com/matrix-org/matrix-rust-sdk), [Blitz](https://github.com/DioxusLabs/blitz), and [Octoscript](https://github.com/OctoSense-org/Octoscript). Original authorship and commit history are preserved.

## License

Rinx is distributed under **[Apache-2.0](LICENSE)**. Inherited Robrix code retains its original **[MIT copyright and permission notice](LICENSE-MIT)**. See [NOTICE](NOTICE) for attribution and [third-party notices](licenses/THIRD-PARTY-NOTICES.html) for bundled dependencies; their licenses remain in force.
