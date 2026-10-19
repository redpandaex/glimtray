<p align="center">
  <img src="https://raw.githubusercontent.com/redpandaex/glimtray/main/assets/icons/glimtray.svg" width="64" height="64" alt="GlimTray 图标">
</p>

# GlimTray · 微光

[![Build](https://github.com/redpandaex/glimtray/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/redpandaex/glimtray/actions/workflows/build.yml)
[![Release](https://img.shields.io/github/v/release/redpandaex/glimtray)](https://github.com/redpandaex/glimtray/releases)

GlimTray 是一个使用 Rust 和 Slint 开发的 Windows 托盘亮度调节工具，通过 DDC/CI 控制外接显示器。

点击托盘图标即可调节每台显示器的亮度，操作完成后收起浮层。

[下载版本](https://github.com/redpandaex/glimtray/releases) · [反馈问题](https://github.com/redpandaex/glimtray/issues) · [参与贡献](https://github.com/redpandaex/glimtray/blob/main/CONTRIBUTING.md) · [开发文档](https://github.com/redpandaex/glimtray/blob/main/docs/development.md)

## 功能

- **多屏调节**：分别显示和调整每台受支持显示器的亮度。
- **托盘浮层**：左击展开，点击外部、按 `Esc` 或再次左击收起。
- **自动读取**：每次展开时重新枚举设备并读取亮度，支持插拔后更新设备列表。
- **开机自启**：托盘菜单根据当前状态显示开启或关闭操作，支持当前用户的登录自启动。
- **主题适配**：托盘图标跟随 Windows 系统任务栏的深浅主题。
- **便携运行**：下载 EXE 即可运行，无需安装，也提供 ZIP；当前本地 Release 构建经 UPX 压缩后约 1.0 MiB，具体大小以发布附件为准。

## 运行要求

- Windows x64。
- 支持 DDC/CI 的外接显示器，并在显示器自身菜单中开启 DDC/CI。
- 显卡、连接线和转接设备能够传递 DDC/CI 指令。

显示器和连接链路会影响兼容性。部分扩展坞、KVM 和转接器可能无法传递亮度控制指令。

## 下载与使用

1. 前往 [Releases](https://github.com/redpandaex/glimtray/releases)，下载 Windows x64 的 `*-compact.exe`，保存到固定目录后双击运行。程序启动后驻留托盘。
2. 也可下载 `*-compact.zip`，完整解压后运行其中的 `glimtray.exe`，与单独下载的 EXE 是同一程序。发布页的 SHA256 校验文件同时覆盖 EXE 和 ZIP；旧版本若仅提供 ZIP，解压运行即可。
3. 左击托盘图标，等待亮度读取完成后拖动对应滑块，松开后应用亮度。读取期间滑块暂时禁用。
4. 右键托盘图标，点击“开启开机自启”即可启用；保存成功后菜单变为“关闭开机自启”，再次点击即可关闭。选择“退出”结束程序。

自启设置默认关闭，使用当前用户的 Windows `Run` 注册项，无需管理员权限。移动程序位置后，需要从新位置重新启用自启。

菜单文字只在保存成功后切换；保存失败时保留原状态并提示原因。无法读取设置时，自启菜单显示为“开机自启（不可用）”。

## 常见问题

### 找不到显示器，或者滑块不可用？

先检查显示器菜单中的 DDC/CI 开关，再确认当前连接方式支持 DDC/CI。插拔或切换连接后，收起并重新打开浮层，程序会重新读取设备。无法控制的设备会显示错误原因。

### 为什么任务管理器中有两个进程？

主进程负责托盘和亮度浮层，控制子进程负责显示器通信。显示器调用超时后，程序可以结束并重建控制进程，避免硬件调用阻塞界面。

### 为什么有时显示“显示器 1”等编号？

程序优先读取 Windows 显示配置中的显示器名称，并匹配到对应屏幕。无法读取有效名称，或镜像等情况无法确认一一对应关系时，会使用有效的硬件描述或编号，避免显示通用占位名称或贴错设备名称。

### 为什么 EXE 很小，运行时内存占用仍高于文件大小？

发布包使用 UPX 压缩可执行文件，程序启动时会解压。运行时还需要界面、图形渲染和字体等资源，因此磁盘文件大小与内存占用不是同一个指标。

## 从源码构建

需要 Rust stable（MSVC 工具链）、Visual Studio C++ 构建工具和 Windows SDK。

```powershell
git clone https://github.com/redpandaex/glimtray.git
cd glimtray
cargo run --locked
```

构建 Release 版本：

```powershell
cargo +nightly-2026-10-18 release-build
```

产物位于 `target/x86_64-pc-windows-msvc/release/glimtray.exe`。stable 的 `cargo build --release` 仍可用于快速检查，但不含 panic 裁剪，体积更大。便携打包、UPX 压缩、VS Code 配置和依赖裁剪说明见[开发文档](https://github.com/redpandaex/glimtray/blob/main/docs/development.md)。

## 当前限制

- 尚未接入笔记本内屏的 WMI 亮度控制。
- 暂不支持全局快捷键和定时调节。
- 当前构建关闭了 Slint 无障碍集成，读屏支持有限。
- 为控制文件体积，裁剪了部分复杂语言分词字典与图片格式支持；具体影响见 [Slint 补丁说明](https://github.com/redpandaex/glimtray/blob/main/vendor/README.md)。

## 参与贡献

欢迎反馈显示器兼容性问题、修复缺陷或改进文档。提交 Issue 时，请提供程序版本、Windows 版本、显示器型号、连接方式和复现步骤。

开发检查、问题报告及真实设备验收说明见 [贡献指南](https://github.com/redpandaex/glimtray/blob/main/CONTRIBUTING.md)。

## 许可证

项目级许可证暂未指定。

Slint 等第三方组件保留各自的许可证。仓库中的 Slint 源码及许可文件见 [vendor](https://github.com/redpandaex/glimtray/blob/main/vendor/README.md) 和[上游许可证目录](https://github.com/redpandaex/glimtray/tree/main/vendor/i-slint-core-1.18.1/LICENSES)。
