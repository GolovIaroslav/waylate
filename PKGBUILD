# Maintainer: jar
pkgname=waylate
pkgver=0.1.5
pkgrel=1
pkgdesc="Wayland popup translator for Arch Linux and KDE Plasma (DeepL/Google/Yandex API or local models)"
arch=("x86_64")
url="https://github.com/jar/waylate"
license=("MIT")
depends=(
  "gtk3"
  "webkit2gtk-4.1"
  "libayatana-appindicator"
  "wl-clipboard"
  "python"
)
makedepends=(
  "cargo"
  "npm"
  "nodejs"
)
optdepends=(
  "python-ctranslate2: NLLB/CTranslate2 local model helper"
  "python-transformers: tokenizer support for the CTranslate2 helper"
  "python-sentencepiece: tokenizer support for NLLB models"
  "huggingface-cli: built-in catalog downloads"
  "llama.cpp: GGUF local OpenAI-compatible server"
)
source=()
sha256sums=()

build() {
  cd "$srcdir/.."
  npm ci
  npm run tauri build -- --no-bundle
}

package() {
  cd "$srcdir/.."

  install -Dm755 "src-tauri/target/release/waylate" "$pkgdir/usr/bin/waylate"
  install -Dm755 "scripts/waylate-ct2-translate" "$pkgdir/usr/bin/waylate-ct2-translate"

  install -Dm644 "packaging/dev.jar.waylate.desktop" \
    "$pkgdir/usr/share/applications/dev.jar.waylate.desktop"
  install -Dm644 "src-tauri/icons/128x128.png" \
    "$pkgdir/usr/share/icons/hicolor/128x128/apps/dev.jar.waylate.png"

  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
  install -Dm644 "CHANGELOG.md" "$pkgdir/usr/share/doc/$pkgname/CHANGELOG.md"
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
