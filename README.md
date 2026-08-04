# TheTerminal

Warp-benzeri, blok tabanlı terminal. Tauri (Rust) + SolidJS. Detay: [PRD.md](PRD.md).

## Mimari (özet)

- **Backend (`src-tauri/`)**: `portable-pty` + `alacritty_terminal` VT grid. OSC 133
  semantic mark'lar (`scanner.rs`) blok sınırlarını kesin belirler; zsh'e ZDOTDIR
  override ile hook enjekte edilir (`zdot.rs`) — kullanıcının `.zshrc`'sine dokunulmaz.
  Çıktı ~60fps coalesce edilip event olarak akar (`session.rs`).
- **Frontend (`src/`)**: Blok listesi (DOM render), CodeMirror 6 komut editörü
  (prompt modunda), ham tuş geçişi (running/alt-screen modunda), Fig spec
  autocomplete (`public/figspecs/`, `@withfig/autocomplete` snapshot).

## Geliştirme

```sh
npm install        # figspecs otomatik kopyalanır (postinstall)
npm run tauri dev  # uygulamayı başlat
```

## Test

```sh
cd src-tauri && cargo test   # scanner + histfile + gerçek-zsh e2e blok testi
```

## Paket

```sh
npm run tauri build   # macOS .app / .dmg
```

## Ayarlar

`~/Library/Application Support/theterminal/config.toml` — font, scrollback,
autocomplete/hook aç-kapa. GUI yok (v1).

## Kısayollar

| Kısayol | İşlev |
|---------|-------|
| ⌘T | Yeni oturum |
| ⌘W | Oturumu kapat |
| ⌘K | Blokları temizle |
| ⌘1-9 | Oturuma geç |
| Enter / Shift+Enter | Çalıştır / yeni satır |
| Tab | Otomatik tamamlama |
| ↑↓ | Komut geçmişi |
