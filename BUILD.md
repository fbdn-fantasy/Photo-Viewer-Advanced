# Building Photo Viewer Advanced

Use the locked dependencies with Node.js 22.12 or later, npm, and a stable Rust toolchain (1.90 or later). Run the commands from the repository root.

## Windows x64

Install the MSVC C++ build tools and Windows SDK. In a developer shell, run:

```powershell
npm ci
npm test
npm run tauri -- build --bundles nsis
```

The NSIS installer is under `src-tauri/target/release/bundle/nsis/`. Tauri downloads the installer tools and the matching Microsoft WebView2 offline installer on the first build. The completed setup includes the runtime installer, decoder, project license and notices.

## Linux x86-64

Install a C compiler, pkg-config and the development packages for GTK 3, WebKitGTK 4.1 and D-Bus. Run:

```sh
npm ci
npm test
npm run build
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --bin photo-viewer-advanced
cargo test --release --locked --manifest-path src-tauri/Cargo.toml --lib
```

Build one private decoder on Debian 13 and one on Ubuntu 26.04, with the development packages for JPEG, PNG, TIFF, WebP, JPEG XL, LibRaw, librsvg, HEIF, OpenJPEG, OpenEXR, Little CMS and libxml2 installed. See `linux/build-debian-decoder.sh` and the pinned source in `linux/vendor/`. The Fedora package uses the system ImageMagick 7. The DEB and TAR include both private decoder builds. The viewer selects the build linked against the installed distribution’s system libraries.

```sh
# On Debian 13:
sh linux/build-debian-decoder.sh ./magick-debian13
# On Ubuntu 26.04:
sh linux/build-debian-decoder.sh ./magick-ubuntu26
# Place both outputs beside package.json on the packaging machine.
binary=src-tauri/target/release/photo-viewer-advanced
python3 linux/package.py deb --binary "$binary" --debian-decoder ./magick-debian13 --ubuntu-decoder ./magick-ubuntu26 --output out
python3 linux/package.py rpm --binary "$binary" --output out
python3 linux/package.py tar --binary "$binary" --debian-decoder ./magick-debian13 --ubuntu-decoder ./magick-ubuntu26 --output out
```

Packaging uses Python 3, GNU tar, binutils and rpmbuild. For a TAR installation, extract the archive and run `sh install-user.sh` inside its top-level directory. It installs the application and desktop entry for the current user.

## Publishing updates

For a version such as 1.0.1, publish a stable GitHub release tagged `V1.0.1` and mark it as the latest release. Attach the installers directly as release assets, using these names:

- `Photo-Viewer-Advanced-V1.0.1-Setup.exe`
- `Photo-Viewer-Advanced-V1.0.1.deb`
- `photo-viewer-advanced-1.0.1-1.x86_64.rpm`
- `Photo-Viewer-Advanced-V1.0.1.tar.gz`

The viewer reads GitHub’s release asset digests. Once a user clicks **Check for Updates**, Windows runs the verified setup and reopens the viewer. Linux runs the installer for the currently installed package type and prompts for authorization when the system package manager requires it.

