#!/bin/sh
# Build the private Debian or Ubuntu codec executable using the distro's shared libraries.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output=$(realpath -m "${1:?Pass the output filename}")
build=$(mktemp -d)
trap 'rm -rf -- "$build"' EXIT
tar xzf "$src/vendor/ImageMagick-7.1.2-35.tar.gz" -C "$build" --strip-components=1
cd "$build"
./configure --disable-shared --enable-static --without-modules --without-perl \
    --without-x --without-magick-plus-plus --without-gslib --without-djvu \
    --without-fftw --without-gvc --without-wmf --without-pango --without-raqm \
    --without-lqr --without-fpx --with-openjp2 --with-rsvg --with-jxl \
    --with-raw --with-heic --with-openexr --enable-hdri --with-quantum-depth=16
make -j"${JOBS:-4}" utilities/magick
install -Dm755 utilities/magick "$output"
strip "$output"
