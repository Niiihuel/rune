#!/usr/bin/env bash
# RUNE's own call sounds, made from numbers: each is a few notes (a sine
# with its octave, a 4 ms attack and an exponential decay) synthesized by
# ffmpeg's aevalsrc and encoded as Ogg Opus, mono, 24 kbit/s. Nothing is
# sampled or copied from anyone; they are RUNE's, under its license.
#
#   nix-shell -p ffmpeg --run scripts/sounds.sh      # or FFMPEG=/path/to/ffmpeg
#
# The names are the events' (rune_ui::CallSound::key), which a folder of
# one's own replaces file by file (docs/MEDIA.md).
set -euo pipefail

ffmpeg="${FFMPEG:-ffmpeg}"
out="$(cd "$(dirname "$0")/.." && pwd)/assets/sounds"
mkdir -p "$out"

# note FREQUENCY START DECAY: one note from START seconds.
note() {
  local f=$1 at=$2 k=$3
  echo "if(gte(t,$at),(sin(2*PI*$f*(t-$at))+0.25*sin(4*PI*$f*(t-$at)))*exp(-(t-$at)*$k)*min(1,(t-$at)/0.004),0)"
}

# sound NAME SECONDS AMPLITUDE NOTE...: the notes summed, at AMPLITUDE.
sound() {
  local name=$1 seconds=$2 amplitude=$3
  shift 3
  local sum
  sum=$(IFS=+; echo "$*")
  "$ffmpeg" -hide_banner -loglevel error -y \
    -f lavfi -i "aevalsrc=exprs='$amplitude*($sum)':s=48000:d=$seconds" \
    -ac 1 -c:a libopus -b:a 24k \
    -map_metadata -1 -fflags +bitexact -flags:a +bitexact \
    "$out/$name.ogg"
}

# Notes: A4 440, B3 246.94, C5 523.25, C#5 554.37, D4 293.66, D5 587.33,
# D6 1174.66, E4 329.63, E5 659.25, F4 349.23, G4 392, G5 783.99,
# G6 1567.98, A5 880, B5 987.77, C6 1046.5.
sound join 0.32 0.32 "$(note 659.25 0 14)" "$(note 880 0.11 12)"
sound leave 0.32 0.32 "$(note 880 0 14)" "$(note 659.25 0.11 12)"
sound user_join 0.24 0.30 "$(note 523.25 0 18)" "$(note 783.99 0.08 16)"
sound user_leave 0.24 0.30 "$(note 783.99 0 18)" "$(note 523.25 0.08 16)"
sound mute 0.16 0.34 "$(note 587.33 0 30)" "$(note 440 0.05 26)"
sound unmute 0.16 0.34 "$(note 440 0 30)" "$(note 587.33 0.05 26)"
sound deafen 0.24 0.34 "$(note 392 0 20)" "$(note 293.66 0.08 18)"
sound undeafen 0.24 0.34 "$(note 293.66 0 20)" "$(note 392 0.08 18)"
sound stream_start 0.36 0.28 "$(note 523.25 0 16)" "$(note 659.25 0.08 16)" "$(note 783.99 0.16 12)"
sound stream_stop 0.36 0.28 "$(note 783.99 0 16)" "$(note 659.25 0.08 16)" "$(note 523.25 0.16 12)"
sound user_stream 0.22 0.28 "$(note 987.77 0 22)" "$(note 1174.66 0.07 20)"
sound disconnect 0.40 0.26 "$(note 329.63 0 9)" "$(note 349.23 0 9)" "$(note 246.94 0.14 8)"
sound message 0.38 0.26 "$(note 1046.5 0 10)" "$(note 1567.98 0.02 12)"
# Private calls: a call coming in rings with a rising phrase, twice, played
# again every 2.5 s; calling someone, with one soft held pair every 3 s.
sound ring 1.10 0.26 "$(note 659.25 0 7)" "$(note 783.99 0.12 7)" "$(note 1046.5 0.24 6)" "$(note 659.25 0.55 7)" "$(note 783.99 0.67 7)" "$(note 1046.5 0.79 6)"
sound ring_out 0.90 0.22 "$(note 440 0 3)" "$(note 554.37 0 3)"
