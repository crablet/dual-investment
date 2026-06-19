<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="Dual Investment Assistant icon" />
  <h1>Dual Investment Assistant</h1>
  <p><strong>Manage BTC/USDT dual-investment-style strategies from your desktop.</strong></p>
  <p>Connect Binance options and spot: select contracts, confirm orders, track short positions, and automate delivery.</p>
  <p>Binance · BTC/USDT · 13 interface languages · <a href="LICENSE">AGPL-3.0-only</a></p>
  <p><a href="README.md">简体中文</a> · <strong>English</strong></p>
  <p>
    <a href="#real-quotes-measurable-differences">Compare returns</a> ·
    <a href="#quick-start">Quick start</a> ·
    <a href="#workflow">Workflow</a> ·
    <a href="#documentation">Documentation</a> ·
    <a href="#development">Contribute</a>
  </p>
</div>

![Strategy workbench: filter Put/Call contracts and review quantity and order estimates](docs/images/workbench-en.webp)

## Real quotes, measurable differences

A Binance quote capture on **16 November 2026** matched 66 pairs with the same strike and expiry. Selling puts directly showed a **median gross income uplift of 33.44%** and a **median gross annualized gap of 12.41 percentage points**.

Comparisons use the same notional principal, **exclude trading costs and capital buffers, and assume no exercise at expiry**. These are quote differences, not executed returns, a trading win rate, or a promise of future income.

![All 66 pairs by expiry: median gross income uplift 33.44%; median gross annualized gap 12.41 percentage points](docs/images/yield-overview-en.svg)

See the [group statistics, sources and calculations](docs/yield-comparison.md), or [all 66 pairs individually](docs/yield-comparison.md#逐笔报价). Detailed notes are in Chinese.

<details>
<summary>Maximum sample: +84.12 percentage points in gross annualized return</summary>

![Maximum sample: 39.19 USDT Dual Investment reward versus 54.00 USDT gross put premium; annualized gap 84.12 percentage points](docs/images/yield-comparison-en.svg)

This is `BTC-261117-64250-P`. See the [calculation](docs/yield-comparison.md#最高样本怎样算) for its terms and amounts.

</details>

## From contract selection to settlement

| Capability | What it does |
| --- | --- |
| Selection and orders | Sell puts to buy low or calls to sell high. Filter by expiry, annualized estimate, exercise probability and distance from spot, then confirm manually. |
| Positions and delivery | Synchronize short net positions and execute spot delivery for obligations registered before expiry. Review exceptions in the recovery center. |
| Premium tracking | View locally recorded and today's sell premiums, with Put/Call fill history. |
| Local storage | Keep credentials in the system keyring and records in an account-bound SQLite database. |

> [!IMPORTANT]
> **This is a live-trading client. Use a dedicated account.** Registered positions can trigger real spot orders in the background. Do not manually fulfill the same delivery elsewhere: the app may execute it again. Positions first discovered after expiry are not handled retrospectively. Read the [trading safety guide](docs/trading-safety.md).

## Quick start

### Install and check platform status

Download installers from [GitHub Releases](https://github.com/crablet/dual-investment/releases), or build from source if a matching release is unavailable.

| Platform | Installation | Validation status |
| --- | --- | --- |
| Linux x64 | Install `.deb` / `.rpm`. Requires WebKitGTK 4.1 and Secret Service / keyring. | Used on Linux; individual distributions need compatibility checks. |
| Windows 10 / 11 x64 | Run `-setup.exe`; it downloads WebView2 if needed. | Packaging configured; real-device validation pending. |
| macOS 13.3+, Apple Silicon / Intel | Open the Universal `.dmg` and drag the app to Applications. | Packaging configured; real-device validation pending. |

Windows installers are unsigned; macOS uses ad-hoc signing without notarization. See [platform setup](docs/development.md#平台环境) for first-run details.

### Run from source

Install **Node.js 24+, pnpm 12.9.1, Rust 1.95+** and the [platform dependencies](docs/development.md#平台环境), then run:

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

`pnpm dev` previews the UI only; trading requires the Tauri desktop client. See [local packaging](docs/development.md#桌面安装包) and [automated releases](docs/development.md#自动构建与发布). CI uses standard runners in public repositories only and skips private repositories.

## Workflow

If you're signing up for Binance, you're welcome to [use our referral link](https://www.bsmkweb.net/register?ref=ZEABG36I) or enter referral code `ZEABG36I` to receive the applicable trading fee discounts.

1. **Connect** — Enter credentials under **Settings → Binance connection**. Disable withdrawals; enable read, spot and options trading, Simple Earn and Universal Transfer permissions.
2. **Select** — Choose a contract and quantity in **Dual investment**.
3. **Confirm** — Review premium, breakeven and quantity before submitting the order.
4. **Follow delivery** — Keep the app running and connected. Check delivery tasks in **Strategy orders** and exceptions in the recovery center.

After Binance cash-settles the option, the app executes spot trades to replicate physical delivery. It handles short positions only; registered obligations can resume after an offline period spanning expiry. See the [trading safety guide](docs/trading-safety.md) for processing rules.

<details>
<summary>Account and return calculations</summary>

- Local data is bound to one Binance account. You can replace its API Key; deleting credentials does not remove the binding. Startup attempts to connect with saved credentials.
- Account sync only reads state and retains the previous result on failure. Holdings reflect net positions, not accumulated order history.
- A completed delivery task may still have a batch awaiting fund cleanup. Clearing the database does not resolve unfinished trades.
- Premium reporting counts the filled portion of locally recorded sell orders. It excludes external orders and is not total account profit.

| Metric | Calculation |
| --- | --- |
| Recorded premium | Average fill price × filled contracts, rounded to two decimals per order before summing |
| BTC amount | Contracts × BTC per contract (`unit`) |
| Put / Call breakeven | Strike minus / plus premium per BTC |
| Simple annualized estimate (%) | Premium ÷ (strike × BTC amount) × 365 ÷ days to expiry × 100 |

Fees are excluded. Today's premium uses the order creation date in the local time zone. The term starts at the candidate scan or order creation, rounding up to at least one day. View logs under **Settings → Logs**; deleting them does not remove orders or credentials.

</details>

## Use your preferred language

简体中文 · English · 繁體中文 · Português (Brasil) · Español · Tiếng Việt · Türkçe · Bahasa Indonesia · 日本語 · 한국어 · ไทย · Esperanto · Français

Choose a language in **Settings → General settings**. The app follows system preferences, falls back to English if needed, and ships with all language resources.

## Documentation

The detailed guides are in Chinese.

| Topic | Guide |
| --- | --- |
| Quote sources, statistics and calculations | [Return comparison](docs/yield-comparison.md) |
| Trading checks, reserved funds and recovery rules | [Trading safety](docs/trading-safety.md) |
| Environment, code entry points, localization, checks and releases | [Development](docs/development.md) |

## Development

Vue 3, TypeScript and Pinia; Tauri 2 and Rust; SQLite for local storage. Financial calculations use Rust Decimal.

Report problems through [Issues](https://github.com/crablet/dual-investment/issues) or contribute a PR. Include your system, reproduction steps and sanitized errors; omit credentials, signed requests and raw Binance responses. Read [AGENTS.md](AGENTS.md) and complete the [relevant checks](docs/development.md#验证与打包) before submitting changes.

## License

[AGPL-3.0-only](LICENSE).
