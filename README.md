# Vire — Live wallpaper + Dock cho Windows (Rust)

Viết bằng Rust. Tác giả: **NSH**.

> **Giao diện:** có hai kiểu, đổi ở tab *Giao diện*.
> - **Cyberpunk / Sci-Fi HUD** — nền `#050505`/`#0D1117` có lưới và đường quét, neon cyan `#00E5FF` / magenta `#FF00FF` / vàng `#FCEE09`, góc vát 45° (không bo tròn), viền 1px phát sáng, font mono (Consolas), hover đặc màu neon đảo chữ + hiệu ứng glitch, trang trí HUD. Mã vẽ ở `src/ui/hud.rs`.
> - **Cinematic Dark** — kiểu Apple: nền tối mềm, kính mờ, màu nhấn lấy từ hình nền, thẻ nhạc kiểu Apple Music, thanh âm lượng dọc kiểu Control Center. Mã vẽ ở `src/ui/cinematic.rs`.
>
> Cả hai chỉ dùng hình vẽ của egui — không thêm ảnh hay thư viện.

## Yêu cầu
- **Hệ điều hành:** Windows 11 (đã thử trên bản 24H2 trở lên). Windows 10 có thể chạy được một phần nhưng **chưa được kiểm tra**: Dock đọc danh sách app từ taskbar kiểu Windows 11 và một số hiệu ứng kính mờ cần Windows 11.
- **Không hỗ trợ Linux và macOS.** Vire dùng trực tiếp các thành phần riêng của Windows (cửa sổ desktop Progman/WorkerW, Media Foundation, WASAPI, DWM, UI Automation), nên không thể chạy trên hệ điều hành khác.
- Không cần cài đặt, không cần quyền quản trị. Chỉ dùng DLL có sẵn của Windows.

## Chạy
Mở `dist\Vire\Vire.exe`. Không cần cài đặt; chỉ cần giữ thư mục `samples` nằm cạnh file exe.

- **Thư viện**: bấm thẻ để đổi hình nền; kéo thả video vào cửa sổ hoặc bấm **+ Thêm video** (.mp4 .mov .m4v .wmv .avi .mkv .webm). Chuột phải hoặc bấm biểu tượng thùng rác để xoá video đã thêm.
- **Thư mục `Data` (cạnh `Vire.exe`)**: video bỏ vào `Data\Wallpapers`, nhạc bỏ vào `Data\Music` thì Vire tự nạp mỗi lần khởi động và mỗi lần mở cửa sổ. Video/nhạc thêm bằng nút hoặc kéo thả cũng được chép vào đây. Xoá một mục trong Vire thì file vào Thùng rác. Danh sách, tên đã đổi và cài đặt cũng nằm trong `Data` (xem mục *Nơi lưu dữ liệu* bên dưới).
- **Hình nền động bật/tắt**: công tắc đầu tiên ở tab *Cài đặt*. Tắt thì Vire giải phóng toàn bộ video và trả lại hình nền gốc của Windows (chỉ còn Dock và widget chạy, rất nhẹ — cho ai chỉ cần Dock). Chọn hoặc thêm một hình nền thì hình nền động tự bật lại.
- **Nén video cho vừa màn hình**: thêm video có độ phân giải lớn hơn màn hình (ví dụ 4K trên màn 1080p) thì Vire **hỏi** có nén xuống đúng độ phân giải màn hình không. Nén bằng bộ chuyển mã có sẵn của Windows, ngầm; video 4K giải mã tốn khoảng 150 MB RAM mỗi màn hình, bản 1080p chỉ còn một phần nhỏ. Bản gốc bạn đã thêm trong thư viện được thay bằng bản đã nén (file gốc ngoài thư viện không đụng tới).
- **Phát hành**: `build.ps1` chạy kiểm thử rồi tạo `release\Vire-<phiên bản>-win64.zip` chỉ gồm `Vire.exe`, `samples`, `README.txt` và `Vire-debug.bat`. Thư mục `Data` không bao giờ được đóng vào gói (script từ chối đóng gói nếu thấy file lạ, và quét `Vire.exe` để chắc không lọt tên người dùng hay đường dẫn máy build). Máy mới mở lần đầu thì `Data` trống.
- **Nhạc + widget**: tab *Nhạc* chứa danh sách bài (thêm bằng nút, kéo thả hoặc bỏ file vào `Documents\Vire\Music`; tên, nghệ sĩ, độ dài đọc từ thẻ tag). Phát nhạc bằng engine có sẵn của Windows (MP3, FLAC, M4A, WAV, WMA, OGG… tuỳ codec máy có), chạy cả khi đóng cửa sổ, nhớ bài đang nghe, hỗ trợ phím media. Widget nhỏ nổi trên desktop (dưới mọi cửa sổ) hiện ảnh bìa, tên bài, nghệ sĩ, Prev/Play/Next, thanh tiến trình, âm lượng; màu lấy theo hình nền, kéo để di chuyển, chuột phải để đổi cỡ/độ trong suốt/ẩn. Kiến trúc `nowplaying.rs` cho phép thêm nguồn ngoài (Spotify, YouTube Music… qua Windows media controls) sau này. Khi nhạc ở trang Nhạc đang phát thì nhạc nền riêng của hình nền tạm nghỉ.
- **Đổi tên**: chỉ bằng chuột phải vào thẻ → *Đổi tên* (tên được lưu vĩnh viễn, ghi file an toàn, có bản sao dự phòng).
- **Nhạc nền**: chuột phải thẻ → *Thêm nhạc…* hoặc kéo file âm thanh (.mp3 .wav .m4a .aac .flac .ogg .wma) vào cửa sổ để gắn vào hình nền đang chọn. Nhạc được chép vào thư mục dữ liệu của Vire, tự lặp, theo âm lượng/tắt tiếng của Vire; khi có nhạc thì tiếng gốc của video bị tắt. Nhạc dừng khi bạn tạm dừng, dùng pin hoặc chơi game toàn màn hình.
- **Màn hình**: một hình nền cho mọi màn hình, hoặc chọn riêng từng màn hình.
- **Dock & Taskbar**: chọn *Taskbar Windows* hoặc *Vire Dock*.
  - **Vire Dock** là Dock riêng của Vire, đặt được **dưới / trái / phải**. Icon phóng to và đẩy icon bên cạnh như Mac. Có thể tự ẩn (không để lại vệt nào) hoặc giữ chỗ trên màn hình để cửa sổ phóng to không đè lên. Đồng hồ kèm thứ/ngày nằm trong Dock (bấm để mở lịch và thông báo), có nút Wi-Fi/âm lượng/pin. Nhiều màn hình: mỗi màn hình có một Dock riêng (cùng cấu hình, cùng danh sách app), tự thêm/bớt khi cắm hoặc rút màn hình. Thứ tự app theo đúng taskbar Windows. Có hai kiểu: kính (mặc định) và **Phản chiếu** (không có nền, icon soi bóng mờ dần bên dưới). Nút **^** mở đúng khay "biểu tượng ẩn" của Windows. Chuột phải vào icon app mở menu giống Windows (tên app, Ghim/Bỏ ghim, Đóng cửa sổ, Kết thúc tác vụ); chuột phải vào Start/đồng hồ/khay mở menu tương ứng của Windows. Dock dùng lại dữ liệu của taskbar Windows nên hai bên không lệch nhau (xem bên dưới).
  - **Chất liệu** *Acrylic / Mica / Glass* áp cho cửa sổ Vire, Vire Dock, Explorer và Settings. Kính vẫn giữ nguyên khi cửa sổ không được chọn.
  - **Kính cho hệ thống**: bật hiệu ứng trong suốt của Windows cho Start, thanh thông báo, Settings và File Explorer.
  - Dock lấy danh sách app, thứ tự và app đã ghim **trực tiếp từ taskbar Windows** (qua UI Automation); rê chuột lên icon hiện ảnh xem trước cửa sổ (bấm để chuyển, bấm chuột giữa để đóng). Ghim/bỏ ghim ở taskbar Windows thì Dock tự theo; ghim trên Dock chỉ có tác dụng trên Dock vì Windows không cho app khác sửa taskbar.
  - Lưu ý: icon khay của app khác và jump list (mục gần đây khi chuột phải) hiện chỉ có trên taskbar Windows.
- **Cài đặt**: Lấp đầy / Vừa khung, *tự nghỉ khi không thấy desktop* (cửa sổ phóng to, game, phim toàn màn hình; dừng riêng từng màn hình), dừng khi dùng pin, khởi động cùng Windows.
- **Đóng cửa sổ**: hình nền và Dock vẫn chạy, giao diện được giải phóng khỏi RAM. Mở lại bằng icon dưới khay; chuột phải vào icon để chọn **Thoát** (taskbar Windows và hình nền tĩnh được trả lại).
- `Vire.exe --tray`: chạy ẩn (dùng cho khởi động cùng Windows). `--tab=gallery|displays|music|widgets|taskbar|theme|settings`: mở thẳng vào tab đó. Thứ tự tab: Hình nền · Màn hình · Nhạc · Dock & Taskbar · Giao diện · Cài đặt (phần giới thiệu nằm cuối trang Cài đặt).

Dữ liệu lưu ở thư mục `Data` cạnh `Vire.exe`.

## Nhẹ cỡ nào (đo trên máy build, Win11)
- `Vire.exe` khoảng 2,7 MB, chỉ dùng DLL có sẵn của Windows.
- Chỉ Dock + widget (hình nền động tắt): khoảng **3–8 MB** RAM thật (cột Memory trong Task Manager), CPU gần 0%.
- Có hình nền động 1080p đang hiển thị: khoảng **15–45 MB** RAM thật (45 MB khi mở thêm bảng điều khiển). Dùng video 4K trên màn 1080p thì tốn thêm cỡ 100 MB, nên Vire gợi ý nén video khi thêm.
- Máy có hai card đồ họa: Vire tự đặt mình chạy trên card tiết kiệm điện (một giá trị trong *Cài đặt → Hệ thống → Màn hình → Đồ họa*, không đổi nếu bạn đã chọn sẵn), nhờ vậy không nạp driver card rời vào tiến trình (~70 MB). Lần đầu chạy ở một thư mục mới, Vire tự khởi động lại một lần để áp dụng.

## Kỹ thuật
| Phần | Cách làm |
|---|---|
| Hình nền dưới icon | `Progman` → `0x052C` → `WorkerW` (Win10/11 cũ) hoặc cửa sổ con của `Progman` dưới `SHELLDLL_DefView` (Win11 24H2+) |
| Phát video | Media Foundation `IMFMediaEngine`, giải mã phần cứng, `SetLoop` lặp liền mạch |
| Ảnh xem trước | Lấy thumbnail của Explorer; nếu Windows không có (thường gặp với video 2K/4K), tự giải mã khung hình ở giây thứ 1 bằng Media Foundation Source Reader |
| Nghỉ khi bị che | 2 lần/giây liệt kê cửa sổ; dừng riêng từng màn hình |
| Vire Dock | 2 cửa sổ trên 1 luồng. Tấm kính là cửa sổ **thường** (không phải tool window, vì DWM tắt làm mờ với tool window) dùng accent blur/acrylic. Góc bo dùng góc bo cửa sổ của Windows 11 (`DWMWCP_ROUND`), vì DWM bỏ qua vùng cắt (`SetWindowRgn`) khi làm mờ. Lớp icon là cửa sổ layered do CPU vẽ (viền kính, vệt bóng, icon, phóng to, chú thích, đồng hồ), chỉ vẽ lại khi có thay đổi. Trạng thái được **đối chiếu mỗi 100 ms** theo "cài đặt mong muốn", nên lệnh bật/tắt/đổi cạnh không bao giờ bị rơi. Chiếm chỗ màn hình qua AppBar (`SHAppBarMessage`) |
| Dock theo taskbar Windows | `vdock/tbmirror.rs`: đọc nút của taskbar thật bằng **UI Automation** (mỗi giây một lần, trên luồng riêng, không cần quyền gì): mã app (`Appid:`), tên, đã ghim hay chưa. Danh sách và thứ tự trên Dock lấy từ đó. Cửa sổ đang chạy được ghép theo AppUserModelID (app Store như Claude, Terminal lấy qua `GetApplicationUserModelId`). Windows không cho app khác ghim vào taskbar, nên ghim chỉ trên Dock là riêng Dock, còn bỏ ghim một app của taskbar chỉ ẩn nó khỏi Dock |
| Taskbar gốc "vô hình" | Không ẩn hẳn (cửa sổ ẩn thì UI Automation không đọc được gì) mà thu vùng cửa sổ về 1 điểm ảnh dưới mép màn hình bằng `SetWindowRgn`, kèm tự động ẩn thanh để nhả chỗ làm việc |
| Xem trước cửa sổ | `vdock/preview.rs`: rê chuột nửa giây lên icon thì hiện cửa sổ phụ với hình thu nhỏ do chính DWM vẽ (`DwmRegisterThumbnail`), không sao chép điểm ảnh nên không tốn RAM. Bấm để chuyển, bấm chuột giữa để đóng |
| Đổi hình nền không chớp | Video mới được nạp trong một bộ phát thứ hai nằm **dưới** bộ phát đang chiếu; khi nó có khung hình (hoặc sau 3 s) mới ẩn bộ phát cũ và cất vào kho dùng lại |
| Nhạc đang phát được giữ | Đổi hình nền mà bài đang phát cũng nằm trong danh sách của hình nền mới thì bài đó phát tiếp (lệnh `Requeue`), hết bài mới sang bài kế |
| Card đồ họa tiết kiệm điện | `src/gpu.rs`: ghi `GpuPreference=1` cho chính `Vire.exe` trong `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` (không ghi đè nếu đã có) |
| Nén video | `src/media.rs`: bộ chuyển mã `MediaTranscoder` của Windows (chạy trên luồng MTA riêng), đưa video về đúng độ phân giải của màn hình lớn nhất; chỉ chạy khi người dùng đồng ý |
| Kính cửa sổ Vire | Accent blur thay cho Mica/Acrylic hệ thống, vì kính hệ thống chuyển thành xám đặc khi cửa sổ không được chọn |
| An toàn | Khi khởi động, Vire dọn mọi thay đổi cũ trên taskbar (vùng cắt, kiểu cửa sổ). Nếu gặp lỗi bất ngờ, Vire trả lại taskbar Windows trước khi dừng. Nếu Explorer bị tắt nhầm, Vire tự mở lại sau vài giây |

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
  model.rs          dữ liệu: thư viện hình nền / nhạc, cài đặt, lưu JSON trong Data cạnh exe; chuyển dữ liệu cũ
  engine.rs         hình nền động: mỗi màn hình một IMFMediaEngine, tạm dừng thông minh, tái dùng player, đổi hình nền không chớp
  player.rs         trình phát nhạc (hàng đợi, lặp, trộn) trên luồng engine
  media.rs          nhập video / nhạc, thumbnail, đo độ phân giải, nén video theo màn hình
  gpu.rs            chọn card đồ họa tiết kiệm điện cho Vire
  desktop.rs        gắn cửa sổ vào desktop, danh sách màn hình, cửa sổ che / toàn màn hình
  vdock.rs + vdock/ Vire Dock: items, draw, click, menus (menu kiểu Windows), tasks, runtime (luồng + poll),
                    tbmirror (đọc taskbar thật), preview (xem trước cửa sổ), winutil
  widget.rs         widget nhạc nhỏ trên desktop + lời bài hát (thử nghiệm)
  widgets/          Widget Center: mod (mô hình, hồ sơ, registry), host (cửa sổ desktop), clock, viz
  audio.rs, dsp.rs  nghe âm thanh hệ thống (WASAPI loopback) -> FFT / dải tần; không phụ thuộc giao diện
  raster.rs         bộ vẽ CPU (hình bo, chữ, kính, đường, phát sáng) dùng chung cho Dock và widget
  ui.rs + ui/       cửa sổ chính (egui): theme, widgets (nút), kit (điều khiển cho widget), pages, wallpapers,
                    widgets_page, cinematic (giao diện Cinematic), hud (giao diện Cyberpunk), music_*, player_panel
  watchdog.rs       ghi hang.log khi một luồng kẹt > 5 s (chẩn đoán treo)
  tray.rs, autostart.rs, sysglass.rs, nowplaying.rs, lyrics.rs
                    icon khay, khởi động cùng Windows, kính hệ thống, nguồn "đang phát", lời bài hát (thử nghiệm)
```
Quy tắc: widget chỉ vẽ + đọc cài đặt (không biết về Dock hay cửa sổ chính); âm thanh đi một chiều audio -> dsp -> widget;
mọi thứ dùng chung khoá (`AppState`, `Hub`) được giữ ngắn, không bao giờ trong lúc chờ luồng khác.

## Nơi lưu dữ liệu (portable)
Hình nền, nhạc, cài đặt, thư viện, widget và log nằm trong **`Data`** ngay cạnh `Vire.exe`. Lần đầu chạy bản này, dữ liệu cũ ở `Documents\Vire` và `%APPDATA%\Vire` được **chuyển** sang `Data` (đường dẫn trong JSON được sửa theo). Thư mục không ghi được thì Vire quay về hai chỗ cũ. `build.ps1` không bao giờ đóng `Data` vào gói phát hành.
