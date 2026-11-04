#!/usr/bin/env bash
# The libraries drift builds against on Linux: D-Bus (keyring), WebKitGTK (the
# sign-in window), what the window needs, and ALSA (calls' audio). The runners' package mirror sometimes
# stalls, so each try times out and the next one goes to another mirror.
set -u
apt="sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30"
for mirror in "" http://archive.ubuntu.com/ubuntu/ http://mirrors.edge.kernel.org/ubuntu/; do
  [ -n "$mirror" ] && echo "$mirror" | sudo tee /etc/apt/apt-mirrors.txt
  timeout 120 $apt update && timeout 600 $apt install -y \
    libdbus-1-dev libxkbcommon-dev libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libasound2-dev pkg-config && exit 0
  echo "apt failed on ${mirror:-the default mirror}"; sleep 10
done
exit 1
