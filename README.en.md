<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="Dual Investment Assistant icon" />
  <h1>Dual Investment Assistant</h1>
  <p><strong>Run BTC/USDT Dual Investment strategies by selling options directly on Binance.</strong></p>
  <p>Choose a contract and confirm your order. The app tracks short positions and trades spot based on exercise results at expiry.</p>
  <p>Binance · BTC/USDT · 13 interface languages · <a href="LICENSE">AGPL-3.0-only</a></p>
  <p><a href="README.md">简体中文</a> · <strong>English</strong></p>
  <p>
    <a href="#dual-investment-vs-selling-puts">Compare returns</a> ·
    <a href="#quick-start">Quick start</a> ·
    <a href="#workflow">Workflow</a> ·
    <a href="#documentation">Documentation</a> ·
    <a href="#development">Contribute</a>
  </p>
</div>

![Buy low screen showing BTC put quotes, with the 62,500 USDT strike selected and its order panel on the right](docs/images/workbench-en.webp)

## Dual Investment vs. selling puts

We compared 66 pairs of Binance quotes on **16 November 2026**, each with the same strike and expiry. Selling puts directly offered **33.44% more gross income at the median**, with a **median annualized difference of 12.41 percentage points** before costs.

Comparisons use the same notional principal, **exclude trading costs and capital buffers, and assume no exercise at expiry**. The results reflect quotes at that time. Actual fills and later prices may differ; the figures do not establish a win rate or predict future returns.

![All 66 pairs by expiry: median gross income uplift 33.44%; median gross annualized gap 12.41 percentage points](docs/images/yield-overview-en.svg)

See the [group statistics, sources and calculations](docs/yield-comparison.md), or [all 66 pairs individually](docs/yield-comparison.md#逐笔报价). Detailed notes are in Chinese.

<details>
<summary>Largest annualized difference before costs: +84.12 percentage points</summary>

![Maximum sample: 39.19 USDT Dual Investment reward versus 54.00 USDT gross put premium; annualized gap 84.12 percentage points](docs/images/yield-comparison-en.svg)

This is `BTC-261117-64250-P`. See the [calculation](docs/yield-comparison.md#最高样本怎样算) for its terms and amounts.

</details>

## Features

Use the left sidebar to switch pages. Settings and the Binance connection status are at the bottom left.

| Page | What you can do |
| --- | --- |
| Overview | Check total asset value, available margin and today’s locally recorded sell premiums. The event calendar shows positions, expiries, deliveries and recovery records by date. |
| Dual investment | Sell puts to buy low or calls to sell high. Filter contracts by expiry, annualized estimate, exercise probability and distance from spot. |
| Strategy orders | Inspect fills for locally recorded orders, delivery tasks and recovery records. |
| Positions & spot | View short net positions, asset valuations and upcoming expiries. Hover over a balance or focus it with Tab for a breakdown by account. |
| Returns | View total sell premiums recorded on this device, today’s premiums, the put/call breakdown and orders with fills. |
| Settings | Connect to Binance, change the interface language and view local logs. |

> [!IMPORTANT]
> **The app places live trades. Use a dedicated account.** It sends spot orders in the background to complete delivery tasks. Do not complete the same delivery manually elsewhere, as the app may execute it again. Positions first found after expiry will not trigger delivery. Read the [trading safety guide](docs/trading-safety.md).

## Quick start

### Installation and platform support

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

For a browser preview, run `pnpm dev` and open `http://127.0.0.1:1420`. Account queries, contract scans, local logs and order submission require the Tauri desktop client. See [local packaging](docs/development.md#桌面安装包) and [automated releases](docs/development.md#自动构建与发布). CI uses standard runners in public repositories only and skips private repositories.

## Workflow

If you're signing up for Binance, you're welcome to [use our referral link](https://www.bsmkweb.net/register?ref=ZEABG36I) or enter referral code `ZEABG36I` to receive the applicable trading fee discounts.

1. **Connect** — Enter credentials under **Settings → Binance connection**. Disable withdrawals; enable read, spot and options trading, Simple Earn and Universal Transfer permissions.
2. **Select a contract** — Choose **Buy low** or **Sell high** in **Dual investment**. Click a column heading to sort the candidates, select a contract and enter the quantity in contracts on the right.
3. **Review and submit** — Click **Review order** and check the full contract symbol, expiry, quantity, estimated premium and breakeven. Choose **Back to edit** to make changes or **Confirm order** to submit the live order.
4. **Follow delivery** — Keep the app running and connected. Check delivery tasks in **Strategy orders** and exceptions in the recovery center.

After Binance cash-settles the option, the app trades spot to replicate physical delivery. It handles short positions only. If a tracked position expires while the app is offline, processing resumes after reconnection. See the [trading safety guide](docs/trading-safety.md).

<details>
<summary>Account and return calculations</summary>

- Credentials are stored in the system keyring. Orders, net positions and delivery records are stored in a SQLite database bound to one Binance account. You can replace the API key for that account. The binding survives credential deletion. Startup attempts to connect with saved credentials.
- **Sync account** in **Strategy orders** queries orders, net positions and assets. It records delivery tasks locally for eligible positions as they expire. Failed position or valuation queries retain the last successful result. Each contract shows its net position.
- After delivery, funds may still be reserved while transfers or Earn subscriptions finish. Do not clear the database to resolve unfinished trades.
- Premium reporting counts fills from locally recorded sell orders. It excludes external orders and other account gains or losses.

| Metric | Calculation |
| --- | --- |
| Recorded premium | Average fill price × filled contracts, rounded to two decimals per order before summing |
| BTC amount | Contracts × BTC per contract (`unit`) |
| Put / Call breakeven | Strike minus / plus premium per BTC |
| Simple annualized estimate (%) | Premium ÷ (strike × BTC amount) × 365 ÷ days to expiry × 100 |

Fees are excluded. Today's premium uses the order creation date in the local time zone. The term starts at the candidate scan or order creation, rounding up to at least one day.

</details>

### View local logs

**Settings → Logs** shows up to 120 recent entries, newest first. The list refreshes every 3 seconds while Settings is visible. It pauses when you leave the page or hide the window and refreshes as soon as you return.

Drag selection is disabled for ordinary interface text. Editable fields support selection and paste. To copy a log entry, choose **Open today's log**, or use **Open log folder** to find another date. **Delete logs** asks for confirmation and removes the local log files. Orders and credentials are retained.

## Interface languages

简体中文 · English · 繁體中文 · Português (Brasil) · Español · Tiếng Việt · Türkçe · Bahasa Indonesia · 日本語 · 한국어 · ไทย · Esperanto · Français

Choose a language in **Settings → General settings**. By default, the app uses your system language, or English if it is not supported. All languages are included in the installer.

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
