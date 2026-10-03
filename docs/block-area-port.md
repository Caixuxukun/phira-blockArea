# Phigros 4.0.1 噪域移植

本次包含 PGR `blockAreaList` 解析、动画几何、噪域绘制、输入屏蔽和 iOS 构建。
噪域不是 `extra.json` 的普通特效：关闭特效仍会绘制噪域并保留其判定影响。

## APK 证据

分析源为工作区用户提供的 `Phigros4.0.1.apk`：

```
SHA-256 f59a041f5bc02d0f3eb51a67739ec0271590ac412e68cf6cc6b43621cdf08985
ARM64 lib/arm64-v8a/libil2cpp.so
IL2CPP metadata version 31
Unity assets/bin/Data/data.unity3d -> sharedassets12.assets
```

使用 Il2CppDumper 6.7.46 定位方法，Capstone 5.0.9 检查 ARM64 指令，UnityPy 1.25.4 导出 shader、材质及贴图。
主要方法的 ARM64 RVA：

| 方法 | RVA | 确认的行为 |
| --- | --- | --- |
| `PreviewBlockControl.UpdateBlockAnimations` | `0x1CDB864` | 先缩放，后旋转，最后叠加移动 |
| `PreviewBlockControl.InterpolateMoveEvent` | `0x1CDC5A4` | 当前关键帧到下一个关键帧；取当前帧缓动 |
| `PreviewBlockControl.UpdateBlockActivation` | `0x1CDB6B8` | 隐藏、停用、预备、激活状态 |
| `JudgeControl.ProcessBlockedTouches` | `0x1D73398` | ID 加入 `blockedFingerIds` 后持续屏蔽 |
| `JudgeControl.CheckBlocks` | `0x1D72100` | 在音符判定前移除已屏蔽手指 |
| `JudgeControl.TryGetBlockingBlock` | `0x1D735D8` | 普通区域并集与减算区域奇偶值异或 |
| `JudgeControl.TryGetBlockTouchHalfSize` | `0x1D73D08` | 普通区域内缩、减算区域外扩，局部最大 0.25 |
| `FingerManagement.SyncFingers` | `0x1D70E78` | 抬手、取消、失踪触点解除 ID 屏蔽 |

`JudgeControl` 构造器的容差默认值是 `0.05`，但 `level12` 中实际序列化实例（path ID 201）为 **0.03**，最大局部容差为 **0.25**。移植使用运行场景中的值。

已定位的 shader：`Unlit/BlockSprite`（37）、`Unlit/ActiveBlock`（38）、`Unlit/DisabledBlock`（39）、`Unlit/BlockCompose`（40）、`Unlit/ReadyBlock`（36）、`Unlit/SubtractBlockBlender`（35），以及 `EdgeMask`、`GlowMask`、`TouchEffect`。括号内是 sharedassets12 的 path ID。

`active.glsl`、`disabled.glsl` 来自 APK GLES 程序，使用实际材质常量；`displace.png`、`spark.png`、`noise.png` 分别来自 path ID 30、17、15。适配包括 GLES 100、Phira 的纹理坐标、将触点数组改为独立 uniform，以及共用位移采样器，使总绑定数量不超过 miniquad 的 12 个槽位。

这些导出资源来源于用户提供的 Phigros 安装包，不代表其版权或许可改为仓库的 GPL。

## 数据和判定

- 四个状态时间以及动画时间都直接使用秒，不进行判定线 BPM 换算。
- 事件首帧之前使用默认几何；区间使用**起始关键帧**的缓动和锚点。这一点以 APK 为准，与[格式文档](https://teamflos.github.io/phira-docs/chart-standard/chart-format/phi/blockArea.html)的部分叙述不同。
- 缓动 1–12 是二到五次幂，而非名称所暗示的正弦；按原版的 101 点查表再线性插值。
- 保留越界坐标、反向对角坐标、独立 X/Y 缓动、累计锚点变化，以及暂停/回退后重新求值。
- 只在激活期屏蔽触点。普通区域取并集，减算区域按奇偶数翻转；单独一个减算区域也会阻挡。
- 判定同时要求原始区域与经过容差处理的区域为实心，避免减算边缘意外阻挡。
- 一根手指一旦触发屏蔽，即使移出区域或区域消失，也必须抬手后才能恢复。其他未屏蔽手指正常工作。
- 同一过滤结果用于 Tap/Hold 头、Flick、Drag 和 Hold 续接；Hold 仍使用 Phira 的尾判、断触容忍时间和结算规则，不凭噪域直接赠送判定。
- 自动演奏保持原有行为。键盘没有屏幕触点坐标，仍按 Phira 原有键盘规则处理。
- 重试、练习模式重置会清除屏蔽 ID；不会修改旧谱面的 `blockAreaList` 缺省行为。
- 旧 PBC 格式无法保存噪域；含噪域的谱面转换时会明确报错，请保留 Phigros JSON，避免静默丢失判定数据。

## 渲染适配范围

激活/停用的主体颜色、位移、颗粒、背景像素化和触摸噪声使用导出的程序。
Unity 摄像机/CommandBuffer 合成被改为 Phira FBO 通道；判定几何和渲染几何共用同一个实现。
输入判定基于几何，不读取随机位移后的 GPU 像素。

以下部分是引擎适配，尚不能声称与原版逐像素一致：

- 遮罩使用普通区域并集与减算 XOR；没有逐指令复刻 Unity 的归一化格式累加/阈值预处理，特别是未激活区域的重叠渐显。
- 边缘/辉光使用合并的八圈扩张 pass，触摸轮廓使用圆形掩码；没有逐帧复刻 Unity prefab 的触摸出现/消失协程。
- 渐显使用谱面时钟而非 Unity 协程时钟，变速/跳转时保证确定性。
- 中间缓冲长边限制为 1280，减小移动设备显存占用；最终场景保持原分辨率。
- APK 的触摸音频低通滤波尚未移植；本次判定干扰是输入屏蔽。

## 复现分析

从仓库根目录执行（不会提取歌曲、谱面等无关内容）：

```sh
python -m pip install UnityPy==1.25.4
python scripts/extract-block-area.py /path/to/Phigros4.0.1.apk /tmp/block-analysis --native
python scripts/port-block-shaders.py /tmp/block-analysis
```

`--native` 可选，额外导出 IL2CPP 与 metadata，供 Il2CppDumper 使用。
本机更完整的反汇编、材质 JSON、原始 GLES 程序在仓库旁的 `analysis` 目录中；不属于构建输入。

## 验证

```sh
cargo check --locked -p prpr --no-default-features
cargo test --locked -p prpr --no-default-features --test block_area
cargo clippy --locked -p prpr --all-targets --no-default-features -- -D warnings
cargo run --locked -p prpr --no-default-features --example block_area_gpu -- /tmp/block-preview
```

GPU 示例使用生产渲染模块，分别输出隐藏、停用、预备、激活、触摸、镜像和留边图，并检查 OpenGL 错误。
已在 Windows 实际 OpenGL 4.6 驱动运行，15 项 CPU 测试通过，核心模块 Clippy 无警告；主程序无默认特性的编译检查通过。
尚未进行 iOS 真机、不同厂商移动 GPU 或与 Phigros 逐帧画面对照。

## iOS workflow

`.github/workflows/ios.yml` 支持手动触发及 main/master 推送、相关 PR。仓库根目录应是含 `Cargo.toml` 的本目录，而不是外层存放 APK 的工作目录。

流程在 macOS 安装项目指定 nightly 和 `aarch64-apple-ios`，直接调用现有 Xcode 工程，保留视频功能；噪域测试可按上面的命令在本地运行。
缺失的字体、背景及内置资源从官方 Phira 0.8.2 发布包恢复，并校验固定 SHA-256；不会覆盖已有资源。
补充了受版本管理的 `xcode/Info.plist` 与 `xcode/build-rust.sh`，消除了对本机忽略文件 `phira.app/Info.plist` 的依赖。

成功后在 Actions 的 Artifacts 下载 `Phira-iOS-unsigned-<run_number>`，内含 `Phira-blockArea-unsigned.ipa`。
这是**未签名 IPA**，安装前需要自行签名；workflow 不需要 Apple 证书或 secrets。
用户提供的 2026-10-03 Actions 日志已确认 Xcode `BUILD SUCCEEDED`，随后打包前的 `lipo` 检查因参数顺序错误退出；现已修正为输入文件在前、`-verify_arch arm64` 在后。修正后的 IPA 打包及上传仍需重新运行 Actions 验证。
