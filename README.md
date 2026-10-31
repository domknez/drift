# drift

**A free and open source, ultra-lightweight desktop client for Microsoft Teams chats
and channels, written in Rust.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)

drift is for the way most people actually use Teams: messages with colleagues, a few
channels to keep an eye on, and the odd call. It shows just that, in one calm sidebar,
in a single native process — no browser engine, no helper processes, no idle CPU, and
a fraction of the memory the official app keeps busy doing nothing.

> **Early days.** Messages, live updates and notifications all work across your chats,
> channels and teams. Calls still open in Teams itself.

## Features

- **Sign in with Microsoft:** your work or school account, in a small window with
  Microsoft's own sign-in page (MFA included). The sign-in is kept in the system
  keychain (macOS Keychain, Windows Credential Manager, Secret Service) and renewed
  on its own
- **Tabs, not clutter:** Activity, Chats, Teams and Calendar in the top bar, with counts
  of what's unread; the meeting on now (or next today) sits beside them with *Join*
- **Calendar:** your meetings for the next two weeks by day, the one on now marked;
  *Join* opens the meeting straight in Teams on the web, a click on a meeting opens its
  chat
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
- **Search:** typing in the search field narrows the list to names, and after a moment
  finds messages too (in all your chats and channels, as Teams' search does), with
  what you typed marked; a click opens that message. ⌘K jumps to it
- **In a conversation:** find in its messages (⌘F): every match marked, the list of
  them beside the conversation, ↑ ↓ (or Enter, Shift+Enter) to go from one to the
  next; see who's in it (or who wrote in a channel) and open your chat with them; pick
  an emoji from Teams' own set; attach pictures (or drop them on the window)
- **Pictures** in messages, and a tenant's own emoji in reactions
- **Activity:** who mentioned you or replied, what they wrote and where; a click
  opens that very message, marked (in its thread if it's a reply)
- **Notifications** for new messages in your chats and meeting chats, and for mentions
  and replies (Activity); a click opens the conversation. Muted chats and the one on
  screen stay quiet; the Dock icon shows how many chats are unread. Off in the menu
  under your picture
- **Your status:** available, busy, do not disturb, be right back, appear away or
  appear offline, from the menu under your picture; a dot on the avatar of everyone
  you chat with shows theirs (out of office too)
- **Live:** new messages arrive as they're sent, pushed by Teams
- **Quick switcher (⌘T)** to jump to any chat, channel or tab; ⌘1–⌘4 for the tabs
- **Pin and unpin** chats and channels from their right-click menu
- **Read here, read in Teams:** opening a conversation marks it read for Teams too
- **Messages as written:** bold, italics, strikethrough, code, tables, quotes, links,
  mentions and emoji; reactions under them; cards from bots and connectors (pull
  requests, alerts) as one compact row that opens its link
- **Updates itself** from the GitHub releases, installing on restart
- **Modern UI:** panels floating on a canvas, a deep ink dark and a warm paper light
  theme (or follow the system) with one violet accent, Geist and Geist Mono
- Runs on macOS, Linux and Windows

## Install

Download drift from the [latest release](https://github.com/domknez/drift/releases/latest):

- **macOS** (Apple silicon and Intel): unzip `drift-<version>-macos-universal.zip` and
  drag `drift.app` to Applications. The app isn't notarized yet, so macOS stops it the
  first time: open it with right-click → Open, or run
  `xattr -dr com.apple.quarantine /Applications/drift.app` once.
- **Linux** (x86_64): unpack `drift-<version>-linux-x86_64.tar.gz` and run `drift`. It
  needs GTK 3, WebKitGTK 4.1 and D-Bus.
- **Windows** (x86_64): unzip `drift-<version>-windows-x86_64.zip` and run `drift.exe`.

drift keeps itself up to date: it checks for a new release now and then, downloads it
in the background (checked against the release's SHA-256 sums) and offers *Restart to
update* in the top bar; quitting installs it too. Turn it off under your picture
(*Update automatically*), or update from a terminal with `drift update`.

## Releases

Every commit to `main` that passes CI is tagged with the next patch version
(`v0.1.0` → `v0.1.1`) and released for all three platforms, by
`.github/workflows/tag.yml` and `release.yml`. A commit whose message contains
`[skip release]` isn't released.

For a bigger step, tag the commit by hand and push the tag; the next automatic
versions count on from it:

```sh
git tag -a v0.2.0 -m "drift 0.2.0" <commit>
git push origin v0.2.0
```

To release a new commit under a version of your choosing, push it together with its
tag (`git push --atomic origin main v0.2.0`) so it isn't also given the next patch
version.

## Build

```sh
cargo build --release
./target/release/drift
```

On macOS, `scripts/bundle-macos.sh` makes `dist/drift.app` (with its icon and an
identity of its own, which notifications need); drag it to Applications.

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
├── update.rs      new versions from GitHub releases: check, download, install
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
