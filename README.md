# Claude Cache Warden

> Bản phát triển hiện tại triển khai roadmap Phase 1-3 trong phạm vi Claude-only. Phase 4 và kiến trúc nhiều nhà cung cấp không nằm trong ứng dụng.

Claude Cache Warden là ứng dụng desktop nhẹ viết bằng Tauri, dùng để quét và dọn cache của Claude Desktop / Cowork trên Windows và macOS.

Ứng dụng được thiết kế theo hướng local-only:

- không telemetry
- không backend service
- không yêu cầu quyền admin/root với các đường dẫn cache được hỗ trợ
- chỉ cho phép xóa trong các root cache Claude đã biết

## Công nghệ sử dụng

- Tauri v2
- Rust backend
- React + TypeScript frontend
- Tailwind CSS
- GitHub API công khai để hiển thị trạng thái lỗi đã biết

## Tính năng

- Quét đệ quy cache, hiện dung lượng, số file, số thư mục và mức độ an toàn.
- Hiển thị treemap để nhìn nhanh khu vực nào đang chiếm nhiều dung lượng.
- Dọn thủ công các thư mục cache được chọn.
- Kiểm tra trạng thái Claude trước khi dọn.
- Dọn tự động theo một trong hai điều kiện:
  - đến giờ lịch
  - vượt ngưỡng dung lượng (GB)
- Cảnh báo tốc độ tăng cache theo GB/giờ dựa trên mẫu local.
- System tray icon với các thao tác show, scan và quit.
- Lịch sử cleanup.
- Xuất báo cáo JSON để gửi bug report.
- Bắt buộc xem trước từ backend trước mỗi lần dọn thủ công.
- Báo cáo chi tiết theo từng đường dẫn: dọn hoàn toàn, dọn một phần, bỏ qua, thất bại hoặc đã cách ly.
- Vùng cách ly dành riêng cho mục `Caution`, giữ mặc định 7 ngày và hỗ trợ khôi phục khi Claude đã thoát hẳn.
- Mở vị trí đã xác thực trong Explorer/Finder; không nhận đường dẫn tùy ý ngoài vùng Claude/CCW.
- Phân tích có giới hạn các tệp/thư mục lớn nhất và phân loại loại tệp ở mức ước đoán.
- Lịch hằng ngày, hằng tuần, hằng tháng hoặc khi mở ứng dụng; có cửa sổ bù lịch và chống chạy trùng.
- Tự dọn mục `Safe` theo dung lượng đĩa trống, có cooldown và giới hạn byte mỗi lần.
- Tùy chọn mở cùng Windows và khởi động thu nhỏ xuống khay; không tạo Windows service.
- Tab Known Issues lấy trạng thái issue công khai từ:
  - `anthropics/claude-code#43390`
  - `anthropics/claude-code#37617`
  - `anthropics/claude-code#34602`

## Mascot

Giao diện sử dụng bộ sprite ếch pixel-art trong thư mục `action/` làm lớp trạng thái trung tâm cho màn hình Tổng quan.

- Idle: `NORMAL.png` kết hợp `OPEN_CLOSE_EYES/OPEN.png` và `CLOSE.png`, lặp chậm khi app đang chờ.
- Alert: `ALERT/UP.png` và `DOWN.png`, lặp khi `growth.active === true`.
- Cleaning: `THROW_TRASH/BIN_NOR.png`, `BIN_NO.png`, `BIN_W_FOLDER.png`, `BIN_FOLDER_END.png`, phát một lần khi bấm `Clean now`.

Animation được giữ theo kiểu frame-by-frame với `image-rendering: pixelated`; không tween và không blur giữa các frame.

## Đa ngôn ngữ

Nội dung UI được tách trong `src/i18n.ts` với hai bộ từ điển riêng:

- `en`: tiếng Anh
- `vi`: tiếng Việt

Lựa chọn ngôn ngữ được lưu trong `localStorage` với key `ccw-language`.

Dữ liệu quét từ backend vẫn có thể chứa path kỹ thuật hoặc tên thư mục chưa được đặt nhãn. Frontend chỉ nội địa hóa các nhãn đã biết như cache-root label, safety description, growth message, issue state và cleanup trigger.

## Đường dẫn được quét

macOS:

```text
~/Library/Application Support/Claude/vm_bundles/
~/Library/Application Support/Claude/vm_bundles/warm/
~/Library/Application Support/Claude/Cache/
~/Library/Application Support/Claude/Code Cache/
~/Library/Application Support/Claude/claude-code-vm/
~/Library/Application Support/Claude/claude-code/
~/Library/Caches/Claude/
```

Windows:

```text
%APPDATA%\Claude\
%LOCALAPPDATA%\Claude\
%LOCALAPPDATA%\Claude-3p\
%LOCALAPPDATA%\Temp\claude\
%LOCALAPPDATA%\Packages\Claude_*\LocalCache\
%LOCALAPPDATA%\Packages\Claude_*\TempState\
%LOCALAPPDATA%\Packages\Claude_*\LocalState\
%LOCALAPPDATA%\Packages\Claude_*\RoamingState\
%LOCALAPPDATA%\Packages\Claude_*\Settings\
%LOCALAPPDATA%\Packages\Claude_*\AC\
%LOCALAPPDATA%\Packages\Claude_*\SystemAppData\
```

App tự nhận diện hệ điều hành lúc runtime và resolve các đường dẫn phù hợp theo user profile hiện tại.

## Mô hình an toàn

Cleanup chỉ được phép khi path nằm trong một Claude cache root đã biết.

Mọi path đều được canonicalize và kiểm tra lại trong Rust ngay trước thao tác thay đổi hệ thống tệp. Frontend không phải ranh giới tin cậy. Symlink, junction/reparse point, path nằm ngoài root đã biết và lựa chọn cha/con chồng lấp đều bị từ chối.

Ba mức xử lý:

- `Safe`: cache có thể tái tạo; được phép dọn trực tiếp sau preview.
- `Caution`: không chọn mặc định và không bao giờ được xóa trực tiếp; chỉ có thể di chuyển nguyên tử cùng volume vào quarantine sau xác nhận nâng cao.
- `Protected`: không thể xóa hoặc cách ly, không có nút override. Nhóm này gồm root `LocalCache`, `LocalState`, `RoamingState`, `Settings`, `AC`, `SystemAppData`, workspace/session/config/identity/browser/project state và các tên protected tương ứng.

Force clean cũ đã bị loại bỏ hoàn toàn. Tùy chọn cho phép dọn khi Claude đang chạy chỉ thay đổi process gate; nó không thay đổi policy path và không tự bật trong luồng Caution.

### Quarantine

Quarantine nằm trong Tauri app-data tại `quarantine/<cleanup-id>/`. CCW chỉ dùng `rename` cùng volume; nếu không thể move nguyên tử thì thao tác thất bại an toàn, không fallback sang copy một phần. Mỗi entry có manifest chứa vị trí gốc, kích thước, số tệp, hạn giữ và trạng thái.

Khôi phục yêu cầu Claude đã thoát hẳn. Nếu đường dẫn gốc đã xuất hiện trở lại, CCW báo conflict và không overwrite/merge dữ liệu.

### Báo cáo và quyền riêng tư

Báo cáo mặc định thay user profile Windows bằng `%USERPROFILE%` và home macOS bằng `~`. Nút xuất chẩn đoán đầy đủ là luồng riêng, có cảnh báo trước vì chứa đường dẫn riêng tư. Ứng dụng không thêm telemetry, tài khoản, cloud sync hay upload log.

## Automation Phase 3

- Daily/weekly/monthly dùng grace window mặc định 30 phút, nên máy thức dậy sau lịch vẫn có thể chạy đúng một lần.
- Ngày tháng không tồn tại dùng ngày hợp lệ cuối cùng của tháng.
- Marker occurrence ngăn cùng một lịch chạy lặp lại khi đồng hồ lùi.
- Startup cleanup có delay, chỉ chạy một lần trong mỗi lần mở app và chỉ dùng target `Safe` mặc định.
- Disk-space cleanup bị tắt mặc định, lọc target theo volume, tôn trọng cooldown 6 giờ và giới hạn byte mỗi lượt.
- Khi Claude hoạt động, automation bị chặn trừ khi người dùng chủ động bật tùy chọn tương ứng. Caution/Protected không bao giờ được automation xử lý.
- Launch at login dùng đăng ký current-user trên Windows, không cần admin; chạy với `--minimized`. Mở app thủ công vẫn hiển thị cửa sổ bình thường.

Lựa chọn mặc định chỉ tự động chọn các vị trí được đánh dấu cho default cleanup, ví dụ:

- renderer cache
- code cache
- warm VM bundle cache
- Claude temp files

Một số vị trí mới trông thấy có thể được gán nhãn `Safe` nhưng vẫn không vào default cleanup cho đến khi debug output xác nhận nội dung bên trong. Các thư mục Claude cấp cao, thư mục giống config, và thư mục giống session sẽ bị gán `NotRecommended` và backend từ chối xóa.

Nếu Claude đang mở hoặc đang chạy nền và khóa file cache, cleanup sẽ bị chặn trừ khi người dùng chủ động bật tùy chọn cho phép cleanup khi Claude đang chạy.

## Trạng thái Claude

App phân biệt 3 trạng thái:

- `Not detected`: không phát hiện process Claude
- `Background`: không có cửa sổ hiển thị, nhưng vẫn còn process Claude đang chạy nền
- `Window`: Claude đang có cửa sổ hiển thị

Trạng thái `Background` rất quan trọng trên Windows, vì Claude có thể đã đóng cửa sổ nhưng vẫn khóa các file như `DIPS`, `DIPS-wal`, `journal.baj`. Trong trường hợp này app sẽ chặn cleanup và báo người dùng thoát hẳn Claude từ tray hoặc Task Manager.

## Giới hạn hiện tại

- Windows safe-folder classification vẫn phụ thuộc vào việc đối chiếu tên thư mục con thực tế trong các Claude roots thông thường và Microsoft Store package roots.
- Automatic cleanup kiểm tra scheduler mỗi phút, nhưng full recursive scan được giảm tần suất xấp xỉ 10 phút trừ khi mtime của root thay đổi.
- macOS đã có logic scan path và icon bundle, nhưng bản phát hành hiện tại mới được verify build/runtime trên Windows.
- Launch at login hiện chỉ được triển khai trên Windows. macOS sẽ từ chối bật thay vì tạo Login Item chưa được xác thực đúng.
- Phát hiện file lock trên Windows phân biệt sharing violation/access denied nhưng không tuyên bố chính xác process nào đang giữ lock.
- Phân loại loại tệp dựa trên tên/extension/path và được ghi rõ là best-effort, không đọc nội dung tệp.
- Quarantine cùng volume không làm tăng dung lượng đĩa trống cho tới khi entry bị xóa vĩnh viễn.
- Scheduler chỉ hoạt động khi Claude Cache Warden đang chạy; ứng dụng không cài service nền.

## Development

Yêu cầu:

- Node.js 20+
- Rust stable toolchain
- Tauri platform prerequisites:
  - macOS: Xcode Command Line Tools
  - Windows: Microsoft C++ Build Tools và WebView2 runtime

Cài dependency:

```bash
npm install
```

Chạy web UI:

```bash
npm run dev
```

Chạy Tauri app:

```bash
npm run tauri:dev
```

Build bundle production:

```bash
npm run tauri:build
```

Đóng gói bản portable Windows sau khi đã có release executable:

```bash
npm run package:portable
```

## Portable build

Bản portable phù hợp khi cần gửi nhanh cho người khác test trên Windows mà không muốn bắt họ cài đặt app.

Script `npm run package:portable` sẽ:

- lấy `src-tauri/target/release/claude-cache-warden.exe`
- copy sang `dist-portable/ClaudeCacheWarden-portable/Claude Cache Warden (Portable).exe`
- thêm `README-portable.txt` song ngữ
- tạo file zip `dist-portable/ClaudeCacheWarden-portable-v0.1.0.zip`

Lưu ý:

- Bản portable không tự cài Microsoft Edge WebView2 Runtime.
- Nhiều máy Windows 11 đã có sẵn WebView2, nhưng một số máy Windows 10 cũ có thể cần cài thêm.
- Vì chưa code signing, Windows SmartScreen có thể cảnh báo ở lần chạy đầu.

Dùng installer NSIS/MSI tạo bởi `npm run tauri:build` cho luồng phân phối chính thức. Dùng `npm run package:portable` cho việc gửi nhanh và test không chính thức.

## Kiểm tra kích thước release

Kích thước quan trọng khi publish là kích thước installer cuối cùng, không phải kích thước toàn bộ thư mục dev.

`node_modules` và Rust build cache như `src-tauri/target/debug` có thể rất lớn trong quá trình phát triển, nhưng không phải file người dùng cuối tải về.

Build release:

```bash
npm run tauri:build
```

Sau đó kiểm tra các file sinh ra:

- Windows NSIS: `src-tauri/target/release/bundle/nsis/*.exe`
- Windows MSI: `src-tauri/target/release/bundle/msi/*.msi`
- macOS DMG: `src-tauri/target/release/bundle/dmg/*.dmg`

Đây mới là kích thước cần quan tâm khi publish. Tauri dùng WebView của hệ điều hành thay vì đóng gói sẵn browser engine, nên installer thường nhỏ hơn rất nhiều so với thư mục dev.

## Icon

Bộ icon hiện tại được tạo bằng:

```bash
npm run tauri -- icon action/NORMAL_icon_1024.png
```

Nguồn icon này là bản upscale 1024x1024 tạm thời của sprite ếch tại `action/NORMAL.png`. Có thể thay sau bằng một ảnh vuông chất lượng cao hơn.

## Validation

Frontend:

```bash
npm run check
```

Rust backend:

```bash
cd src-tauri
cargo fmt
cargo check
cargo test
```

CI chạy cùng các bước trên bằng Windows GitHub Actions. Workflow chỉ kiểm tra, không publish release.

Nếu máy hiện tại chưa có Rust/Cargo trong `PATH`, cần cài hoặc thêm đúng toolchain trước khi build Tauri backend.
