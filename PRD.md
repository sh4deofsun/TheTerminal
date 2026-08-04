# TheTerminal — Ürün Gereksinim Dokümanı (PRD)

**Sürüm:** 0.2
**Tarih:** 2026-07-13
**Sahip:** Batuhan Yalçın

> v0.2 notu: Grilling oturumu sonrası 15 mimari karar netleşti; ilgili bölümler
> bu kararlarla güncellendi. Kararların tam listesi §11.

---

## 1. Özet

Warp'tan ilham alan modern bir terminal. Klasik terminallerin "yalnızca klavye kabul eden
akan metin" modelini kırar: her komut ve çıktısı ayrı bir **blok**; komut girişi tam bir
metin editörü gibi imleç/fare ile serbestçe düzenlenir. Sol panelde **oturum listesi**.
Temel komut + parametreler için **Fig statik spec** tabanlı otomatik tamamlama. Tüm bu
model, shell'e enjekte edilen **OSC 133 semantic prompt mark'ları** üstüne kurulur — komut
sınırları tahminle değil, kesin işaretlerle bulunur.

## 2. Hedefler ve Hedef-Olmayanlar

### Hedefler (v1)
- Kullanıcının shell'ini (zsh) eksiksiz çalıştırmak; tüm komutlar sorunsuz.
- Komut + çıktıyı blok bazlı göstermek; blok kopyala/yeniden çalıştır.
- Komut girişini metin editörü gibi düzenlemek (imleç, seçim, fare, çok satır, vurgu).
- Sol panelde çoklu oturum; aç/kapat/geç.
- Fig statik spec ile komut + parametre otomatik tamamlama.
- macOS.

### Hedef-Olmayanlar (v1)
AI komut önerisi / doğal dil çevirisi, bulut senkron, SSH oturum yönetimi UI'ı, tema
pazarı/eklenti sistemi, **split-pane**, ayarlar GUI'si, blok kalıcılık DB'si, keyfi Fig
shell-out generator'ları, bash/fish/PowerShell.

## 3. Teknoloji Mimarisi

**Yığın:** Tauri (Rust backend + web frontend). İlk hedef **macOS**; kod cross-platform
yazılır, Windows(ConPTY)/Linux v1 sonrası.

| Katman | Seçim | Neden |
|--------|-------|-------|
| Backend / PTY | Rust + `portable-pty` | Gerçek pseudo-terminal, cross-platform |
| VT emülasyonu | Rust `alacritty_terminal` | Tek buffer; grid + scrollback + alt-screen + OSC 133 aynı yerde |
| Köprü | Tauri IPC (command + event) | Frontend ↔ backend akış |
| Frontend | SolidJS | İnce reaktivite, sanal DOM yok → uzun blok listesi + akan çıktı ucuz |
| Söz dizimi editörü | CodeMirror 6 | Olgun imleç/seçim/çok-satır, vurgulama, autocomplete API |
| Terminal render | **DOM-only** (satır=div, stil-run=span) | Seçim/kopyala/vurgu bedava |
| Otomatik tamamlama | Fig autocomplete specs (statik, vendored) | Offline, geniş komut+parametre kapsamı |

### 3.1 Veri Akışı
1. Backend her oturum için: PTY (`portable-pty`) + `alacritty_terminal` grid + shell child.
   PTY okuma bloklayıcı → **oturum başına ayrı thread** okur, byte'ları kanala verir → VT
   parse → dirty-line hesapla.
2. Backend değişiklikleri **~60fps coalesce** ederek frontend'e push eder (throttling
   backend'de; `yes` gibi sel çıktısı IPC'yi boğmaz).
3. Frontend olayları DOM'a uygular; giriş satırını CodeMirror yönetir.

**IPC olayları (backend→frontend):**
`line_update{row, styled_runs[]}`, `block_start{id}`, `block_end{id, exit_code, duration}`,
`mode_change{normal|alt_screen}`, `cursor{row,col}`.

**IPC komutları (frontend→backend):**
`write{session_id, bytes}`, `resize{cols,rows}`, `signal{...}`, `complete_path{cwd,prefix}`,
`list_path_bins`, `history`, `new_session`, `close_session`.

## 4. Shell Entegrasyonu (blok modelinin temeli)

- **OSC 133 semantic prompt marks** (`A` prompt-başı, `B` komut-başı, `C` çıktı-başı,
  `D` komut-bitti + exit code) tüm blok sınırlarını **kesin** belirler; heuristik yok.
  Cwd takibi OSC 7.
- **Enjeksiyon — ZDOTDIR override:** app zsh'i başlatırken `ZDOTDIR`'i geçici bir dizine
  işaret ettirir. O dizindeki `.zshrc`: (1) gerçek `ZDOTDIR`'i geri yükler, (2) kullanıcının
  orijinal zsh başlangıç dosyalarını doğru sırada source eder, (3) OSC 133 entegrasyon
  parçasını ekler. **Kullanıcının `~/.zshrc`'si hiç değişmez.**
- v1 **sadece zsh**. Login + interactive başlatılır (`zsh -l -i`).
- Varsayılan **açık**; kapatılabilir (§9 ayar).

## 5. Fonksiyonel Gereksinimler

### 5.1 Oturum Paneli (Sol Panel)
- Açık oturum listesi (isim + durum ikonu: çalışan/boşta/hatalı).
- Yeni oturum (`+`), kapat, geçiş (tık + kısayol), yeniden adlandır.
- Her oturum bağımsız PTY + grid + geçmiş.
- **Yeni oturum hep `$HOME`'da açılır.**
- **Split yok** — tek aktif oturum terminal alanını doldurur.

### 5.2 Blok Modeli
- Her çalıştırma bir blok: komut satırı + çıktı + çıkış kodu + süre (OSC 133'ten).
- Aksiyonlar: kopyala (komut/çıktı/ikisi), yeniden çalıştır, temizle, sabitleme.
- Başarısız blok (exit ≠ 0) görsel işaret.
- Blok/geçmiş içinde arama, uzun çıktı katlama.
- **Blok hasadı:** `D` mark'ında bloğun çıktı satırları canlı grid'den değişmez blok
  kaydına (styled satırlar) taşınır, DOM'da kalıcı gösterilir.

### 5.3 Komut Girişi (İki Mod)
Giriş modu tamamen OSC 133 durumundan sürülür:
- **Prompt modu** (shell boşta; önceki `D` ile sonraki `B` arası): **CodeMirror** zengin
  düzenleme — imleç, fare seçimi, çok satır, kopyala/yapıştır, geri al/yinele, söz dizimi
  vurgusu. Hiç byte PTY'ye gitmez. Enter'da komut toplu gönderilir (Shift+Enter yeni satır).
- **Raw modu** (komut çalışıyor `C`→`D`, alt-screen, veya stdin okunuyor): CodeMirror kapalı,
  tuşlar **ham geçişle** direkt PTY'ye (vim/REPL/`sudo` parola/`read` sorunsuz).
- **Echo uzlaşması:** Enter'da B→C echo bölgesi grid'de **render edilmez**; blok başlığı
  CodeMirror'ın **vurgulu snapshot'ı** olur (çift gösterim yok, vurgu korunur).
- Sinyaller (Ctrl+C/Ctrl+D) her iki modda PTY'ye iletilir.

### 5.4 Full-screen / TUI (alt-screen)
- DECSET 1049 tespiti → `mode_change` → raw grid render + raw giriş.
- Alt-screen'den çıkışta blok moduna otomatik dönüş.

### 5.5 Otomatik Tamamlama
- **Frontend JS motoru**, CodeMirror `autocompletion` üstünde.
- **Fig autocomplete specs** snapshot'ı vendored (MIT), komut başına lazy yüklenir.
- Motor giriş satırını token'lar (komut→alt-komut→arg), Fig ağacında yürür.
- Dinamik veri Rust'tan: dosya/dizin (`complete_path`), PATH binary'leri, HISTFILE geçmişi.
- Generator: v1 **sadece dosya/dizin**; keyfi shell-out generator → v2.
- Klavye: ok tuşları gez, Tab kabul, Esc kapat; parametre açıklaması gösterilir.

### 5.6 Genel Terminal Davranışı
Tam ANSI renk/stil, kopyala/yapıştır, cwd takibi, sinyal iletimi, 10k satır scrollback.

### 5.7 Dayanıklılık — Mark Yoksa Fallback
OSC 133 akışı kesilirse (ssh, `sudo -s`, tmux, `exec` başka shell, hook'suz ortam) oturum
**ham tek-blok** moduna düşer: klasik sürekli terminal, ham giriş, autocomplete kapalı.
Mark'lar dönünce blok modu **otomatik** geri gelir. Böylece her komut daima çalışır.

## 6. Fonksiyonel-Olmayan Gereksinimler
- **Perf:** yüksek hacim çıktı 60fps coalesce ile donmadan akar (throttling backend'de).
- **Bellek:** oturum başına 10k satır bounded scrollback.
- **Başlatma:** soğuk açılış < 1 sn hedefi.
- **Kararlılık:** bir oturum çökerse diğerleri etkilenmez.
- **Yeniden boyutlama:** canlı grid reflow olur; **geçmiş bloklar yakalandığı genişlikte
  donar** (tarihsel reflow v2).

## 7. Kalıcılık
- **v1'de kalıcılık yok.** Oturumlar + bloklar sadece bellekte; app kapanınca biter.
- Komut geçmişi (autocomplete + ok-tuşu) = zsh'in kendi **`$HISTFILE`**'ı (Rust okur).
  Ayrı geçmiş deposu yok.
- Blok geçmişi DB → v2.

## 8. Kullanıcı Akışları
1. Açılış → varsayılan zsh oturumu (`$HOME`), OSC 133 hook enjekte, prompt hazır.
2. Komut yaz → autocomplete önerir → Enter → blok oluşur, çıktı akar, `D`'de süre+exit.
3. Yeni oturum → `+` → bağımsız zsh (`$HOME`).
4. Komut düzelt → prompt modunda imleçle gez, metni değiştir → Enter/yeniden çalıştır.
5. TUI → `vim x` → alt-screen raw → çıkışta blok moduna dönüş.
6. `ssh remote` → mark yok → ham tek-blok fallback → tam çalışır → çıkışta blok modu geri.

## 9. Ayarlar
Tek **TOML** dosyası (app config dizini): font ailesi/boyutu, tema adı, scrollback limiti,
autocomplete aç/kapa, hook aç/kapa. Varsayılanlar: sistem monospace ~13pt, tek gömülü
**koyu tema**, 10k scrollback, autocomplete açık, hook açık. **Ayarlar GUI'si yok** v1'de
(dosya elle düzenlenir); tema seçici/panel → v2.

## 10. Sürümleme
- **v1 (MVP, macOS):** OSC 133 blok modeli, zsh entegrasyonu, iki-mod giriş, DOM render,
  Fig autocomplete, alt-screen + ham fallback, çoklu oturum, TOML ayar.
- **v2:** bash/fish/PowerShell, Windows/Linux, split-pane, tema seçici + ayar GUI, blok
  kalıcılık, tarihsel reflow, keyfi Fig generator, shell-native completion, komut paylaşımı.
- **v3:** AI öneriler, bulut senkron.

## 11. Karar Kaydı (Grilling v0.2)
1. Komut sınırı = OSC 133 marks, varsayılan-açık (heuristik değil).
2. v1 = zsh only; diğer shell → heuristik/fallback.
3. Enjeksiyon = ZDOTDIR override (rc'ye dokunmaz).
4. VT emülasyonu = Rust `alacritty_terminal` (tek buffer).
5. Renderer = DOM-only.
6. Akış = 60fps coalesced dirty-line, JSON IPC; throttling backend'de.
7. Giriş = iki mod (prompt=CodeMirror / running=raw), OSC 133 sürücülü.
8. Enter = B→C echo bastır + CodeMirror snapshot başlık.
9. Autocomplete = frontend JS + vendored Fig specs; generator sadece dosya/dizin.
10. Yeni oturum hep `$HOME`; login+interactive zsh; thread-per-PTY.
11. Kalıcılık yok; komut geçmişi = HISTFILE; 10k scrollback.
12. Split yok — tek görünüm + panel geçişi.
13. Mark yoksa ham tek-blok fallback, otomatik geri dönüş.
14. Tek TOML ayar, GUI yok, tek koyu tema.
15. Canlı grid reflow / geçmiş blok donar (reflow v2).

## 12. Bilinen Riskler / Netleşecekler
- En yüksek risk: blok modeli ↔ alt-screen geçişi + echo bastırma kesinliği (OSC 133'e bağlı).
- Fig specs vendor biçimi (repo'ya snapshot vs runtime yükleme).
- Hook kapalıyken kullanıcının blok modu kaybını nasıl fark ettiği (UI ipucu gerekebilir).
