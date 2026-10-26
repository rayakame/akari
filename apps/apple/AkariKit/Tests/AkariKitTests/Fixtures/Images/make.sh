#!/bin/sh
# Rebuilds AkariKit's animated WebP (Sources/AkariKit/Resources/animated.webp): the launch
# probe's input and the tests' fixture. Needs img2webp and webpmux (`brew install webp`) and perl.
set -eu
cd "$(dirname "$0")"
out="$(cd ../../../../Sources/AkariKit/Resources && pwd)/animated.webp"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# frame NAME LEFT RIGHT: a 32 × 32 PAM, its left half one RGBA color and its right half another.
frame() {
    {
        printf 'P7\nWIDTH 32\nHEIGHT 32\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n'
        perl -e 'my @l = split /,/, $ARGV[0]; my @r = split /,/, $ARGV[1];
            for (1 .. 32) { print pack("C4", @l) x 16, pack("C4", @r) x 16 }' "$2" "$3"
    } >"$work/$1.pam"
}

frame red 255,0,0,255 255,0,0,255
frame green 0,255,0,255 0,255,0,255
frame clear-blue 0,0,0,0 0,0,255,255
frame half-yellow 255,255,0,128 255,255,0,128
frame magenta-yellow 255,0,255,255 255,255,0,128

# Five frames with their own delays and three loops; lossless, so every pixel can be checked.
# The last changes only its left half, so it is stored as a part over the one before.
img2webp -loop 3 -lossless \
    -d 100 "$work/red.pam" -d 200 "$work/green.pam" \
    -d 50 "$work/clear-blue.pam" -d 300 "$work/half-yellow.pam" \
    -d 150 "$work/magenta-yellow.pam" \
    -o "$work/opaque-background.webp"
# A transparent canvas background, so disposing to it must clear, whether or not a decoder
# honors the color (img2webp writes opaque white).
webpmux -set bgcolor 0,0,0,0 "$work/opaque-background.webp" -o "$out"
