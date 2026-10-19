# 开发文档

[返回 README](../README.md) · [贡献指南](../CONTRIBUTING.md)

## 环境

- Windows x64。
- Rust stable，使用 MSVC 工具链。
- Visual Studio C++ 构建工具及 Windows SDK。

首次构建会下载并编译依赖。仓库已包含所需的 Slint 补丁源码，克隆后可以直接使用 Cargo 构建。

### VS Code

在扩展面板搜索 `@recommended`，安装工作区推荐扩展：

| 扩展 | 用途 |
| --- | --- |
| rust-analyzer | Rust 补全、诊断和格式化 |
| Slint | `.slint` 补全、格式化和预览 |
| CodeLLDB | Rust 调试 |
| Even Better TOML | Cargo 配置编辑 |
| Code Spell Checker | 英文拼写检查，项目词典见 `cspell.json` |

`Ctrl+Shift+B` 构建，`F5` 调试；其他 Cargo 任务见 [tasks.json](../.vscode/tasks.json)。

## 开发与检查

在仓库根目录运行：

```powershell
cargo run --locked
cargo check --locked
cargo fmt --package glimtray -- --check
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo test --locked
```

格式化和 Clippy 命令面向应用包，避免重新格式化或检查未修改的上游源码。真实显示器调节需要额外进行[设备验收](../CONTRIBUTING.md#真实设备验收)。

只读诊断：

```powershell
cargo run --locked -- --list
```

该命令输出设备 JSON，包含本次枚举的标识、名称、连接和亮度。本次枚举的设备标识不适合作为长期配置中的固定 ID。

## 构建与打包

```powershell
cargo build --release --locked
./scripts/package.ps1
```

普通编译产物为 `target/release/glimtray.exe`。打包脚本生成 `dist/glimtray-<version>-windows-x64-standard.zip`，内含 `glimtray.exe` 和 README，并生成对应的 SHA256 校验文件。

### UPX 压缩

准备官方 UPX 可执行文件后，传入实际路径：

```powershell
./scripts/package.ps1 -UpxPath 'C:/tools/upx.exe'
```

脚本复制编译产物后再压缩，保留 `target/release/glimtray.exe`，生成 `*-compact.exe`、`*-compact.zip` 和校验文件，并执行 UPX 数据完整性检查。ZIP 内的程序名仍为 `glimtray.exe`。

`standard` 和 `compact` 使用相同的应用源码与裁剪依赖，名称只区分是否经过 UPX 压缩。当前本地构建未压缩约 4.44 MiB，压缩后约 1.52 MiB；实际大小会随编译器和版本变化。UPX 减少磁盘体积，运行时需要解压。

## 架构与依赖

主进程使用 Slint、Winit 和 FemtoVG，负责托盘、浮层与异步状态更新。隐藏子进程通过同一可执行文件的 `--ddc-worker` 参数启动，负责 Windows 显示器枚举和 DDC/CI 控制，通过标准输入输出通信。

设备读取的响应超时为 8 秒，亮度写入的响应超时为 3 秒。超时会结束控制子进程，重新展开浮层后可重试。连续调节请求会合并，减少重复硬件调用。

Windows x64 静态链接 CRT，无需额外安装 VC++ Redistributable。界面使用 Windows 系统字体，没有随程序分发中文字体文件。当前构建已关闭 Slint 无障碍集成。

### 为什么有 vendor 目录？

Slint 桌面端启用了 Parley 的 `complex-scripts` 特性，会引入较大的分词字典。项目将 `i-slint-core 1.18.1` 保存到 `vendor/`，仅移除这一项依赖特性，并通过 `[patch.crates-io]` 应用补丁。

Slint 和 `slint-build` 固定为 `=1.18.1`。本地和 CI 使用同一份源码；升级时需重新核对补丁。来源、校验、许可及语言支持影响见 [vendor/README.md](../vendor/README.md)。`vendor/` 只在构建时使用，无需随 EXE 分发。

## 自动构建与发布

[GitHub Actions 工作流](../.github/workflows/build.yml) 在推送 main、向 main 提交 PR 和推送 `v*` tag 时执行检查与 Windows x64 构建。

创建 Release 需要同时满足：

1. 事件为版本 tag 推送，且 tag 对应的提交属于远端 main 历史。
2. tag 去掉 `v` 后与该提交的 `Cargo.toml` 版本一致。
3. 发布校验、格式、依赖特性、Clippy、测试及 Release 构建全部成功。

发布附件为 `*-compact.zip` 和对应的 SHA256 校验文件。带 `-rc.1` 等后缀的版本会创建预发布版。普通 main 推送和 PR 只生成 Actions 构建产物。

### 发布步骤

1. 选择尚未发布的版本，更新 `Cargo.toml`，运行 `cargo update --workspace` 同步 `Cargo.lock`。
2. 完成检查，将版本变更提交并推送到 main。
3. 从当前包版本创建附注 tag，校验后推送：

```powershell
$ErrorActionPreference = 'Stop'
$releaseMetadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
$releaseVersion = ($releaseMetadata.packages | Where-Object name -eq 'glimtray').version
$releaseTag = "v$releaseVersion"
git tag -a $releaseTag -m "GlimTray $releaseTag"
./scripts/check-release.ps1 -Tag $releaseTag
git push origin $releaseTag
```

已发布 tag 应保持不变。发布规则的独立验证脚本为 `./scripts/test-release-guard.ps1`，它只在临时仓库中创建测试 tag。

## 代码导航

| 路径 | 职责 |
| --- | --- |
| `src/app.rs` | UI 状态、托盘生命周期、自启绑定和异步更新 |
| `src/flyout_window.rs`、`src/flyout.rs` | 浮层窗口、定位及收起规则 |
| `src/native.rs` | Windows 显示器枚举、亮度接口和句柄管理 |
| `src/worker/` | 子进程协议、超时管理和请求调度 |
| `src/startup.rs` | 当前用户开机自启注册、读取和移除 |
| `ui/app-window.slint` | 亮度浮层、滑块和原生托盘菜单 |
| `build.rs` | Slint 编译和 Windows 图标资源 |
| `assets/icons/` | 应用及托盘图标，含可编辑源文件 |
| `vendor/` | 固定版本的 Slint core、补丁说明及上游许可证 |

`target/`、`dist/` 和 `work/` 是本地构建与临时目录，已加入 `.gitignore`。
