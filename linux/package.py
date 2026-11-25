#!/usr/bin/env python3
"""Package a compiled Linux executable. Run --help for the build inputs."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LINUX = ROOT / 'linux'
NAME = 'photo-viewer-advanced'
ID = 'com.photovieweradvanced'
import json
VERSION = json.loads((ROOT / 'package.json').read_text())['version']
BINARY = f'{NAME}-V{VERSION}'
RELEASE = f'Photo-Viewer-Advanced-V{VERSION}' 
ICON = ROOT / 'src-tauri/icons/128x128@2x.png'


def put(source, target, mode=0o644):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(mode)


def copy_heif(target):
    for directory in ('lib', 'plugins'):
        for source in (LINUX / 'vendor/fedora-heif' / directory).iterdir():
            put(source, target / directory / source.name)


def desktop(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    mimes = ';'.join((LINUX / 'mime-types.txt').read_text().split()) + ';'
    path.write_text(f"""[Desktop Entry]
Type=Application
Name=Photo Viewer Advanced
Comment=View and edit local images
Exec=photo-viewer-advanced %f
Icon={ID}
Terminal=false
Categories=Graphics;Photography;Viewer;
MimeType={mimes}
StartupNotify=true
StartupWMClass=com.photovieweradvanced
""")


def copy_licenses(target):
    for name in ('Prior-Apache-2.0.txt', 'NPM-DEPENDENCIES.txt'):
        put(ROOT / 'src-tauri/resources/licenses' / name, target / name)
    for source in (LINUX / 'notices').iterdir():
        put(source, target / source.name)
    put(LINUX / 'Original-MIT.txt', target / 'Original-MIT.txt')


def tar(output, folder, *items, compression='-czf'):
    subprocess.run(['tar', '--sort=name', '--owner=0', '--group=0', '--numeric-owner',
                    compression, str(output), '-C', str(folder), *items], check=True)


def build(kind, binary, decoder, ubuntu_decoder, output, work):
    if kind == 'tar':
        folder = work / RELEASE
        payload = folder / 'payload'
        put(binary, payload / BINARY, 0o755)
        (payload / 'binary-name').write_text(BINARY + '\n')
        copy_heif(payload / 'heif')
        put(decoder, payload / 'magick-debian13', 0o644)
        put(ubuntu_decoder, payload / 'magick-ubuntu26', 0o644)
        put(ROOT / 'LICENSE', folder / 'LICENSE')
        put(ROOT / 'NOTICE', folder / 'NOTICE')
        put(ROOT / 'README.md', folder / 'README.md')
        put(LINUX / 'debian-depends.txt', payload / 'debian-depends.txt')
        put(ICON, payload / 'icon.png')
        put(LINUX / 'install-user.sh', folder / 'install-user.sh', 0o755)
        desktop(payload / f'{ID}.desktop')
        copy_licenses(payload / 'licenses')
        tar(output / (folder.name + '.tar.gz'), work, folder.name)
        return

    root = work / 'payload'
    program = root / 'usr/lib' / NAME / BINARY
    put(binary, program, 0o755)
    program.with_name('install-format').write_text(kind + '\n')
    if kind == 'deb':
        put(decoder, program.with_name('magick-debian13'), 0o755)
        put(ubuntu_decoder, program.with_name('magick-ubuntu26'), 0o755)
    else:
        copy_heif(program.parent / 'heif')
    bin_dir = root / 'usr/bin'
    bin_dir.mkdir(parents=True)
    (bin_dir / NAME).symlink_to(f'../lib/{NAME}/{BINARY}')
    put(ICON, root / f'usr/share/icons/hicolor/256x256/apps/{ID}.png')
    desktop(root / f'usr/share/applications/{ID}.desktop')
    docs = root / 'usr/share/doc' / NAME
    license_dir = root / 'usr/share/licenses' / NAME
    put(ROOT / 'LICENSE', license_dir / 'LICENSE')
    put(ROOT / 'NOTICE', license_dir / 'NOTICE')
    put(ROOT / 'README.md', docs / 'README.md')
    copy_licenses(license_dir / 'third-party')

    if kind == 'deb':
        control = work / 'control'
        control.mkdir()
        deps = (LINUX / 'debian-depends.txt').read_text().strip()
        (control / 'control').write_text(f"""Package: {NAME}
Version: {VERSION}-1
Architecture: amd64
Maintainer: Photo Viewer Advanced contributors <maintainers@photovieweradvanced.invalid>
Section: graphics
Priority: optional
Installed-Size: {sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) // 1024}
Depends: {deps}
Description: Offline photo viewer with animation playback and image editing
 Browse local images, inspect animations, crop, rotate and save copies.
""")
        shutil.copyfile(ROOT / 'LICENSE', docs / 'copyright')
        (work / 'debian-binary').write_bytes(b'2.0\n')
        tar(work / 'control.tar.gz', control, '.')
        tar(work / 'data.tar.xz', root, '.', compression='-cJf')
        subprocess.run(['ar', 'rcD', str(output / f'{RELEASE}.deb'),
                        'debian-binary', 'control.tar.gz', 'data.tar.xz'], cwd=work, check=True)
        return

    spec = work / 'viewer.spec'
    spec.write_text(f"""Name: {NAME}
Version: {VERSION}
Release: 1
Summary: Offline photo viewer with animation playback and image editing
License: AGPL-3.0-only
Requires: curl, polkit, ImageMagick >= 7, ImageMagick-heic, python3-pillow, libavif-tools, webkit2gtk4.1, xdg-utils
%global __provides_exclude ^libde265[.]so.*$
%global __requires_exclude ^libde265[.]so.*$
%global debug_package %{{nil}}
%description
Browse local images, inspect animations, crop, rotate and save copies.
%install
mkdir -p "%{{buildroot}}"
cp -a "{root}/." "%{{buildroot}}/"
%files
/usr/bin/{NAME}
/usr/lib/{NAME}
/usr/share/applications/{ID}.desktop
/usr/share/icons/hicolor/256x256/apps/{ID}.png
%dir /usr/share/doc/{NAME}
%license /usr/share/licenses/{NAME}
%doc /usr/share/doc/{NAME}/README.md
""")
    subprocess.run(['rpmbuild', '-bb', '--define', f'_topdir {work}/rpm',
                    '--define', f'_tmppath {work}', '--define', '_build_id_links none', '--define', '_buildhost localhost', str(spec)], check=True)
    packages = list((work / 'rpm/RPMS').rglob('*.rpm'))
    assert len(packages) == 1
    shutil.copy2(packages[0], output / packages[0].name)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('format', choices=('deb', 'rpm', 'tar'))
    cli.add_argument('--binary', type=Path, required=True)
    cli.add_argument('--debian-decoder', type=Path)
    cli.add_argument('--ubuntu-decoder', type=Path)
    cli.add_argument('--output', type=Path, required=True)
    opts = cli.parse_args()
    if opts.format != 'rpm' and not all(p and p.is_file() for p in (opts.debian_decoder, opts.ubuntu_decoder)):
        cli.error('DEB and TAR require decoders built on Debian 13 and Ubuntu 26.04. See BUILD.md.')
    opts.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='pva-package-') as tmp:
        build(opts.format, opts.binary.resolve(), opts.debian_decoder, opts.ubuntu_decoder, opts.output.resolve(), Path(tmp))
