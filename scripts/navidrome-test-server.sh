#!/bin/bash
# A throwaway Navidrome for trying dust's Navidrome support and running the live
# tests (`cargo test subsonic::live -- --ignored`). Needs Docker and ffmpeg.
#
#   scripts/navidrome-test-server.sh [dir]   # default dir: ./target/navidrome-test
#
# Generates a small library of sine tones in several formats and sample rates (MP3,
# FLAC at 48 kHz, AAC in M4A, Vorbis, Opus, WAV, MP3 at 22.05 kHz), starts Navidrome
# on http://127.0.0.1:4533 with user `dust` / password `dust-test-pw`, and scans.
# Stop it with: docker rm -f dust-navidrome-test
set -euo pipefail

DIR=$(mkdir -p "${1:-target/navidrome-test}" && cd "${1:-target/navidrome-test}" && pwd)
MUSIC=$DIR/music
IMAGE=deluan/navidrome:0.58.0
USER_NAME=dust
PASSWORD=dust-test-pw

mkdir -p "$MUSIC" "$DIR/data" "$DIR/art"
ffmpeg -loglevel error -y -f lavfi -i "color=c=0xC6F36B:s=300x300" -frames:v 1 "$DIR/art/a.jpg"
ffmpeg -loglevel error -y -f lavfi -i "color=c=0x7B6CF6:s=300x300" -frames:v 1 "$DIR/art/b.png"

# tone <album dir> <file> <title> <track> <Hz> <sample rate> <artist> <album> <year> <codec args...>
tone() {
  local dir="$MUSIC/$1" file=$2 title=$3 track=$4 hz=$5 rate=$6 artist=$7 album=$8 year=$9
  shift 9
  mkdir -p "$dir"
  ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=$hz:sample_rate=$rate:duration=20" -ac 2 \
    -metadata title="$title" -metadata artist="$artist" -metadata album_artist="$artist" \
    -metadata album="$album" -metadata date="$year" -metadata track="$track" -metadata genre="Test" \
    "$@" "$dir/$file"
}

A="Sine Collective/Tones (2024)"
B="Sine Collective/Live at the Lab (2025)"
C="Square Wave Trio/Single Pulse (2023)"
tone "$A" "01 First Light.mp3" "First Light" 1 440 44100 "Sine Collective" "Tones" 2024 -c:a libmp3lame -b:a 192k
tone "$A" "02 Second Wind.flac" "Second Wind" 2 523 48000 "Sine Collective" "Tones" 2024 -c:a flac
tone "$A" "03 Third Eye.m4a" "Third Eye" 3 659 44100 "Sine Collective" "Tones" 2024 -c:a aac -b:a 160k
tone "$B" "01 Lab Intro.ogg" "Lab Intro" 1 330 44100 "Sine Collective" "Live at the Lab" 2025 -c:a vorbis -strict -2 -q:a 4
tone "$B" "02 Lab Outro.opus" "Lab Outro" 2 392 48000 "Sine Collective" "Live at the Lab" 2025 -c:a libopus -b:a 96k
tone "$C" "01 Pulse.wav" "Pulse" 1 220 44100 "Square Wave Trio" "Single Pulse" 2023 -c:a pcm_s16le
tone "$C" "02 Echo.mp3" "Echo" 2 247 22050 "Square Wave Trio" "Single Pulse" 2023 -c:a libmp3lame -b:a 128k
cp "$DIR/art/a.jpg" "$MUSIC/$A/cover.jpg"
cp "$DIR/art/a.jpg" "$MUSIC/$B/cover.jpg"
cp "$DIR/art/b.png" "$MUSIC/$C/cover.png"

docker rm -f dust-navidrome-test >/dev/null 2>&1 || true
docker run -d --name dust-navidrome-test -p 127.0.0.1:4533:4533 \
  -e ND_SCANSCHEDULE=0 -e ND_LOGLEVEL=warn -e ND_ENABLEINSIGHTSCOLLECTOR=false \
  -v "$MUSIC:/music:ro" -v "$DIR/data:/data" "$IMAGE" >/dev/null

for _ in $(seq 1 30); do
  curl -sf http://127.0.0.1:4533/ping >/dev/null && break
  sleep 1
done
# First run: create the admin user (ignored if it exists).
curl -s -X POST http://127.0.0.1:4533/auth/createAdmin -H 'Content-Type: application/json' \
  -d "{\"username\":\"$USER_NAME\",\"password\":\"$PASSWORD\"}" >/dev/null || true
curl -s "http://127.0.0.1:4533/rest/startScan?u=$USER_NAME&p=$PASSWORD&v=1.16.1&c=dust&f=json&fullScan=true" >/dev/null
sleep 3
echo "Navidrome is up on http://127.0.0.1:4533 (user $USER_NAME, password $PASSWORD)"
echo "Live tests: DUST_TEST_SUBSONIC=\"http://127.0.0.1:4533|$USER_NAME|$PASSWORD\" cargo test subsonic::live -- --ignored"
