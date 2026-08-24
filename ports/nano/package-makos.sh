#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
binary=${MAKOS_NANO_BINARY:-$repo_dir/build/ports/nano/makos/src/nano}
terminfo=${MAKOS_NANO_TERMINFO:-$repo_dir/build/ports/ncurses/stage/usr/share/terminfo/m/makos}
license=${MAKOS_NANO_LICENSE:-$repo_dir/build/ports/nano/makos/COPYING}
ncurses_license=${MAKOS_NCURSES_LICENSE:-$repo_dir/build/ports/ncurses/stage/COPYING.ncurses}
image=${1:-$repo_dir/build/makos-data-aarch64.img}

test -x "$binary" || "$port_dir/build-makos.sh" >/dev/null
for path in "$binary" "$terminfo" "$license" "$ncurses_license"; do
    test -f "$path" || { echo "nano package missing: $path" >&2; exit 2; }
done

python3 "$repo_dir/scripts/mkpackage.py" "$image" "$port_dir/package-root" \
    --prefix usr/share/licenses/nano \
    --add "/usr/bin/nano=$binary" \
    --add "/usr/share/terminfo/m/makos=$terminfo" \
    --add "/usr/share/licenses/nano/COPYING=$license" \
    --add "/usr/share/licenses/ncurses/COPYING=$ncurses_license"
python3 "$repo_dir/scripts/verify_package.py" "$image"
echo "MAKOS_NANO_PACKAGE_OK image=$image exec=/usr/bin/nano terminfo=/usr/share/terminfo/m/makos licenses=nano,ncurses"
