# GlimTray · 微光

使用 Rust + Slint 开发的轻量 Windows 显示器控制工具。

当前已接入 Windows 外接显示器枚举、DDC/CI 亮度读取与调节、系统托盘、展开浮层自动读取和开机自启。笔记本内屏 WMI 控制、快捷键和定时调节尚未实现。

## 使用

1. 运行 `cargo run --locked`，应用启动后留在系统托盘，只枚举和读取显示器，不会自动改变亮度。
2. 左击托盘图标，在任务栏附近直接展开亮度面板。拖动滑块即可调节；点击外部、按 Esc 或再次左击托盘可收起。
3. 每次展开面板都会重新读取设备。读取期间滑块暂时禁用，完成后显示实际亮度；连续调节请求会在后台合并。
4. 插拔显示器或设备状态变化后，收起并重新打开浮层即可更新。无法读取亮度的设备会显示原因并禁用滑块。
5. 右键托盘只有“开机自启”和“退出”，不再提供独立主界面或刷新按钮。托盘图标跟随 Windows 系统任务栏的深浅主题。
6. 点击托盘菜单中的“开机自启”，下次登录 Windows 时自动驻留托盘。菜单勾选状态读取当前程序的注册状态，默认不会主动启用；取消勾选会移除 GlimTray 的启动项。保存失败时恢复原状态，并用系统对话框显示原因。

自启使用当前用户的 Windows `Run` 注册项，保存带引号的程序完整路径，无需管理员权限。移动程序后，需要从新位置重新开启自启。

浮层仅保留设备名称、亮度百分比和滑块。亮度调节时滑块位置保持固定。连接路径、DDC/CI 标记和普通状态提示不在界面中展示；无法控制的设备仍会显示错误原因。文字使用 Windows 系统字体，没有随程序打包中文字体文件。

只读诊断命令：

```powershell
cargo run --locked -- --list
```

诊断输出 JSON，包含本次枚举的设备标识、名称、连接和亮度。标识只在本次枚举期间有效，不能作为长期配置中的固定设备 ID。

DDC 操作位于隐藏子进程中。刷新响应超时设为 8 秒，亮度响应超时设为 3 秒；超时会结束控制进程并禁用控制，刷新后可重试。界面线程不等待显示器调用。

真实亮度变化、扩展坞兼容性和休眠恢复需要在实际设备上验收。自动验证只执行读取，不会修改屏幕亮度。

## 图标

![GlimTray 图标预览](assets/icons/preview.png)

图标采用接近 Windows 11 系统状态图标的简洁线条。蓝色应用图标已接入窗口和 Windows 可执行文件；黑白托盘素材提供深浅主题两个版本，见 `assets/icons/README.md`。

## 开发环境

- Windows x64、Rust stable（MSVC 工具链）。
- Visual Studio C++ 构建工具和 Windows SDK。
- VS Code 打开仓库后，在扩展面板中搜索 `@recommended`，安装工作区推荐扩展。

| 扩展 | 用途 |
| --- | --- |
| rust-analyzer | Rust 补全、诊断和格式化 |
| Slint | `.slint` 补全、格式化和实时预览 |
| CodeLLDB | F5 调试 Rust 程序 |
| Even Better TOML | Cargo 配置补全和格式化 |
| Code Spell Checker | 英文拼写检查，项目词典见 `cspell.json` |

扩展推荐只写入仓库配置，不会自动安装扩展或修改用户级设置。

## 常用命令

```powershell
cargo run --locked
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo fmt --package glimtray -- --check
cargo build --release --locked
./scripts/package.ps1
# 可选：用已下载的官方 UPX 生成压缩版，普通编译产物保持原样
./scripts/package.ps1 -UpxPath 'C:/tools/upx.exe'
```

首次构建会下载并编译依赖。当前使用 Winit + FemtoVG 渲染后端和系统托盘，已关闭 Slint 无障碍集成以精简依赖；普通鼠标、键盘操作和开机自启不依赖该集成。

Release 配置采用体积优先优化、完整 LTO、单代码生成单元和符号剥离；发生 panic 时直接终止进程。Windows x64 静态链接 CRT，不需要额外安装 VC++ Redistributable。

默认构建已采用实验版的 Slint 分词字典裁剪方案，并使用当前纯托盘界面：没有独立主界面或刷新按钮，展开浮层自动读取亮度，自启位于托盘菜单。`vendor/i-slint-core-1.18.1` 保存官方 crate 和一行依赖补丁，只移除 Parley 的 `complex-scripts` 特性。Slint 和构建工具固定为 1.18.1，本地构建和 GitHub Actions 均使用同一份补丁，来源、校验及升级说明见 `vendor/README.md`。

当前版本未压缩 EXE 为 4.44 MiB，UPX 压缩后为 1.52 MiB。`standard` 和 `compact` 都使用上述裁剪依赖与纯托盘界面，名称只区分是否经过 UPX 压缩。

该裁剪影响中日文词边界和部分东南亚语言的词典换行，当前界面为简短的只读标签。系统字体、Unicode 文字显示和字形排版仍保留；将来添加长篇文字或输入框时需重新评估。请检查设备名称和错误提示是否正常显示。实际体积会随编译器和依赖版本变化。

`cargo build --release` 生成的 `target/release/glimtray.exe` 是普通版。默认打包生成 `*-standard.zip`；传入 `-UpxPath` 会另存 `dist/*-compact.exe`，使用 UPX 压缩并检查压缩数据完整性，再生成 `*-compact.zip`。两个 ZIP 内的程序都叫 `glimtray.exe`，校验文件按版本和变体分别命名。UPX 减少磁盘和下载体积，运行时会解压，不代表内存占用同比下降或启动更快。

针对纯托盘启动后的光标转圈，主进程在托盘就绪后、隐藏 DDC 子进程在接收请求前，通过一次只读取自身线程消息的 `GetMessage` 调用结束 Windows 启动反馈。Winit 的消息循环使用 `PeekMessage`，不会执行这一步。实际效果需通过下面的启动验收确认。

在 VS Code 中，`Ctrl+Shift+B` 构建，`F5` 使用 CodeLLDB 调试；也可运行 `.vscode/tasks.json` 中的 Cargo 任务。

## 手动验收

先从托盘退出旧进程，再运行同一路径的新 EXE，避免同时启动多个实例。

1. **启动与菜单**：分别运行普通版和压缩版，启动后只出现托盘图标，不弹主窗口，检查图标就绪后光标是否仍持续转圈；退出后再重开检查一次。右键菜单应只有“开机自启”和“退出”，自启状态应与之前在同一路径保存的设置一致。
2. **浮层开关**：左击展开浮层，只显示设备名称、百分比和滑块。再次左击、点击外部或按 Esc 都应收起；浮层不应出现在任务栏。
3. **自动读取**：收起浮层，通过显示器实体菜单改变亮度，再重新展开。读取完成后百分比应更新，期间不能拖动滑块；没有刷新按钮，也不应出现“正在应用”等小字。
4. **亮度调节**：小幅拖动或用键盘方向键调节，检查真实亮度、百分比和滑块是否一致，线条是否稳定，快速连续调节后是否停在最后一次目标附近。
5. **开机自启**：勾选后关闭菜单再打开，勾选应保留；退出程序并从同一路径重开后也应保留。取消勾选后同样检查。可另外注销并登录 Windows，检查启用后是否自动驻留托盘、禁用后是否不再自动运行。
6. **设备与显示设置**：有多屏时检查每台设备均可调节，插拔后重新展开会更新列表且浮层不会越出屏幕。检查深浅主题及常用 DPI 缩放下的托盘图标、文字、滑块和菜单是否清晰。设备不可控时应显示原因并禁用对应滑块。
7. **退出**：从菜单退出后托盘图标消失，再次启动应正常工作。

## 自动构建与发布

GitHub Actions 在推送 `main`、向 `main` 提交 PR 和推送 `v*` tag 时运行检查与 Windows x64 构建，产物可在 Actions 的 Artifacts 下载。

仅版本 tag 可以创建 GitHub Release，并且必须同时满足：

1. tag 对应的提交属于远端 `main` 的历史，且构建的正是该提交。其他分支尚未合入 `main` 的提交会被拒绝。
2. tag 为 `v0.1.0` 或 `v0.1.0-rc.1` 等版本格式，去掉 `v` 后必须与该提交的 `Cargo.toml` 版本完全一致。
3. 发布校验、格式检查、依赖裁剪检查、Clippy、测试和 Release 构建全部成功。

普通推送和 PR 只生成构建产物，不创建 Release。带 `-rc.1` 等后缀的 tag 创建预发布版。工作流下载固定版本 UPX 并校验 SHA256，生成压缩版便携 ZIP（`*-compact.zip`，包含 `glimtray.exe` 和 README）及对应的 `*-SHA256SUMS.txt`；不发布普通版 ZIP。

例如发布当前 `0.1.0` 版本，在代码已提交并推送到 `main` 后执行：

```powershell
git switch main
git pull --ff-only origin main
git tag -a v0.1.0 -m "GlimTray v0.1.0"
git push origin v0.1.0
```

后续版本先修改 `Cargo.toml`，通过 Cargo 更新并提交 `Cargo.lock`，再合入 `main`、推送对应 tag。工作流使用仓库自动提供的 `GITHUB_TOKEN`，只有发布 job 具有 `contents: write` 权限。

本地可运行 `./scripts/test-release-guard.ps1` 验证发布限制。测试只在临时仓库中创建 tag，不会给当前项目打 tag。

## 目录

```text
src/main.rs          应用、只读诊断及控制子进程入口
src/app.rs           UI 状态、异步更新和托盘生命周期
src/flyout_window.rs  托盘浮层、失焦收起和窗口属性
src/flyout.rs         浮层定位与点击切换规则
src/native.rs        Windows 枚举、亮度接口和物理句柄管理
src/display.rs       显示器快照及亮度范围换算
src/queue.rs         写入合并和过期响应处理
src/startup.rs       当前用户开机自启注册、读取和移除
src/worker/          子进程协议、超时管理和后台调度
ui/app-window.slint  亮度浮层、滑块模型和原生托盘菜单
build.rs             编译 Slint 界面
assets/icons/        应用和托盘图标，含 SVG 源文件、PNG、ICO 和预览
.vscode/             工作区扩展推荐、设置、任务和调试配置
.cargo/config.toml   Windows 链接配置
vendor/             固定版本的 Slint core、分词依赖补丁和上游许可
cspell.json          项目拼写词典
scripts/            发布校验、校验测试和便携 ZIP 打包
.github/workflows/  Windows 检查、构建及 main 版本 tag 发布
```

`target/`、`dist/` 和 `work/` 已加入 `.gitignore`；`Cargo.lock` 和 `.vscode/` 保留在版本控制中，便于复现依赖和共享开发配置。
