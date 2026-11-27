# Vire — Live wallpaper + Dock cho Windows (Rust)

Viết bằng Rust. Tác giả: **NSH**.

> **Giao diện:** kiểu Cyberpunk / Sci-Fi HUD — nền `#050505`/`#0D1117` có lưới và đường quét, neon cyan `#00E5FF` / magenta `#FF00FF` / vàng `#FCEE09`, góc vát 45° (không bo tròn), viền 1px phát sáng, font mono (Consolas), hover đặc màu neon đảo chữ + hiệu ứng glitch, trang trí HUD (ngoặc `[ ]`, tâm ngắm, mã vạch, chuỗi hex). Mã vẽ nằm ở `src/ui/hud.rs`, chỉ dùng hình vẽ của egui — không thêm ảnh hay thư viện.

## Yêu cầu
- **Hệ điều hành:** Windows 11 (đã thử trên bản 24H2 trở lên). Windows 10 có thể chạy được một phần nhưng **chưa được kiểm tra**: Dock đọc danh sách app từ taskbar kiểu Windows 11 và một số hiệu ứng kính mờ cần Windows 11.
- **Không hỗ trợ Linux và macOS.** Vire dùng trực tiếp các thành phần riêng của Windows (cửa sổ desktop Progman/WorkerW, Media Foundation, WASAPI, DWM, UI Automation), nên không thể chạy trên hệ điều hành khác.
- Không cần cài đặt, không cần quyền quản trị. Chỉ dùng DLL có sẵn của Windows.

## Chạy
Mở `dist\Vire\Vire.exe`. Không cần cài đặt; chỉ cần giữ thư mục `samples` nằm cạnh file exe.

- **Thư viện**: bấm thẻ để đổi hình nền; kéo thả video vào cửa sổ hoặc bấm **+ Thêm video** (.mp4 .mov .m4v .wmv .avi .mkv .webm). Chuột phải hoặc bấm biểu tượng thùng rác để xoá video đã thêm.
- **Thư mục riêng `Documents\Vire`**: video bỏ vào `Wallpapers`, nhạc bỏ vào `Music` thì Vire tự nạp mỗi lần khởi động và mỗi lần mở cửa sổ (dùng tại chỗ, không đổi tên hay xoá file của bạn). Video/nhạc thêm bằng nút hoặc kéo thả cũng được chép vào đây với đúng tên gốc. Xoá một mục trong Vire thì file vào Thùng rác. Chỉ danh sách, tên đã đổi và cài đặt nằm ở `%APPDATA%\Vire`.
- **Phát hành**: `build.ps1` tạo `release\Vire-win64.zip` chỉ gồm `Vire.exe` và video mẫu. Video/nhạc của người dùng nằm ở `Documents\Vire\Wallpapers` và `Documents\Vire\Music`, danh sách/cài đặt ở `%APPDATA%\Vire` — tất cả nằm ngoài thư mục app nên không bao giờ lọt vào bản phát hành (script từ chối đóng gói nếu thấy file lạ). Máy mới mở lần đầu thì các thư mục này trống.
- **Nhạc + widget**: tab *Nhạc* chứa danh sách bài (thêm bằng nút, kéo thả hoặc bỏ file vào `Documents\Vire\Music`; tên, nghệ sĩ, độ dài đọc từ thẻ tag). Phát nhạc bằng engine có sẵn của Windows (MP3, FLAC, M4A, WAV, WMA, OGG… tuỳ codec máy có), chạy cả khi đóng cửa sổ, nhớ bài đang nghe, hỗ trợ phím media. Widget nhỏ nổi trên desktop (dưới mọi cửa sổ) hiện ảnh bìa, tên bài, nghệ sĩ, Prev/Play/Next, thanh tiến trình, âm lượng; màu lấy theo hình nền, kéo để di chuyển, chuột phải để đổi cỡ/độ trong suốt/ẩn. Kiến trúc `nowplaying.rs` cho phép thêm nguồn ngoài (Spotify, YouTube Music… qua Windows media controls) sau này. Khi nhạc ở trang Nhạc đang phát thì nhạc nền riêng của hình nền tạm nghỉ.
- **Đổi tên**: chỉ bằng chuột phải vào thẻ → *Đổi tên* (tên được lưu vĩnh viễn, ghi file an toàn, có bản sao dự phòng).
- **Nhạc nền**: chuột phải thẻ → *Thêm nhạc…* hoặc kéo file âm thanh (.mp3 .wav .m4a .aac .flac .ogg .wma) vào cửa sổ để gắn vào hình nền đang chọn. Nhạc được chép vào thư mục dữ liệu của Vire, tự lặp, theo âm lượng/tắt tiếng của Vire; khi có nhạc thì tiếng gốc của video bị tắt. Nhạc dừng khi bạn tạm dừng, dùng pin hoặc chơi game toàn màn hình.
- **Màn hình**: một hình nền cho mọi màn hình, hoặc chọn riêng từng màn hình.
- **Dock & Taskbar**: chọn *Taskbar Windows* hoặc *Vire Dock*.
  - **Vire Dock** là Dock riêng của Vire, đặt được **dưới / trái / phải**. Icon phóng to và đẩy icon bên cạnh như Mac. Có thể tự ẩn (không để lại vệt nào) hoặc giữ chỗ trên màn hình để cửa sổ phóng to không đè lên. Đồng hồ kèm thứ/ngày nằm trong Dock (bấm để mở lịch và thông báo), có nút Wi-Fi/âm lượng/pin. Nhiều màn hình: mỗi màn hình có một Dock riêng (cùng cấu hình, cùng danh sách app), tự thêm/bớt khi cắm hoặc rút màn hình. Thứ tự app cố định như Windows (app mới nối vào cuối). Nút **^** mở đúng khay "biểu tượng ẩn" của Windows. Chuột phải vào icon app mở menu giống Windows (tên app, Ghim/Bỏ ghim, Đóng cửa sổ, Kết thúc tác vụ); chuột phải vào Start/đồng hồ/khay mở menu tương ứng của Windows. Lần bật đầu tiên tự lấy các app đã ghim trên taskbar Windows.
  - **Chất liệu** *Acrylic / Mica / Glass* áp cho cửa sổ Vire, Vire Dock, Explorer và Settings. Kính vẫn giữ nguyên khi cửa sổ không được chọn.
  - **Kính cho hệ thống**: bật hiệu ứng trong suốt của Windows cho Start, thanh thông báo, Settings và File Explorer.
  - Dock lấy danh sách app, thứ tự và app đã ghim **trực tiếp từ taskbar Windows** (qua UI Automation); rê chuột lên icon hiện ảnh xem trước cửa sổ (bấm để chuyển, bấm chuột giữa để đóng). Ghim/bỏ ghim ở taskbar Windows thì Dock tự theo; ghim trên Dock chỉ có tác dụng trên Dock vì Windows không cho app khác sửa taskbar.
  - Lưu ý: icon khay của app khác và jump list (mục gần đây khi chuột phải) hiện chỉ có trên taskbar Windows.
- **Cài đặt**: Lấp đầy / Vừa khung, *tự nghỉ khi không thấy desktop* (cửa sổ phóng to, game, phim toàn màn hình; dừng riêng từng màn hình), dừng khi dùng pin, khởi động cùng Windows.
- **Đóng cửa sổ**: hình nền và Dock vẫn chạy, giao diện được giải phóng khỏi RAM. Mở lại bằng icon dưới khay; chuột phải vào icon để chọn **Thoát** (taskbar Windows và hình nền tĩnh được trả lại).
- `Vire.exe --tray`: chạy ẩn (dùng cho khởi động cùng Windows). `--tab=displays|music|taskbar|theme|settings`: mở thẳng vào tab đó. Thứ tự tab: Hình nền · Màn hình · Nhạc · Dock & Taskbar · Giao diện · Cài đặt (phần giới thiệu nằm cuối trang Cài đặt).

Dữ liệu lưu ở `%APPDATA%\Vire`.

## Nhẹ cỡ nào (đo trên máy build, 1080p, Win11)
- `Vire.exe` 2,2 MB, chỉ dùng DLL có sẵn của Windows.
- Chạy ngầm kèm Vire Dock: khoảng **6 MB** RAM (cột Memory trong Task Manager), CPU gần 0% khi không có thay đổi.

## Kỹ thuật
| Phần | Cách làm |
|---|---|
| Hình nền dưới icon | `Progman` → `0x052C` → `WorkerW` (Win10/11 cũ) hoặc cửa sổ con của `Progman` dưới `SHELLDLL_DefView` (Win11 24H2+) |
| Phát video | Media Foundation `IMFMediaEngine`, giải mã phần cứng, `SetLoop` lặp liền mạch |
| Ảnh xem trước | Lấy thumbnail của Explorer; nếu Windows không có (thường gặp với video 2K/4K), tự giải mã khung hình ở giây thứ 1 bằng Media Foundation Source Reader |
| Nghỉ khi bị che | 2 lần/giây liệt kê cửa sổ; dừng riêng từng màn hình |
| Vire Dock | 2 cửa sổ trên 1 luồng. Tấm kính là cửa sổ **thường** (không phải tool window, vì DWM tắt làm mờ với tool window) dùng accent blur/acrylic. Góc bo dùng góc bo cửa sổ của Windows 11 (`DWMWCP_ROUND`), vì DWM bỏ qua vùng cắt (`SetWindowRgn`) khi làm mờ. Lớp icon là cửa sổ layered do CPU vẽ (viền kính, vệt bóng, icon, phóng to, chú thích, đồng hồ), chỉ vẽ lại khi có thay đổi. Trạng thái được **đối chiếu mỗi 100 ms** theo "cài đặt mong muốn", nên lệnh bật/tắt/đổi cạnh không bao giờ bị rơi. Chiếm chỗ màn hình qua AppBar (`SHAppBarMessage`) |
| Kính cửa sổ Vire | Accent blur thay cho Mica/Acrylic hệ thống, vì kính hệ thống chuyển thành xám đặc khi cửa sổ không được chọn |
| An toàn | Khi khởi động, Vire dọn mọi thay đổi cũ trên taskbar. Nếu gặp lỗi bất ngờ, Vire trả lại taskbar Windows trước khi dừng |

**Giới hạn:** phần nội dung bên trong File Explorer vẫn có nền đặc vì Explorer tự tô nền; chỉ thanh tiêu đề và khung là kính (muốn hơn phải tiêm DLL vào explorer.exe, Vire không làm). Nếu tắt Vire bằng Task Manager khi đang dùng Vire Dock, taskbar Windows sẽ hiện lại ở lần mở Vire tiếp theo.

**Dung lượng:** PNG và hộp thoại chọn file dùng WIC / IFileOpenDialog có sẵn trong Windows thay vì đóng gói thư viện riêng. Font lấy từ Windows. Build `opt-level = "z"`, LTO, `panic = "abort"`, strip.

## Build lại
```powershell
.\build.ps1
```
Cần Rust `stable-x86_64-pc-windows-gnu` (rustup) và `windres` của MSYS2 (`C:\msys64\ucrt64\bin`). `tools\gen_samples.ps1` tạo video mẫu bằng ffmpeg; `assets\logo_1024.png` là logo gốc.

Cờ dành cho nhà phát triển: `--preview-dock=out.png [--edge=left|right] [--material=glass|mica|acrylic]` vẽ thử Dock ra ảnh; `--dev-cycle-edges` tự đổi cạnh Dock; `--dev-ignore-fullscreen`; `--dev-thumb="video|out.png"`.

- **Player trong tab Hình nền**: thẻ nhạc kiểu Spotify bên phải ảnh xem trước (ảnh bìa = chính ảnh hình nền, thanh tua, trộn bài, lùi/tiếp, lặp, danh sách + bộ sưu tập có mục Yêu thích). **Lời bài hát (thử nghiệm)**: bật nút LỜI; đặt file .lrc cùng tên cạnh bài hát (vd. bai.mp3 -> bai.lrc), lời chạy theo nhạc hiện ở giữa phía dưới desktop (dưới mọi cửa sổ, chuột xuyên qua).

- **Nhạc theo hình nền**: nút ♪ trên ảnh xem trước chỉ để chọn bài khớp với hình nền. Mỗi lần đổi hình nền, bài khớp hiện lên và phát trong thẻ nhạc bên phải; muốn nghe bài khác thì chuyển ngay trong thẻ (không đổi bài khớp). Âm lượng và tắt tiếng chỉ chỉnh ở thẻ nhạc (rê chuột vào loa). Có ô tìm hình nền ở trang Hình nền và ô tìm nhạc trong danh sách của thẻ nhạc.

- **Widgets (tab Widgets)**: thư viện widget nổi trên hình nền desktop (dưới mọi cửa sổ), thêm / ẩn / nhân đôi / xoá, nhiều bản cùng loại, nhiều màn hình, kéo để di chuyển, Ctrl + lăn chuột để đổi cỡ, chuột phải để mở menu, nhớ vị trí; **hồ sơ desktop** (mỗi hồ sơ một bộ widget) và **lưu / khôi phục bố cục**. Đăng ký widget mới: thêm một file `src/widgets/<tên>.rs` (impl `Widget` + một `KindInfo`) và một dòng trong `widgets::kinds()` — lõi quản lý không cần sửa. Hai widget đầu: **Đồng hồ** (5 kiểu, phông, cỡ, màu, căn lề, mờ nền, bóng, phát sáng, giây, 12/24 giờ, múi giờ) và **Music Visualizer** (Spectrum Bars, Waveform, Circular, Ambient).
- **Âm thanh cho visualizer** (`src/audio.rs` + `src/dsp.rs`): WASAPI *loopback* trên thiết bị phát mặc định — nghe mọi âm thanh Windows đang phát (Spotify, trình duyệt, VLC, game…), không cần nhập bài hát, không cần quyền đặc biệt hay micro. FFT 2048 điểm → 96 dải log 40 Hz–16 kHz + dạng sóng + mức RMS. Chỉ chạy khi có visualizer đang hiện (dừng trong ~10 ms khi tắt), theo kịp đổi thiết bị phát mặc định, tự mở lại khi dịch vụ âm thanh gián đoạn. Không ghi âm, không lưu, không gửi đi. Nguồn "Trình phát trong app" chỉ phản ứng khi trình phát của Vire đang phát.
## Cấu trúc mã nguồn
```
src/
  main.rs           khởi động: engine, Dock, widget, tray, cửa sổ chính; các cờ dev ẩn (--dev-*)
  model.rs          dữ liệu: thư viện hình nền / nhạc, cài đặt, lưu JSON (%APPDATA%\Vire)
  engine.rs         hình nền động: mỗi màn hình một IMFMediaEngine, tạm dừng thông minh, tái dùng player
  player.rs         trình phát nhạc (hàng đợi, lặp, trộn) trên luồng engine
  desktop.rs        gắn cửa sổ vào desktop, danh sách màn hình, cửa sổ che / toàn màn hình
  vdock.rs + vdock/ Vire Dock: items, draw, click, menus (menu kiểu Windows), tasks, runtime (luồng + poll)
  widget.rs         widget nhạc nhỏ trên desktop + lời bài hát (thử nghiệm)
  widgets/          Widget Center: mod (mô hình, hồ sơ, registry), host (cửa sổ desktop), clock, viz
  audio.rs, dsp.rs  nghe âm thanh hệ thống (WASAPI loopback) -> FFT / dải tần; không phụ thuộc giao diện
  raster.rs         bộ vẽ CPU (hình bo, chữ, kính, đường, phát sáng) dùng chung cho Dock và widget
  ui.rs + ui/       cửa sổ chính (egui): theme, widgets (nút), kit (điều khiển cho widget), pages, wallpapers,
                    widgets_page, cinematic (giao diện Cinematic), hud (giao diện Cyberpunk), music_*, player_panel
  watchdog.rs       ghi hang.log khi một luồng kẹt > 5 s (chẩn đoán treo)
```
Quy tắc: widget chỉ vẽ + đọc cài đặt (không biết về Dock hay cửa sổ chính); âm thanh đi một chiều audio -> dsp -> widget;
mọi thứ dùng chung khoá (`AppState`, `Hub`) được giữ ngắn, không bao giờ trong lúc chờ luồng khác.

## Nơi lưu dữ liệu (portable)
Hình nền, nhạc, cài đặt, thư viện, widget và log nằm trong **`Data`** ngay cạnh `Vire.exe`. Lần đầu chạy bản này, dữ liệu cũ ở `Documents\Vire` và `%APPDATA%\Vire` được **chuyển** sang `Data` (đường dẫn trong JSON được sửa theo). Thư mục không ghi được thì Vire quay về hai chỗ cũ. `build.ps1` không bao giờ đóng `Data` vào gói phát hành.
