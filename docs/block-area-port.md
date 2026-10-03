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
- 接受有限数值的空/反向时间区间，按原来的时间比较自然隐藏或不激活，不修正时间。原版「ハテ IN」索引 148 的 `appearTime=28.3`、`disappearTime=28.29932` 属于这种情况；不再因此拒绝整张谱面。
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

## 普通模式判定时间

从 4.0.1 ARM64 `JudgeControl::.cctor`（`0x1D74010`）核对的普通模式基础阈值为 Perfect 80 ms、Good 180 ms、Bad 220 ms。`ClickControl::Judge`（`0x1D82114`）与 `HoldControl::Judge`（`0x1D83B50`）使用音符与当前时间的绝对差比较 Perfect；未触发音符迟到超过 Good 时判 Miss。

Phira 原有触摸路径先从迟到误差中减去 `EARLY_OFFSET=70 ms`，使 Tap/Hold 头的有效 Perfect 范围达到提前 80 ms、迟到约 150 ms（1 倍速）；键盘路径原本没有该补偿。现已移除这段非对称补偿，Good 从 160 ms 调整为 180 ms，迟到超过 Good 不再进入 Bad。提前 Tap 的 Bad 上限仍为 220 ms。保留用户输入延迟校准和变速下的真实时间换算。

这次对齐普通模式时间窗口，不代表挑战模式、Flick 手势阈值、空间判定、Hold 续押或成绩认证与官方完全等价。新加入的时间回归覆盖迟到 81–150 ms 不再判为 Perfect、Good 180 ms 边界及变速换算；真实导出的全部 30 张新增/修改谱已通过解析，包括「ハテ IN」的 781 条噪域。

## 渲染适配范围

激活/停用的主体颜色、位移、颗粒、背景像素化和触摸噪声使用导出的程序。
Unity 摄像机/CommandBuffer 合成被改为 Phira FBO 通道；判定几何和渲染几何共用同一个实现。
输入判定基于几何，不读取随机位移后的 GPU 像素。

以下部分是引擎适配，尚不能声称与原版逐像素一致：

- 遮罩使用普通区域并集与减算 XOR；没有逐指令复刻 Unity 的归一化格式累加/阈值预处理，特别是未激活区域的重叠渐显。
- 边缘/辉光使用合并的八圈扩张 pass，触摸轮廓使用圆形掩码；没有逐帧复刻 Unity prefab 的触摸出现/消失协程。
- 渐显使用谱面时钟而非 Unity 协程时钟，变速/跳转时保证确定性。
- 按 APK `BlockRender.Start` 恢复遮罩为谱面视口的 1/8、EffectRT 为 1/4；遮罩用最近邻，辉光用线性采样，边缘按 EffectRT 像素中心采样。最终场景保持原分辨率。
- 触摸音频低通已接入谱面音乐，参数与触发规则见下节；Unity 内部 DSP 以双声道 biquad 适配，尚未做原版录音对照。

## 噪域音频低通

4.0.1 的 `level12` / `LevelControl`（path ID 195）序列化数据在 `0x1F8`、`0x1FC` 分别保存截止频率 **1500 Hz**、过渡时长 **0.1 秒**。`LevelControl.<Start>d__46.MoveNext` 在 `0x1D796CC` 读取截止频率并设置到音乐 AudioSource 上动态添加的 AudioLowPassFilter，随后禁用组件；`0x1D796F4` 将渐变时长交给 ProgressControl。

`JudgeControl.ProcessBlockedTouches` 根据本帧仍按住的被屏蔽手指决定低通开关。手指滑出噪域后仍保持屏蔽及低通，最后一根被屏蔽手指松开/取消才恢复。`ProgressControl.<LerpLowPassFilter>d__54.MoveNext` 从当前截止频率线性渐变到 1500 Hz，释放时渐变到 22000 Hz 后关闭滤波；中途反转从当前频率重新过渡。首次启用时组件原本已是 1500 Hz，因此没有从 22000 Hz 开始的首次扫频。

本地 `vendor/sasa` 基于原锁定版本 `e76229b2f68e4dc68b22bc375d2a92e8e7897691`，新增音乐专用二阶低通，Q=1（[Unity AudioLowPassFilter 默认参数](https://docs.unity3d.com/es/2018.3/Manual/class-AudioLowPassFilter.html)）。左右声道独立，按音频线程实际输出采样率计算系数，截止频率在低采样率设备上限制到 Nyquist 以下；过渡使用音频输出时间，不随谱面倍速变化。未启用及释放过渡结束后直接输出原始音乐样本，打击音效不经过这个滤波器。

控制端通过原子布尔值传递触摸状态，不向有界播放命令队列逐帧塞入控制命令；暂停和 seek 清空滤波状态，重建音乐默认旁路。自动演奏、预览、倒计时和暂停回退不触发低通。没有新增系统框架或原生依赖，iOS workflow 使用同一份本地音频库，无需额外安装步骤。

这是已核实触发规则及参数的移植，biquad 并非 Unity 内部 DSP 的逐指令复刻，音色仍需真机与原版对照。`cargo test --locked -p sasa --no-default-features` 的 5 项测试覆盖多采样率高频衰减、低频保留、声道隔离、默认/释放旁路、播放位置不变、暂停/seek 复位、渐变反转及采样率切换稳定性。

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
针对真机反馈的渲染问题，Unity `_Time` 改名为私有 `blockTime`，避免 Macroquad 每次绘制覆盖为 `(t, sin(t), cos(t), 0)`；场景复制改用独立全屏 pass，避免低分辨率遮罩 pass 留下的 OpenGL scissor 裁掉 `glBlitFramebuffer`。GPU 示例在 1600×900 校验跨时刻动画变化、回到同一谱面时间时像素一致，以及噪域外/留边区域像素不变。
已在 Windows 实际 OpenGL 4.6 驱动运行；本次 22 项 CPU 回归测试通过，全部 30 张新增/修改实谱解析通过，核心模块 Clippy 无警告；主程序无默认特性的编译检查此前已通过。
尚未进行 iOS 真机、不同厂商移动 GPU 或与 Phigros 逐帧画面对照。

## iOS workflow

`.github/workflows/ios.yml` 支持手动触发及 main/master 推送、相关 PR。仓库根目录应是含 `Cargo.toml` 的本目录，而不是外层存放 APK 的工作目录。

流程在 macOS 安装项目指定 nightly 和 `aarch64-apple-ios`，直接调用现有 Xcode 工程，保留视频功能；噪域测试可按上面的命令在本地运行。
缺失的字体、背景及内置资源从官方 Phira 0.8.2 发布包恢复，并校验固定 SHA-256；不会覆盖已有资源。
补充了受版本管理的 `xcode/Info.plist` 与 `xcode/build-rust.sh`，消除了对本机忽略文件 `phira.app/Info.plist` 的依赖。

成功后在 Actions 的 Artifacts 下载 `Phira-iOS-unsigned-<run_number>`，内含 `Phira-blockArea-unsigned.ipa`。
这是**未签名 IPA**，安装前需要自行签名；workflow 不需要 Apple 证书或 secrets。
后续构建保留 Rust release 调试信息，并上传 `Phira-iOS-symbols-<run_number>`（原始可执行文件、dSYM、UUID 和源码提交号）。分析 `.ips` 时必须使用同一次构建且 UUID 匹配的符号；重新构建的符号不能用于旧安装包。该改动用于定位真机崩溃，不代表已修复启动问题。

2026-10-03 的启动崩溃报告与用户提供 IPA 的 UUID 均为 `0e80915f-e4f7-3921-a8f2-a609d7105ae3`，包内运行资源完整。二进制反汇编确认报告中的 `+17410872` 返回地址位于 miniquad `draw_in_rect` 的 unwind 终止分支，调用 Rust 的 `panic_cannot_unwind`；这不是最初的 panic 位置。该包没有保留函数符号，报告也没有原始 panic 消息，尚不能由此确定根因。

iOS 入口现在在 Rust future 内捕获 panic，在失败时停止业务执行并显示错误页，同时通过 panic hook 将原始消息、源码位置、启动阶段和回溯追加到应用 `Documents/phira-crash.txt`。可在“文件”App 的 Phira 文件夹或文件共享中导出。后台线程的 panic 也会记录；原生异常、abort、图形框架在该 future 外的错误以及 panic 期间再次 panic 不能保证被捕获。

后续真机日志已定位原始错误：`TimeManager::resume` 在 `pause_time = None` 时执行 `unwrap()`。iOS 初次 `applicationDidBecomeActive` 可在未暂停过的情况下发送恢复通知，进入 `MainScene::resume` 后触发崩溃。修复包括：`Main` 忽略无状态变化的暂停/恢复通知，`TimeManager` 重复暂停保留首次暂停时间、未暂停时恢复无操作；主循环在更新前非阻塞处理生命周期消息，暂停期间返回帧循环，避免阻塞派发恢复通知的 iOS 主线程。新增 `cargo test --locked -p prpr --no-default-features --test time` 覆盖首次/重复恢复、重复暂停及暂停后重置。修复后的启动与前后台切换仍需真机确认。

用户提供的 2026-10-03 Actions 日志已确认 Xcode `BUILD SUCCEEDED`，随后打包前的 `lipo` 检查因参数顺序错误退出；现已修正为输入文件在前、`-verify_arch arm64` 在后。修正后的 IPA 打包及上传仍需重新运行 Actions 验证。
