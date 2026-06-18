<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="双币赢助手图标" />
  <h1>双币赢助手</h1>
  <p><strong>在桌面上管理 BTC/USDT 双币赢式策略。</strong></p>
  <p>连接 Binance 期权与现货，把候选筛选、下单复核、卖方净仓和自动交割集中在一个工作台。</p>
  <p>Binance · BTC/USDT · 13 种界面语言 · <a href="LICENSE">AGPL-3.0-only</a></p>
  <p><strong>简体中文</strong> · <a href="README.en.md">English</a></p>
  <p>
    <a href="#快速开始">快速开始</a> ·
    <a href="#工作流程">工作流程</a> ·
    <a href="#文档">使用文档</a> ·
    <a href="#开发">参与开发</a>
  </p>
</div>

![双币赢工作台：左侧筛选 Put/Call 候选，右侧复核数量与订单预估](docs/images/workbench-zh-CN.webp)

<p align="center">双币赢工作台 · 候选筛选与下单预估</p>

## 从筛选到结算

双币赢助手面向使用专用 Binance 账户执行 BTC/USDT 卖方期权策略的用户。你选择合约、确认新订单；客户端自动同步卖方净仓，为到期前已登记的义务继续处理交割。

| 能力 | 你可以做什么 |
| --- | --- |
| 筛选候选 | 在卖 Put 低买与卖 Call 高卖之间切换，按到期日、年化、行权概率和距现价筛选合约。 |
| 下单复核 | 查看权利金与保本价预估，确认合约和数量后提交；已有活跃仓位的合约会显示持仓标识。 |
| 净仓跟踪 | 连接后自动同步 BTC/USDT 卖方净仓；总览、持仓标识和到期日历按合约展示，订单历史单独保存。 |
| 自动交割 | 为到期前已登记的义务核对行权事实，执行所需现货交易与资金收尾；在交割任务和恢复中心查看进度。 |
| 权利金统计 | 查看本机已记录及今日卖出权利金，按卖 Put / Call 分类查看成交记录。 |
| 本地保存 | 凭据存入系统 keyring，本地 SQLite 按账户绑定并保存订单、净仓、交割任务和恢复记录。 |
| 多语言界面 | 支持 13 种语言，跟随系统或手动选择；切换时保留当前表单与交易状态。 |

> [!IMPORTANT]
> **这是实盘客户端，请使用专用账户。** 连接后自动跟踪 BTC/USDT 卖方净仓，不包含买方仓位；到期前已登记的义务可由后台自动发送真实现货订单，模拟实物交割。新期权订单仍须手动确认。请勿在其他终端手工补齐同一交割义务，以免重复执行。使用前阅读[实盘交易安全](docs/trading-safety.md)。

## 快速开始

### 安装与平台状态

安装包通过 [GitHub Releases](https://github.com/crablet/dual-investment/releases) 分发；如果尚无对应版本，请按下方说明从源码运行。

| 平台 | 安装包与运行方式 | 验证状态 |
| --- | --- | --- |
| Linux x64 | 用系统包管理器安装 `.deb` / `.rpm`；需要 WebKitGTK 4.1 与 Secret Service / keyring。 | 已在 Linux 开发、使用；各发行版兼容性仍需分别验证。 |
| Windows 10 / 11 x64 | 运行 `-setup.exe` 安装；缺少 WebView2 时安装器会联网下载。 | 已有平台实现与打包配置，尚待 CI 构建及实机验证。 |
| macOS 13.3+，Apple Silicon / Intel | 打开通用版 `.dmg`，将应用拖到“应用程序”后运行。 | 已有平台实现与打包配置，尚待 CI 构建及实机验证。 |

Windows 安装包未做发布者签名；macOS 使用免费的 ad-hoc 签名，未做 Apple 公证，首次打开可能需要系统确认。详见[平台运行说明](docs/development.md#平台环境)。

### 从源码运行

三个平台使用相同命令。准备 **Node.js 24+、pnpm 12.9.1、Rust 1.95+**，并安装对应的[平台构建依赖](docs/development.md#平台环境)。Windows 使用 MSVC 工具链；macOS 需要 Xcode Command Line Tools。

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

在当前系统构建安装包：

```bash
pnpm tauri:build
```

默认产物位于 `src-tauri/target/release/bundle/`，Tauri 自动选择当前系统的打包配置。交易功能需要 Tauri 桌面客户端；`pnpm dev` 仅用于浏览器界面预览。

仓库还提供[自动构建流水线](.github/workflows/build-desktop.yml)：推送版本标签后生成三平台安装包并上传到 Releases 草稿，审核后手动发布。为保持免费，流水线仅在公开仓库使用标准 runner，私有仓库自动跳过。启用方式与费用边界见[发布指南](docs/releases.md)。

## 工作流程

1. **连接专用账户** — 在“设置 → Binance 连接”填写 API 凭据。关闭提现权限，并配置所需的读取、交易、Simple Earn 和 Universal Transfer 权限；连接后自动同步未到期 BTC/USDT 卖方净仓。
2. **选择策略** — 在“双币赢”选择低买或高卖，筛选到期日和候选合约，输入数量。
3. **复核并提交** — 查看权利金、保本价等预估，在复核窗口确认后发送真实订单。
4. **跟踪自动交割** — 在“总览”查看净仓，在“策略订单”分别查看本机订单历史与交割任务。保持客户端运行并连接，让后台处理已登记义务；异常进入“恢复中心”核对。

Binance 期权采用现金结算，客户端另行执行真实现货交易以模拟实物交割。离线跨过到期时刻后，已登记义务可继续恢复；到期后才首次发现的仓位或历史行权不会自动补做。页面上的年化与权利金预估不代表最终成交或收益承诺。完整说明见[使用指南](docs/usage.md)。

## 用你熟悉的语言

简体中文 · English · 繁體中文 · Português (Brasil) · Español · Tiếng Việt · Türkçe · Bahasa Indonesia · 日本語 · 한국어 · ไทย · Esperanto · Français

在“设置 → 通用设置”切换语言。默认跟随系统，未匹配到支持的语言时使用英语；语言资源随应用提供，切换无需联网下载。日期保持系统本地时区和公历。

## 文档

| 你想了解 | 从这里开始 |
| --- | --- |
| 连接、筛选、跟踪订单与查看收益 | [使用指南](docs/usage.md) |
| 交易授权、资金预留、待确认请求与恢复边界 | [实盘交易安全](docs/trading-safety.md) |
| 源码运行、构建、架构与多语言维护 | [开发指南](docs/development.md) |
| 自动生成安装包、免费构建约束与发布版本 | [发布指南](docs/releases.md) |
| 修改项目时需要遵循的约束 | [AGENTS.md](AGENTS.md) |

## 开发

前端使用 **Vue 3、TypeScript 和 Pinia**，桌面运行时使用 **Tauri 2**。交易编排与金融计算在 **Rust** 中完成，金额使用 **Decimal**，本地记录使用 **SQLite**。

欢迎通过 [Issues](https://github.com/crablet/dual-investment/issues) 反馈问题，或提交改进界面、翻译和文档的 PR。问题报告请附系统环境、复现步骤和脱敏后的错误信息；不要附 API 凭据、签名请求或账户原始响应。

开始修改前阅读 [AGENTS.md](AGENTS.md) 与[开发指南](docs/development.md)，并按[验证要求](docs/development.md#验证与打包)完成相关检查。涉及下单、资金或恢复的改动，还须先阅读[实盘交易安全](docs/trading-safety.md)。

## 许可证

项目自有内容采用 [AGPL-3.0-only](LICENSE)。
