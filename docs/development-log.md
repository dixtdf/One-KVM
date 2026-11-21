# 开发记录

## 2026-10-28：建立多通道分支及第一阶段基础

用户授权继续使用现有构建仓库开发多通道平台，并要求保留开发文档以跟进后续上游。

已编码：

- 独立实验分支；稳定main与线上custom.2不切换。
- `platform-core`，无新增第三方依赖、无硬件或数据库副作用。
- 通道/资源模型、稳定选择器、离线和歧义失败、别名物理冲突检查。
- 按通道授权判定，查看/控制/配置分开，撤权后重新判定拒绝。
- 平台Actions检查，以及接入既有ARM64构建的域模型测试步骤。
- 架构边界与上游升级流程。

CI已验证：提交 `4b0a8ed13925c32cfa69d3c24139cb840f6991af`，
[Actions 40690101642](https://github.com/dixtdf/One-KVM/actions/runs/40690101642)
成功。8项域模型测试、10项已有配置兼容测试通过，全部现有补丁仍可应用于固定上游。
证据产物 `experimental-platform-evidence` ID 12816438302，ZIP SHA256：
`28117c10faeac5f813686ebec1b5080239215389fcde9b53aafbcbaa521bbd65`。
产物保留30天；开发文档和对应源码长期保存在Git中。本次没有运行ARM64全量编译，
也未进行多路硬件验收，不能用上述18项测试替代这些检查。

尚未实现：运行时设备发现适配、持久化/迁移、多用户认证、API/输入鉴权、工作进程、
原子资源租约、控制权锁、多路网页和独立RustDesk入口。不得将当前核心库称为成品。
未对树莓派、Mac或Unraid执行本阶段部署，也未修改现有USB直通。

下一阶段：先管理数据库和仅管理员资源发现/通道CRUD，再做单路兼容工作进程。

## 2026-10-28：管理服务和元数据持久化（验证中）

新增Rust管理服务crate `platform-service`，复用已固定上游锁文件中的直接依赖版本。
使用独立SQLite数据库及版本化事务建表，不打开线上one-kvm.db。
包括多用户、Argon2密码、仅存哈希的会话、逐请求鉴权、通道/资源CRUD、授权撤销和
不记录秘密的审计。管理员启动需本机stdin初始化，监听限制为127.0.0.1:18080。
配置修改与审计同事务提交；默认拒绝普通用户管理资源和授予权限。

管理HTTP接口当前只处理元数据，`authorize`返回runtime_available=false，不发送视频或
输入事件。身份选择器尚未接入真实设备I/O；生产路径适配器必须通过只读发现结果绑定。
集成测试覆盖跨通道权限、撤权/停用/改密、配置冲突、重启持久化、会话哈希、登录节流、
未来schema拒绝及初始化不可覆盖，待Actions编译后记录真实结果。

继续实现：只读sysfs/udev设备发现与管理员登记接口。设备选择器不暴露原始物理路径，
通过端口+型号+接口计算稳定键；当前设备节点变化不改变键，型号变化使键失效。
登记必须匹配唯一发现结果，不能直接提交任意宿主机路径。此阶段仍无实际设备读写。

首次管理服务CI发现sha2 0.11输出类型不实现LowerHex，已改为逐字节十六进制编码；
这是编译问题，未关闭校验或修改密码验证。依赖锁由Actions生成并核对下载产物SHA256，
现已加入仓库，后续只用--locked构建。

管理与发现版本 `962dea0a055bae455e67114edd04d43650a09e75` 已通过
[Actions 40695746590](https://github.com/dixtdf/One-KVM/actions/runs/40695746590)：
8项域模型、10项已有配置兼容、2项设备发现、8项管理API集成测试，共28项。
管理服务二进制也在测试构建中编译成功。此轮不是ARM64发布或硬件验收。
最新提交另补会话过期断言并将生成的依赖锁提交，需单独记录该提交CI结果。
源码/证据Artifact ID 12819272359，ZIP SHA256：
`4f6b10b2c12c9fabf88b5ae8c2fe43ec4d84086c78785c3fc31993d442931fb4`。

## 2026-10-28：旧配置迁移与运行准备

新增显式CLI `--data-dir DIRECTORY --import-legacy LEGACY_DB`。仅用于停止写入的
旧数据库副本；以read-only连接读取，VACUUM INTO生成私有恢复快照，目标写入在事务中
提交。目标必须为空，迁移标记使重复执行返回同一通道，默认禁用导入通道。
原账号密码哈希保留，RustDesk ID、Key和密码只存私有工作进程模板，不出现在通道响应。
发现MFA凭据时拒绝导入，避免认证降级；未实现MFA迁移前不得用于此类实例。
当前没有在生产数据库上执行此命令。

提交b467071已通过[Actions 40696739149](https://github.com/dixtdf/One-KVM/actions/runs/40696739149)，
包含3项迁移测试：原文件不变/重复导入/凭据保留、设备缺失或歧义无部分写入、MFA拒绝。
之前固定依赖与会话过期版本a32f7ff也通过Actions 40696225018。

继续编码工作进程配置计划与原子资源预留，保留编码和凭据、替换精确解析的视频与控制
路径、绑定回环监听并关闭继承的其他外部服务。此模块不启动进程。预留只能在确认工作
进程退出并释放输入后清除，不能在发送停止请求后立刻释放。
下一步需落实工作进程启动、OTG全局所有权保护及管理网关，当前仍不具备多路运行能力。

工作进程准备提交318b1f1通过[Actions 40697062000](https://github.com/dixtdf/One-KVM/actions/runs/40697062000)。
本次增加精确设备计划和原子预留测试；迁移、发现、管理API、域模型及现有配置兼容检查
一起通过。并未运行工作进程或部署，不能把该结果称为两路实机验收。

## 2026-10-28：OTG进程所有权与监管基础

补丁0006增加ONE_KVM_OTG_OWNERSHIP进程策略：none完全跳过网络恢复和OTG配置/退出清理，
尝试启用OTG功能则报错；owner及普通单路实例取得进程级文件锁，避免合作实例重复管理
同一全局gadget。未打补丁的旧线上实例不参与该锁，切换部署仍必须显式停旧服务。
锁只保护本程序正常入口，不构成针对root进程的安全沙箱。

新增Supervisor：指定可信程序及SHA256，启动前验证、私有目录检查、精确资源预留、
清理继承环境、显式设置OTG所有权；设备缺失仅停止所属进程。停止后等待进程退出，
资源进入恢复隔离态；必须由可信硬件适配器确认neutral HID报告及gadget状态恢复后
才能再次分配，进程退出本身不能证明按键已释放。监管模块尚未通过HTTP暴露。

提交329e91f通过[Actions 40697978407](https://github.com/dixtdf/One-KVM/actions/runs/40697978407)。
新增OTG测试直接提取实际补丁中的所有权方法，以临时锁路径验证none不取得锁、owner独占、
进程对象释放后可重新取得及非法策略拒绝。监管测试使用无硬件子进程验证设备归属、
停止等待、恢复隔离和程序摘要拒绝。现有迁移/管理/发现/配置检查也通过。
该证据不覆盖整套One-KVM ARM64编译、configfs实际调用或USB按键释放，后续必须验证。

待完成：worker私有数据库配置落地、硬件neutral恢复适配器、优雅退出、管理网关、
现有协议的跨通道权限/控制租约、多路网页、每通道RustDesk部署与双路硬件验收。
此轮没有部署任何实验程序或改变Type-C链路。

## 2026-10-28：私有工作数据库与管理网页

工作数据库模块为每通道创建上游兼容config/users表，使用随机内部账号和Argon2哈希，
账号每次准备时轮换，不复用平台用户口令；目录700、数据库600。仅在确认旧进程停止后
才能调用。此模块尚未接入进程启动API。工作配置不继承其他扩展自启动。

管理网页已接管理服务静态路由，可登录、列出获分配通道、管理员登记发现的硬件、创建
停用通道和普通用户、分配/撤销权限。登录令牌仅存JS内存，不放URL或持久存储；动态文本
用textContent/Option渲染。响应设置no-store、nosniff及CSP。UI明确标注无视频/控制入口，
未放置无效连接按钮。新页面尚需视觉与浏览器交互验收，路由测试不代替此验收。

b605d96通过Actions 40698803613；一致快照修正c5f3d47通过Actions 40699162887。
旧配置导入改为先生成SQLite快照再读取所有数据，避免各项读取看到不同版本；旧数据库
不写入，并限制可用的原账号哈希算法。

已启动完整ARM64构建，尚未把运行中状态写成成功：

- 工作程序：Actions 40699406609，提交358d53e，版本0.2.8+custom.3.platform.1。
- 独立管理服务：Actions 40699301440，提交7f0a1ed，ARM64原生runner。

管理DEB不注册/启用服务，不启动监听，不操作硬件。产物提供对应平台源码、开发文档、
provenance和SHA256；依赖libc6>=2.39，部署前必须核对目标库依赖。
工作程序构建还会测试管理服务和实际OTG所有权方法，并打包平台对应源码。
本轮没有对树莓派执行安装或试运行。

## 2026-10-28：视频信令网关与输入旁路隔离

ARM64管理服务Actions 40699301440已成功（提交7f0a1ed）。该产物早于视频网关改动，
不能作为网关ARM64已验证的证据。工作程序40699406609仍需跟进，且该构建不含补丁0007。

网关只允许显式的WebRTC offer/ICE/close和流状态请求，工作端固定127.0.0.1及管理器提供
的端口；不转发用户提供URL、任意路径、请求头、配置、终端或文件接口。内部工作账号登录
cookie只存管理器内存，不发给用户。短JSON传输使用HTTP/1.0关闭连接，限定超时/报文大小，
不跟随重定向、不支持流代理。后续视频媒体直接走WebRTC，不经此JSON传输。

每个视频会话绑定通道、用户和确切登录令牌哈希；offer删除用户传入client_id，不能要求
复用任意上游会话。ICE和close再次校验权限及所有权。管理器定期扫描过期、退出、停用或
被撤权的会话并请求worker关闭；失败保留记录重试。该机制尚未对接worker故障强制停止，
因此还不能声称在网络/进程失效时具有确定撤权时限。

补丁0007使ONE_KVM_PLATFORM_WORKER=1的WebRTC不直接绑定HID控制器，避免仅查看用户
通过数据通道绕过管理网关发送按键。输入将走独立的逐事件权限检查和控制租约。
监管器显式设置该标记；手动直接运行旧单路仍保留既有行为。

1909770通过[Actions 40700589225](https://github.com/dixtdf/One-KVM/actions/runs/40700589225)。
测试用本机模拟worker HTTP服务确认cookie不泄漏、拒绝重定向、去掉client_id、同用户
另一登录不能接管、未授权通道拒绝、撤权后请求关闭。首次构建的局部Result别名冲突已修正。
接口未连接真实worker、网页未开放连接，未进行端到端浏览器或实机媒体验收。

## 2026-10-28：输入网关与控制权租约

补丁0008新增仅平台worker模式启用的内部/api/platform/input，仍受上游登录中间件保护，
复用现有parse_hid_message和HidController；空数组请求reset，其他报文限制最多7字节。
未改变普通单路WebSocket输入行为，内部接口不接受任意命令。

管理网关新增控制权申请、输入、释放接口。每个租约绑定通道、用户和确切登录令牌，
有效期15秒，每次有效输入续期；观看者不可申请，管理员也不能直接抢占已有租约。
申请先请求复位；每次输入重新检查通道Control权限和租约。撤权、账号停用、会话到期
或租约到期后扫描请求复位。失败保留租约阻止其他人接管，后续需连接进程隔离恢复。
目前复位成功只表示上游reset请求被接受，不构成实际USB neutral报告已发送的证据。
RustDesk输入还未与此控制租约共享，不能声称网页/RustDesk竞争已解决。

74e5b28通过[Actions 40701312068](https://github.com/dixtdf/One-KVM/actions/runs/40701312068)。
HTTP集成测试实际检查worker收到的请求顺序为reset、合法按键、撤权后reset；
无租约输入、viewer申请、另一个持有者申请、坏报文、撤权后按键均未转发。
租约单元测试覆盖到期及跨登录/通道拒绝。测试没有向真实设备发送按键。
0007/0008仍需完整One-KVM ARM64编译和实机验证；之前40699406609不含这两个补丁。

下一步将把网关与监管器实际连接，完善每通道运行配置/凭据管理、热插拔监控及失败恢复，
再开放网页观看与控制并部署第二路USB硬件进行验收。当前生产程序仍未替换。

## 2026-10-28：运行联动及每通道私有配置

新增Host将资源解析、私有工作数据库、摘要校验启动、随机内部账号登录和网关挂接串联。
管理器默认仍不启动worker；显式CLI `--data-dir DIRECTORY --worker ABS_PROGRAM SHA256`
启用该能力。只能由本机启动参数指定程序，HTTP不能提供程序或任意参数。
管理员可POST /channels/{id}/runtime/start或stop；未启用runtime返回503。
启动等待真实认证成功，失败停止并进入恢复隔离。设备消失只停止所属进程，网关删除离线
worker映射。配置编辑/删除运行中通道被拒绝；启动前仍需验证相应candidate完整补丁。

停止流程先请求确认HID复位，再SIGTERM并等待进程退出；失败不提前释放物理资源。
补丁0008增加reset_confirmed，通过队列oneshot等待后端reset实际执行并返回结果，超时
或失败时接口拒绝成功。这个后端确认也不代表目标电脑已实测接收到neutral报告。
53a4e84通过Actions 40702583936；尚未做真实worker启动/停止集成验收。

新增/runtime/config GET/PATCH：通道配置权限检查、采样宽高帧率、RustDesk启用、服务器、
自定义Key/设备密码。GET只返回key_set/password_set，不返回秘密。Key原样保存、空值
清空；密码留空或省略保留，不足6字节拒绝；更新拒绝未知字段和运行中修改。新通道模板
不复制其他通道的身份私钥，导入通道保持原私钥。独立用户和采集源绑定仍由管理员控制。

工作程序首个完整ARM64构建40699406609成功（358d53e，platform.1，仅至补丁0006）。
最新候选platform.2包含0007/0008及以上运行联动，已重新派发完整构建；不能拿前一个
成功记录替代最新补丁的编译和硬件证据。管理器ARM64也重新构建，同样尚未部署。

## 2026-10-28：网页通道控制台

网页不再只有管理元数据：读取受授权runtime状态，在线通道可发起独立WebRTC观看，
申请/续期/释放控制租约后通过HTTP输入网关发送键鼠。没有创建WebRTC键鼠数据通道。
键盘只在视频元素取得焦点时处理，鼠标按画面contain有效区域映射0..32767，移动限频，
队列拥塞释放控制权；失焦、页面隐藏、刷新、关闭或退出时尝试释放及关闭会话。
页面关闭请求本身不可靠，后端租约到期和撤权扫描仍是恢复保障。

管理员可编辑采样/RustDesk设置、手动ID/密码/Key、显式清Key、启用/停用配置及启动/停止
通道。动态内容均用DOM文本写入；没有回显秘密。尚未实现完整设备重绑定编辑与用户停用
网页入口，后端已有对应部分API。通道经理的配置表单权限呈现仍需完善。

新增3项Node行为测试：异步控制申请不会在断开后恢复控制、释放时等待在途输入并跳过
排队输入、观看者不发送按键。24444df通过Actions 40704265201。
这些是生命周期模拟，不代替浏览器渲染、媒体或实际输入验收。

发现工作数据库重准备可能覆盖worker生成的UUID/签名私钥，已修复为保留通道本地身份，
新的显式设备密码仍可覆盖旧值。9c5044b通过Actions 40704498808（含新增重启身份测试）。
对应ARM64管理构建40704498736已进入产物上传；完整worker platform.2构建40703387385
仍在编译，后续继续检查同一run，不能因观测等待而重启构建。生产仍未替换。

下一步：核验产物SHA及目标依赖后，在目标机私有目录旁路运行管理器并做浏览器视觉/交互
检查；完成RustDesk与网页共享控制权、故障恢复和两路实机验收前不晋升稳定版。

## 2026-10-29：目标机旁路与设备发现实测

40703387385完整ARM64工作程序构建成功，核验提交c34ede7的补丁清单包括0001至0008，
因此platform.2确实包含确认reset和视频不直连HID的适配。ZIP Artifact 12824860018，
SHA256 `6f093e0b2d090e0d756c2e241305a34f947027f87ddd6c3126a0062f36deb079`；
提取程序SHA256 `20bd10d05cfbc7f108bc9a8e05616f3241e78f6614745bb7238b03658690a2da`。
未dpkg安装，不覆盖/usr/bin/one-kvm。

管理器40704498736产物ZIP SHA256
`c2030583a1ac5463364fb5b93dbe2b34be13758989b8206ac7bb17e5ddca4005`，
在目标机glibc2.41验证ldd无缺失。原数据库SQLite backup为私有副本，导入到独立目录
`/root/one-kvm-platform-eval-20261029/`，导入通道默认停用。旁路管理器只监听127.0.0.1:18080，
浏览器经临时本机SSH转发测试；没有开放新的局域网监听或替换稳定服务。

实机发现旧版枚举24项，错误包含编码器/ISP、元数据及等价USB别名。新增QUERYCAP只读
过滤（capture+streaming、排除codec/ISP流水线队列），USBv2/v3别名归一化；CSI原始节点
和C792-NV12按同一管线租约处理，有桥接时优先只展示桥接，避免重复声明同一采集管线。
2a7e784通过Actions 40729499284及ARM64构建40729499222；新管理器ZIP SHA256
`cc659edbc7d162265387c659efaa78bfc7a1bad1c872732c9c057a9e689509df`。
旁路升级后实机列表恰为USB HDMI采集、OTG、ttyUSB0控制及C792-NV12共4项。

浏览器实际登录、查看导入通道和设置、检查四项设备列表及退出均完成；Key/密码字段留空
且仅提示已设置，未回显秘密。API匿名访问401。截图及脱敏JSON只留本地安装记录目录。

显式启用旁路管理器的--worker，创建临时“USB 验证通道”，仅绑定USB采集与串口控制，
RustDesk暂不注册，原导入Mac通道仍停用，原One-KVM正常运行。真实worker启动/内部登录
成功，串口与OTG由不同进程持有。确认reset后SIGTERM停止及重新启动成功。
视频尚未解码：无观看者时上游streamer的配置切换状态显示device_busy，这个状态本身
不能证明USB被其他进程占用；fuser确认没有其他进程占用USB采集节点。

浏览器首个观看测试失败于等待ICE complete超时，offer尚未送到worker。a811d38改为短暂
收集本地候选并用鉴权Trickle ICE传后续候选，避免公网STUN不可达阻塞局域网连接。
该修复正在CI/ARM64构建，独立解码探针依赖安装也在运行；没有把在线/启动成功写成媒体
已验收。还需验证实际帧、控制输入归属、RustDesk共享控制租约、断线恢复和完整回退。

USB通道首次独立解码已完成：平台登录/鉴权网关→真实USB工作进程→WebRTC，900帧
1920x1080 H.264，33.38秒，首帧1.42秒，ICE completed/connection connected；
服务端streaming，采集YUYV，目标30FPS，观测28.776787FPS。原稳定CSI/OTG实例继续运行。
脱敏结果本地 `平台USB-WebRTC实测-20261029-脱敏.json`，未保存或公开远程画面像素。
此结果不证明浏览器新Trickle ICE修复已实测，也不证明USB控制归属和长时间并发性能。

a811d38通过Actions 40731384524及ARM64管理构建40731384437；产物12835416685的
ZIP SHA256为`9d33da226b2692c1e8c20f238b2939d0a545f923f8398d9897c7b871d13205d1`。
该产物正在下载，后续替换旁路实例复查浏览器；当前运行的旁路管理器仍为2a7e784。
临时SSH转发只绑定本机18180，旁路管理器18080仍只绑定目标机回环地址。

## 2026-11-20：bcm2835 V4L2 M2M NV12 色度损坏修复并实机部署

现象：直连C792显示器正常，网页RGB画面绿屏/偏色。采集源 `/dev/video0` 与
C792 bridge 输出的 `/dev/video42` NV12 单帧统计正常，说明问题在编码阶段。

实测证据（Pi 4B, Debian 13, FFmpeg 7.1.5）：

- `h264_v4l2m2m` 输入 NV12：解码帧 RGB 均值约 `(78.6, 196.3, 97.1)`，
  U均值98.2、U标准差55.4，与源图 `(133.7, 143.7, 199.5)` 明显不符。
- 同一编码器改喂 `yuv420p`（YU12）：RGB均值 `(133.7, 143.7, 199.5)`，
  与源图逐项一致；软件 libx264 同样正确。
- testsrc2 合成图在720p/1080p/1088p下都复现NV12损坏，排除分辨率对齐问题。
- 把NV12字节交换为NV21仍不正确，排除简单的U/V平面反转解释。
- 设备上 `bcm2835-codec` 仅提供 H.264 硬件编码，HEVC 只有解码，
  故修复限定在 H.264 v4l2m2m。

补丁 `0009-v4l2m2m-yuv420p-input.patch`：

- `video/codec/mod.rs` 新增 `requires_planar_input`，仅当编码器名含
  `v4l2m2m` 且 `/proc/device-tree/model` 为 Raspberry Pi 时返回真，
  其它平台的 v4l2m2m 仍保留原有NV12零拷贝路径。
- `video/codec/h264.rs` 在该条件下把编码器输入从 NV12/NV21 切换为
  YUV420P，并记录切换日志。
- `video/pipeline/encoder_state.rs` 令流水线在对应条件下执行
  NV12→YUV420P 转换，避免直接把半平面NV12字节交给编码器。

验证：`cargo check` 通过（仅有一个与本次改动无关的既有deprecated警告）。
在本机临时以官方FFmpeg构建的ARM64 release二进制上，隔离数据目录与RTSP
输出抓帧的像素统计与源帧一致；随后将包含补丁0001–0009的完整二进制部署到
10.10.10.18，生产实例恢复硬件编码后实测RGB `(133.7, 143.8, 199.6)`、
U 157.6、V 118.5，U/V标准差28.6/12.3，与源帧一致；日志显示
`switching input Nv12 -> Yuv420p` 与 `Using NV12->YUV420P converter`。
自定义ID/Key接口与前端override仍在部署后正常。

部署与回退：备份在 `/root/one-kvm-video-fix-20261120/`，含上一版二进制、
升级前数据库、已部署的修复二进制及 `rollback.sh`。网页仍可随时在设置中把
编码器切换为 Software 作为不依赖二进制的兜底。

注意：本机验证构建以桩替代Rockchip RKMPP C++符号并使用固定版本libyuv源码，
因为 Pi 上官方FFmpeg不含RKMPP头；官方CI的Docker构建仍走完整RKMPP路径。
该部署不能替代CI产物与长期稳定性观察。

## 2026-11-21：移除 Web 登录边界并默认使用端口 80

按用户要求增加补丁 `0010-remove-web-login-port-80.patch`，版本
`0.2.8+custom.3.platform.3`。该补丁仅面向受信任局域网，不适用于公网部署。

- 删除全部 Web `/api/auth/*` 路由和 `/api/config/auth`，移除 API 路由上的
  `auth_middleware`。首次设置不再要求或创建 Web 用户名/密码，仍保留视频、HID、
  扩展三步硬件配置。
- 删除网页登录页、认证 store、TOTP 设置卡片及相应 API 调用。进入页面直接读取
  控制台和设置。
- Web UI、视频、HID、ATX、MSD、终端和配置接口均无 Web 身份边界。Redfish、
  RTSP、VNC 的独立认证不回退；用户、会话、TOTP 内部数据模块保留但 Web 不再使用。
- `WebConfig` 默认 HTTP 端口由 `8080` 改为 `80`。加载旧持久化配置时，仅将
  精确的 `8080` 迁移为 `80` 并写回；显式配置的其他端口保持不变。
- 增加后端测试：默认端口为 80、旧 8080 迁移、显式 9000 保持、SetupRequest
  不要求账户字段。前端 `vue-tsc` 与 `vite build` 已通过（2777 modules）。
- 本机没有 Rust 工具链，后端编译和 ARM64 DEB 仍以固定上游的 Actions 构建为准。
  当前尚未把“前端构建通过”写成后端或实机验证成功。

### 构建、部署与 OTG 实测结论

- Actions `44887225781`（提交 `cd066aa`）成功。ZIP SHA256
  `650996f9c893df3ebec779921f6071acedead5ba0dad8719040b92dbc182e4e5`；
  ARM64 DEB `one-kvm_0.2.8+custom.3.platform.3_arm64.deb` SHA256
  `d7cf01b1039e67988c38ffb27235921b1570b739156bffe100d320904997b1cc`。
- 已安装到 `10.10.10.18`。升级前备份在 `/root/one-kvm-web-noauth-20261121/`，
  含数据库备份、旧二进制、旧 drop-in、新 DEB、SHA256 清单和 `rollback.sh`。
  旧中文前端认证 override `30-manual-id-frontend.conf` 已移到备份目录并禁用。
- 只重启了 `one-kvm` 服务，没有重启 Pi。服务监听 `0.0.0.0:80`，原 `8080`
  已关闭；数据库中的 `web.http_port` 已迁移为 `80`。无 Cookie 请求
  `/`、`/api/info`、`/api/hid/status`、`/api/hid/otg/self-check`、
  `/api/config` 均返回 200；`/api/auth/login` 和 `/api/config/auth` 返回 404。
- `10.10.10.18` 的 OTG 自检为 `overall_ok=true`、`bound_udc=fe980000.usb`、
  `udc_state=configured`、`udc_speed=high-speed`；`/api/hid/status` 为
  `available=true/initialized=true/online=true`，`/dev/hidg0` 至 `/dev/hidg2`
  由服务进程持有。
- `10.10.10.2` 主机侧已枚举 `1d6b:0104 One-KVM USB Device`，包含键盘、相对鼠标、
  绝对鼠标三个 HID 接口；QEMU 命令行也保留 `usb-host` 的 `hostdev2` 映射。
  但这只能证明 Linux 主机侧和 QEMU 配置存在，不能证明 guest 内输入生效。
- guest 侧历史日志证明部署前 USB 直通曾经生效：15:59:50 macOS 识别
  `One-KVM USB Device`，`VendorID=7531`、`ProductID=260`，并创建键盘
  `usage 6` 和绝对鼠标 `usage 2`。17:14:58 服务重启造成主机 USB 断开时，guest
  同时记录 `hardware connection lost` 并移除该设备，之后没有再枚举。
- 当前 guest 直通是陈旧的：主机设备号已从 126 重新枚举为 3，QEMU 监控仍保存
  `/dev/bus/usb/001/126`，日志出现 `libusb_release_interface: -4 [NO_DEVICE]`；
  QGA 只读检查的 IORegistry 只剩 QEMU xHCI 控制器，没有 `IOUSBHostDevice`。
  因此能确认部署前 OTG 在本目标上工作过，但不能声称当前 guest 输入仍可用。
- 本次没有向 guest 发送任何键鼠、ATX、MSD 或配置写入，也没有重启或修改 `.2`/VM。
  要恢复当前直通，需要在 `.2` 上重新 hotplug/detach-attach `hostdev2`，或重启
  VM 让其重新按当前设备号打开；这些操作超出本次只读授权。仅重启 `.18` 只会再次
  重枚举 USB，不能让已经运行且持有旧设备路径的 QEMU 自动重新接回，因此不能作为
  当前故障的修复或验收方案。
- 登录移除与端口 80 必须使用新二进制/DEB，旧数据库或重启 Pi 都不会产生该行为。
  OTG 的长期稳定性仍与“服务重启会重建 gadget”有关；若后续要求避免每次升级都让
  guest 丢失 USB 绑定，需要另做“复用已存在 gadget/减少重枚举”或带退避的恢复补丁，
  并单独实机验证，不能把一次服务重启后的 `configured` 当作根因已修复。
## 2026-11-21：保留已绑定 OTG gadget 并固化 ARM64 构建

补丁 `0011-preserve-otg-gadget.patch` 针对服务重启造成 USB 物理重枚举、
进而使 `10.10.10.2` 上运行中的 QEMU 仍指向旧 `/dev/bus/usb/001/126` 的问题：

- `OtgGadgetManager` 在绑定前记录期望 UDC，并校验已有 gadget 的
  `idVendor/idProduct/bcdDevice`、字符串描述符、全部函数目录及配置链接。
- 校验通过且 UDC 匹配时直接接管已有 gadget，不重写 UDC、不删除并重建
  ConfigFS，避免 USB 设备号变化。
- descriptor、函数、链接或 UDC 不匹配时仍执行原有清理并重新创建，防止把
  不可用或配置冲突的旧 gadget误当作健康设备。
- `OtgService::shutdown` 改为 `relinquish`，只停止进程所有权，保留 UDC 绑定
  和 ConfigFS 状态。
- 新增文件系统级测试：匹配已有 gadget 的接管、descriptor/函数不匹配检测、
  `relinquish` 后 UDC 仍保持。

`pins.json` 构建版本提升为 `0.2.8+custom.3.platform.4`，补丁由
`scripts/prepare.py` 按文件名顺序自动应用并进入 ARM64 DEB。

## 2026-11-21：修复 0011 编译错误并实机验证 OTG 保留生效

`0011` 首轮 CI（run `44909887928`）在“Build ARM64 using pinned Debian 12 image”
步骤失败。CI job 日志需管理员权限无法读取，改用在 `10.10.10.18` 上对补丁后的
ARM64 源码做原生 `cargo check` 复现，定位并修复了两处补丁代码错误：

- `manager.rs` 中 `read_attribute` 是 `impl` 关联函数，闭包内必须以
  `Self::read_attribute` 调用，原先裸函数名导致 `E0425`。
- 测试辅助 `prepare_matching_gadget` 中 `fs::create_dir_all(manager.config_path)`
  在 `&mut` 引用上会移动 `PathBuf`，改为传 `&manager.config_path`，消除 `E0507`。

修复后 `cargo check` 与 `cargo check --tests` 均通过（仅剩与补丁无关的既有警告），
补丁以提交 `aa270db` 合入 `main`。

CI 重新构建 run `44924415304`（提交 `aa270db`）**全部 15 个步骤 success**，
包含此前失败的步骤 12 Build ARM64，步骤 13 打包 DEB 成功。产物：
`one-kvm_0.2.8+custom.3.platform.4_arm64.deb`，
SHA256 `ef218bb4b9b91432f1d3eefedf54726a8c3391404d2e667c5e0f519f37da95a6`。

部署到 `10.10.10.18`（仅重启 `one-kvm` 服务，未重启 Pi）后实机验证 OTG 保留：

- 服务监听 `0.0.0.0:80`，`/api/info` 返回 200，版本升级为
  `0.2.8+custom.3.platform.4 arm64`。
- 使用 `platform.4` 二进制重启服务后，日志确认保留行为生效：
  - 关闭时：`Released OTG gadget ownership without unbinding or deleting ConfigFS state`
    （`OtgService::shutdown` 走 `relinquish`）。
  - 启动时：`Adopting an existing bound OTG gadget without re-enumeration`。
- UDC 仍为 `fe980000.usb`，state `configured`，`/dev/hidg0`、`/dev/hidg1`、
  `/dev/hidg2` 存在并由服务持有，服务重启未触发 USB 物理重枚举。

说明：从 `platform.3` 升级到 `platform.4` 的那次重启，由旧二进制执行关闭，
仍走旧 `cleanup()` 路径（部署日志中可见 `Cleaning up`/`Unbound gadget`），
因此那一次会重建 gadget；这是新旧二进制替换时的一次性行为。`platform.4`
自身运行后的重启已实测为“保留 + 接管”，即本补丁目标行为成立。

`10.10.10.2` 的 guest 侧 USB 直通恢复仍需在用户对该主机额外授权后执行
USB hostdev 重新挂载或 VM 重启，本阶段未对 `.2` 做任何写操作。
