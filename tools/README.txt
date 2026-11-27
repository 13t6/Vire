Vire - hình nền động, Dock và widget cho Windows 11
====================================================

Chạy:   bấm đúp Vire.exe (cần Windows 10/11, không cần cài đặt hay quyền quản trị).
        Vire nằm ở khay hệ thống; bấm vào biểu tượng để mở bảng điều khiển.

Dữ liệu của bạn nằm trong thư mục Data ngay cạnh Vire.exe (tự tạo ở lần chạy đầu):
  Data\Wallpapers , Data\Music ... hình nền và nhạc (thả file vào đây là Vire tự nạp)
  Data\*.json ................... cài đặt, thư viện, widget
Có thể chép cả thư mục sang máy khác. Xoá thư mục Vire là gỡ xong ứng dụng.
(Nếu thư mục không ghi được, ví dụ trong Program Files, Vire dùng Documents\Vire và %APPDATA%\Vire.)

Gặp lỗi? Chạy Vire-debug.bat khi lỗi xảy ra rồi gửi Data\debug.log
(và Data\hang.log nếu Vire từng bị treo).

Âm thanh cho Music Visualizer: Vire chỉ phân tích tần số của âm thanh Windows đang phát
(WASAPI loopback). Không dùng micro, không ghi âm, không lưu, không gửi đi.
Có thể tắt trong tab Widgets > "Nghe âm thanh hệ thống".
