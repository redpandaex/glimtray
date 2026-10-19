# GlimTray 图标

图标采用与 Windows 系统状态图标接近的单色亮度符号：圆形中心、八条短光线和统一的圆头线条。应用图标为 Windows 蓝色，托盘图标分别提供深色和浅色版本。

| 文件 | 用途 |
| --- | --- |
| `glimtray.svg` | 可编辑的蓝色单线条源文件，透明背景 |
| `glimtray.png` | 256 × 256，Slint 窗口图标 |
| `glimtray.ico` | Windows 可执行文件图标，包含多种尺寸 |
| `tray-on-light.svg` / `.png` | 浅色任务栏使用的深色图形 |
| `tray-on-dark.svg` / `.png` | 深色任务栏使用的浅色图形 |
| `preview.png` | 应用图标和深浅主题的预览 |

应用 ICO 包含 16、20、24、32、40、48、64、128、256 像素。托盘使用 32 × 32 PNG。

SVG 是源文件。修改设计后，应重新导出对应 PNG 和应用 ICO，并在 16、20、24、32 像素下检查辨识度。

`build.rs` 将应用 ICO 编入 Windows 可执行文件，Slint 窗口使用应用 PNG。托盘组件根据 Windows 的 `SystemUsesLightTheme` 选择黑白图标，每 5 秒检查一次主题变化。
