# drift

**A free and open source, ultra-lightweight desktop client for Microsoft Teams chats
and channels, written in Rust.**

[![CI](https://github.com/domknez/drift/actions/workflows/ci.yml/badge.svg)](https://github.com/domknez/drift/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)

drift is for the way most people actually use Teams: messages with colleagues, a few
channels to keep an eye on, and the odd call. It shows just that, in one calm sidebar,
in a single native process — no browser engine, no helper processes, no idle CPU, and
a fraction of the memory the official app keeps busy doing nothing.

Sibling of [dust](https://github.com/domknez/dust) (music) and
[haze](https://github.com/domknez/haze) (IPTV).

> **Early days.** drift reads and sends messages in your teams, channels and chats.
> Live updates and notifications come next; calls open in Teams itself.

## Features

- **Sign in with Microsoft:** your work or school account, in a small window with
  Microsoft's own sign-in page (MFA included). The sign-in is kept in the system
  keychain (macOS Keychain, Windows Credential Manager, Secret Service) and renewed
  on its own
- **Tabs, not clutter:** Activity, Chats, Teams and Meetings in the top bar, with counts
  of what's unread; today's meeting sits beside them with *Join*
- **One list for what's going on:** chats and channels together, newest first, each
  with its newest message and the time; chips narrow it to unread, channels or
  people, and the search field to a name
- **Chats as bubbles:** others' on the left with their picture, yours on the right;
  runs of messages share one bubble group, with a day pill between days
- **Channels with threads:** posts as bubbles with a link to their thread, which
  opens beside the channel for reading and replying
- **Right-click a message** to react, reply with a quote, reply in its thread, copy
  it or open its link; your own also to edit or delete; click a reaction to add
  yours (or take it back). Reactions and emoji are Teams' own, in colour
- **Write like in Teams:** Markdown (`**bold**`, `*italic*`, `~~strike~~`, `` `code` ``,
  code blocks, lists, `>` quotes, links), `@` to mention a colleague, ↑ in an empty box
  to edit your last message, Esc to cancel a quote or an edit
- **Activity:** who mentioned you or replied, what they wrote and where; a click
  opens that very message, marked (in its thread if it's a reply)
- **Pin and unpin** chats and channels from their right-click menu
- **Read here, read in Teams:** opening a conversation marks it read for Teams too
- **Messages as written:** bold, italics, strikethrough, code, tables, quotes, links,
  mentions and emoji; reactions under them; cards from bots and connectors (pull
  requests, alerts) as one compact row that opens its link
- **Modern UI:** panels floating on a canvas, a deep ink dark and a warm paper light
  theme (or follow the system) with one violet accent, Geist and Geist Mono
- Runs on macOS, Linux and Windows

## Build

```sh
cargo build --release
./target/release/drift
```

Linux needs D-Bus, GTK 3 and WebKitGTK 4.1 development packages (see
`.github/linux-deps.sh`).

## How it works

drift talks to the same services the Teams web app uses: the chat service for
conversations and messages, the chat aggregator for teams, channels and chats, and the
middle tier for people's names. It signs in with the Teams web client's public client
id, so no app registration and no administrator are needed. These services are not a
documented public API: they may change, and your organisation's policies apply to
drift as they do to any Teams client.

## Under the hood

```
src/
├── main.rs        entry point: the app, or the sign-in window (login.rs)
├── backend.rs     the Backend trait the UI talks to, and its errors
├── model/         conversations, teams, channels, messages; message HTML as styled
│                  runs (text.rs); times (time.rs)
├── teams/         Microsoft Teams: signing in and tokens (auth.rs), requests (http.rs),
│                  answers as model types (parse.rs)
├── ui/
│   ├── app.rs        the sign-in page or the workspace, the theme
│   ├── workspace.rs  the signed-in state: sidebar, open conversation, refreshes
│   ├── views/        the top bar, the list, the conversation, the thread, sign-in
│   ├── widgets.rs    avatars, message text, cards, reactions
│   └── style/        palette, type scale, metrics, icons — all visual constants
├── demo.rs        a made-up workspace (DRIFT_DEMO=1) for screenshots
├── login.rs       "Sign in with Microsoft" webview helper (its own process)
├── credentials.rs the sign-in in one OS keychain item
├── settings.rs    settings file
└── log.rs         DRIFT_LOG levels and the log file
```

- **Settings** live in `~/Library/Application Support/drift/` (macOS), `%APPDATA%\drift\`
  (Windows) or `~/.config/drift/` (Linux). The log is `~/Library/Logs/drift/drift.log`
  on macOS; `DRIFT_LOG=debug` prints more.
- `DRIFT_DEMO=1` opens a made-up workspace instead of your account (with
  `DRIFT_THEME=light|dark`); `DRIFT_SCREENSHOT=file.png` saves the window once it has
  loaded and quits.
- `cargo test` covers the parsers (message HTML, the services' answers, times) and the
  sign-in helpers.

## License

[MIT](LICENSE). drift is not affiliated with or endorsed by Microsoft. Microsoft Teams
is a trademark of Microsoft Corporation.
