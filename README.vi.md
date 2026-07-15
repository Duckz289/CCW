# Claude Cache Warden

**[English](README.md) · [Tiếng Việt](README.vi.md)**

Claude Cache Warden là ứng dụng desktop nhẹ, ưu tiên chạy cục bộ, giúp kiểm tra và dọn cache của Claude Desktop / Cowork trên Windows và macOS. Ứng dụng được xây dựng bằng Tauri v2, Rust, React và TypeScript.

Nguyên tắc quan trọng nhất là an toàn: frontend không phải nguồn tin cậy và không có nút force-clean nào có thể biến thư mục Claude được bảo vệ thành có thể xóa.

> Phạm vi hiện tại: chỉ quản lý cache Claude. Không triển khai đa nhà cung cấp, cloud sync, tài khoản, telemetry hay upload log từ xa.

## Tải ứng dụng

Bộ cài Windows được đăng trên trang [Releases](https://github.com/Duckz289/CCW/releases).

- **NSIS Setup EXE** là lựa chọn nên dùng cho hầu hết người dùng Windows.
- **MSI** dành cho môi trường cần gói cài đặt Windows Installer.

## Tính năng

- Quét các cache root Claude đã biết, hiển thị dung lượng, số file, số thư mục và mức độ an toàn.
- Hiển thị treemap pixel-art để dễ xem vùng cache nào đang chiếm nhiều dung lượng.
- Bắt buộc backend tạo bản xem trước trước mọi lần dọn.
- Báo cáo kết quả có cấu trúc: dọn hoàn toàn, dọn một phần, bỏ qua, thất bại hoặc đã cách ly.
- Đưa mục mức Caution vào quarantine có thể khôi phục thay vì xóa trực tiếp.
- Phân tích file/thư mục lớn, phân loại loại tệp và báo mục bị khóa hoặc không truy cập được.
- Tự động dọn theo lịch, lúc khởi động, ngưỡng dung lượng hoặc dung lượng ổ đĩa trống.
- Có system tray, tùy chọn mở cùng Windows và khởi động thu nhỏ.
- Xuất báo cáo JSON. Sau khi xuất báo cáo thường hoặc báo cáo chẩn đoán, popup sẽ hiện file vừa tạo và có nút mở thư mục chứa file.

## Mô hình an toàn

Mọi đường dẫn được yêu cầu đều do Rust kiểm tra độc lập lúc xem trước và kiểm tra lại ngay trước khi thay đổi hệ thống tệp.

| Mức độ | Cách xử lý |
| --- | --- |
| **Safe** | Cache có thể tái tạo; được dọn sau khi người dùng xem trước và xác nhận. |
| **Caution** | Không bao giờ bị xóa trực tiếp; chỉ có thể di chuyển nguyên tử vào vùng quarantine sau xác nhận bổ sung. |
| **Protected** | State, cài đặt, phiên làm việc, dữ liệu project/workspace, identity/browser state và vị trí nhạy cảm tương tự. Không thể xóa hoặc cách ly. |

Backend canonicalize đường dẫn và từ chối path traversal, symlink/junction/reparse point, root không được biết, nhánh Protected, lựa chọn cha/con chồng lấp và path đã cũ sau lần quét. Tùy chọn dọn khi Claude đang chạy chỉ thay đổi process gate, không bao giờ làm yếu chính sách đường dẫn.

### Quarantine

Quarantine chỉ dành cho mục Caution. CCW dùng di chuyển nguyên tử trên cùng volume; nếu không thể thực hiện, thao tác sẽ thất bại an toàn và không fallback sang copy một phần.

Mỗi entry lưu vị trí gốc, dung lượng, số file, thời gian tạo, thời hạn lưu và trạng thái khôi phục. Khi khôi phục, Claude phải được thoát hoàn toàn; CCW từ chối ghi đè hoặc merge nếu vị trí gốc đã tồn tại.

## Quyền riêng tư

CCW ưu tiên cục bộ:

- Không telemetry hoặc analytics
- Không tài khoản/đăng nhập
- Không cloud sync
- Không upload log từ xa
- Không tự động gửi issue

Báo cáo thông thường sẽ ẩn đường dẫn home (`%USERPROFILE%` trên Windows và `~` trên macOS). Xuất chẩn đoán với đường dẫn đầy đủ luôn yêu cầu cảnh báo xác nhận riêng.

## Tự động hóa

Automation bị tắt mặc định và chỉ dùng Safe target được phép dọn mặc định.

- Rule theo dung lượng ổ đĩa hỗ trợ chọn volume, mức trống tối thiểu GB, phần trăm trống tùy chọn, dung lượng mục tiêu, cooldown và giới hạn dung lượng dọn.
- Lịch chạy hỗ trợ hằng ngày, hằng tuần, hằng tháng và một lần khi mở ứng dụng. Lịch bị lỡ có grace window và occurrence marker để tránh chạy trùng.
- Dọn lúc khởi động tôn trọng delay, cooldown, trạng thái Claude và chính sách Safe-only.
- Trên Windows, mở cùng Windows dùng registry của user hiện tại và chạy với `--minimized`; CCW không tạo Windows service.

## Vị trí được hỗ trợ

CCW chỉ kiểm tra các vị trí Claude đã biết. Ví dụ:

- Windows: `%APPDATA%\\Claude`, `%LOCALAPPDATA%\\Claude`, `%LOCALAPPDATA%\\Claude-3p`, `%LOCALAPPDATA%\\Temp\\claude` và các nhánh được cho phép của Claude Microsoft Store package.
- macOS: cache branch trong `~/Library/Application Support/Claude/` và `~/Library/Caches/Claude/`.

Mức độ an toàn cuối cùng do Rust backend quyết định lúc runtime, không do README hoặc React UI tự quyết.

## Phát triển

### Yêu cầu

- Node.js 20+
- Rust stable toolchain
- Tauri prerequisite theo hệ điều hành
  - Windows: Microsoft C++ Build Tools và WebView2 Runtime
  - macOS: Xcode Command Line Tools

### Cài và chạy

```bash
npm install
npm run tauri:dev
```

### Kiểm tra

```bash
npm run check
cd src-tauri
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

### Build installer

```bash
npm run tauri:build
```

File Windows sinh ra:

- `src-tauri/target/release/bundle/nsis/*.exe`
- `src-tauri/target/release/bundle/msi/*.msi`

## Giới hạn hiện tại

- Windows runtime và installer là luồng phát hành đã được kiểm chứng; macOS cần kiểm chứng riêng theo nền tảng.
- Quarantine chỉ hỗ trợ move nguyên tử cùng volume.
- Phân loại loại tệp là best-effort dựa trên tên, extension và path; CCW không đọc nội dung file.
- Báo file bị khóa nhận diện lỗi lock/access có khả năng cao, nhưng không khẳng định process cụ thể đang giữ lock.
- Scheduler chỉ chạy khi CCW đang mở; ứng dụng không cài background service.

## License

Xem license của repository nếu có.
