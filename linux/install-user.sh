#!/bin/sh
# Install runtime dependencies and register the viewer for the current user.
set -eu
cd -- "$(dirname -- "$0")"
as_admin() {
    if [ "$(id -u)" -eq 0 ]; then "$@"; elif [ -t 0 ]; then sudo "$@"; else pkexec "$@"; fi
}
. /etc/os-release
case "$ID" in
    debian|ubuntu)
        deps=$(cat payload/debian-depends.txt)
        if ! plan=$(LC_ALL=C apt-get --simulate --no-remove satisfy "$deps"); then
            as_admin apt-get update
            plan=$(LC_ALL=C apt-get --simulate --no-remove satisfy "$deps")
        fi
        if printf '%s\n' "$plan" | grep -q '^Inst '; then
            as_admin apt-get -y --no-remove satisfy "$deps"
        fi
        ;;
    fedora)
        set -- webkit2gtk4.1 ImageMagick ImageMagick-heic python3-pillow libavif-tools xdg-utils curl polkit
        if ! rpm -q "$@" >/dev/null; then as_admin dnf -y install "$@"; fi
        ;;
    *)
        echo 'This installer supports Debian 13, Ubuntu 26.04 and Fedora 44 on x86-64.' >&2
        exit 1
        ;;
esac
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
app="$data/photo-viewer-advanced"
mkdir -p "$app" "$data/applications" "$data/icons/hicolor/256x256/apps"
install -m644 LICENSE NOTICE "$app/"
cd payload
binary=$(cat binary-name)
staged=$(mktemp "$app/.viewer.XXXXXX")
trap 'rm -f -- "$staged"' EXIT
install -m755 "$binary" "$staged"
mv -f -- "$staged" "$app/$binary"
printf 'tar\n' > "$app/install-format"
if test -f magick-debian13; then install -m755 magick-debian13 "$app/magick-debian13"; fi
if test -f magick-ubuntu26; then install -m755 magick-ubuntu26 "$app/magick-ubuntu26"; fi
cp -R licenses heif "$app/"
install -m644 icon.png "$data/icons/hicolor/256x256/apps/com.photovieweradvanced.png"
python3 - "$app/$binary" "$data/applications/com.photovieweradvanced.desktop" <<'PY'
from pathlib import Path
import sys
path = sys.argv[1].replace('%', '%%')
for char in ('\\', '"', '`', '$'):
    path = path.replace(char, '\\' + char)
# A desktop entry unescapes string values before parsing the Exec arguments.
path = path.replace('\\', '\\\\')
entry = Path('com.photovieweradvanced.desktop').read_text()
Path(sys.argv[2]).write_text(entry.replace('Exec=photo-viewer-advanced %f', f'Exec="{path}" %f'))
PY
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$data/applications"; fi
echo "Photo Viewer Advanced is available in the applications menu."
