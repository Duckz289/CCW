# Claude Cache Warden

Claude Cache Warden là một app desktop nhỏ gọn, sinh ra để giải quyết một vấn đề khá phổ biến: Claude Desktop (đặc biệt là Cowork) ghi cache vào ổ đĩa nhiều hơn mức cần thiết, đôi khi phình lên rất nhanh mà không ai để ý cho tới khi máy đầy ổ. App này giúp mình nhìn thấy rõ cache đang nằm ở đâu, nặng bao nhiêu, và dọn nó đi một cách an toàn.

Không thu thập dữ liệu, không có server nào đứng sau, chạy hoàn toàn local trên máy. Cũng không cần quyền admin — mọi thứ app đụng vào đều nằm trong thư mục cá nhân của người dùng.

## App làm được gì

- Quét thư mục cache của Claude, cho biết đang chiếm bao nhiêu dung lượng, và đánh giá phần nào an toàn để xóa, phần nào nên cẩn thận.
- Hiển thị trực quan kiểu treemap, nhìn phát biết ngay chỗ nào đang ăn dung lượng nhiều nhất.
- Cho chọn tay từng thư mục muốn dọn, không ép xóa hàng loạt.
- Tự kiểm tra Claude có đang mở hay chạy nền không trước khi xóa, tránh đụng vào file đang được dùng dở.
- Có thể để app tự dọn định kỳ — theo lịch, hoặc khi cache vượt một mức dung lượng mình đặt ra, cái nào tới trước thì chạy trước.
- Theo dõi tốc độ cache phình ra theo thời gian, để cảnh báo sớm nếu có gì bất thường thay vì đợi đầy ổ mới biết.
- Có icon trên system tray, tiện bấm mở app, quét lại, hoặc thoát nhanh.
- Lưu lại lịch sử những lần đã dọn, và xuất được báo cáo nếu cần gửi kèm khi report bug.
- Có mục Known Issues, tự cập nhật trạng thái các bug liên quan đã được ghi nhận công khai trên GitHub của Anthropic.

## Về độ an toàn

App chỉ được phép xóa trong những khu vực cache đã biết rõ của Claude — không đụng vào bất cứ thứ gì ngoài phạm vi đó.

Một số thư mục dù trông có vẻ an toàn vẫn được giữ ở chế độ "cần xác nhận thêm" trước khi đưa vào danh sách tự động xóa, để chắc ăn hơn là đoán bừa. Những thư mục có khả năng chứa cấu hình hay dữ liệu phiên làm việc thì app sẽ không cho xóa luôn, dù người dùng có cố tình chọn.

Nếu Claude đang mở hoặc vẫn còn chạy ngầm (kể cả khi đã đóng cửa sổ), app sẽ chặn việc dọn dẹp cho tới khi Claude được thoát hẳn, hoặc người dùng tự ý bật chế độ cho phép dọn trong lúc Claude đang chạy.

## Dùng thử / build

Cần có Node.js và Rust cài sẵn trên máy.

```bash
npm install
npm run tauri:dev     # chạy thử app
npm run tauri:build   # build bản chính thức (installer)
npm run package:portable   # đóng gói bản chạy thẳng, không cần cài đặt
```

Bản portable tiện khi muốn gửi nhanh cho ai đó test mà không bắt họ cài đặt gì — chỉ cần giải nén và chạy. Vì chưa được ký số nên Windows có thể cảnh báo ở lần mở đầu tiên, bấm "More info" rồi "Run anyway" là chạy bình thường. Máy Windows 10 đời cũ có thể cần cài thêm WebView2 Runtime nếu app không mở lên được.

## Đang hỗ trợ

Hiện tại app đã chạy ổn định và được test kỹ trên Windows. Phần macOS đã có code xử lý nhưng chưa được kiểm chứng thực tế nhiều bằng bản Windows.

## Icon

Icon hiện tại lấy từ hình ếch pixel-art của app (`action/NORMAL.png`), phóng lên làm ảnh gốc để sinh bộ icon. Có thể thay bằng ảnh khác đẹp hơn sau này nếu muốn.