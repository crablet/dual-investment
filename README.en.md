<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="Dual Investment Assistant icon" />
  <h1>Dual Investment Assistant</h1>
  <p><strong>Manage BTC/USDT dual-investment-style strategies from your desktop.</strong></p>
  <p>Work with Binance options and spot in one place: filter contracts, review orders, track short net positions, and automate delivery.</p>
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

![The strategy workbench, with contract filters on the left and order estimates on the right](docs/images/workbench-en.webp)

<p align="center">Strategy workbench · Contract selection and order estimates</p>

## Real quotes, measurable differences

Selling a put directly lets you collect the option buyer's premium for taking on a buy-low obligation. Dual Investment Assistant brings that route into a desktop workflow: compare quotes, confirm orders, and automate delivery for positions registered before expiry.

**Across all 66 pairs, the median gross income uplift is +33.44%, and the median gross annualized gap is +12.41 percentage points.** The capture covers eight expiries, from 1 to 74 days. All 66 gross income differences were positive. The annualized-gap interquartile range is **6.46–24.61 percentage points**, with a full range of **2.48–84.12 percentage points**.

These Binance quotes were captured on **16 November 2026, 10:52:31–10:52:40 UTC**. Each pair matches the target/strike and expiry, uses the same notional principal, and sizes the option at 0.10 contracts. **All comparisons use gross income: trading costs and the capital buffer are excluded, with no exercise or currency conversion assumed.**

![All 66 matched quotes across eight expiry groups. Each dot is a pair; gold diamonds are group medians. Overall median gross income uplift: 33.44%. Median gross annualized gap: 12.41 percentage points, with a 6.46–24.61 percentage point interquartile range. Costs and capital buffer excluded; no exercise assumed.](docs/images/yield-overview-en.svg)

Every pair is plotted. Gold diamonds and the numbers to the right show group medians:

| Term | Pairs | Median gross income uplift | Median annualized gap, pp | Annualized gap range, pp |
| --- | ---: | ---: | ---: | ---: |
| 1 day | 10 | +49.01% | +45.67 | +10.95–84.12 |
| 2 days | 13 | +26.73% | +26.71 | +6.61–45.55 |
| 3 days | 7 | +32.42% | +19.86 | +6.31–31.06 |
| 4 days | 8 | +33.99% | +17.28 | +5.85–24.49 |
| 11 days | 7 | +31.05% | +9.55 | +3.22–16.78 |
| 18 days | 7 | +26.52% | +7.72 | +2.97–12.45 |
| 39 days | 8 | +29.61% | +3.89 | +2.48–8.31 |
| 74 days | 6 | +46.40% | +5.80 | +3.06–6.66 |

These equally weighted quote statistics are **neither a trading win rate nor a portfolio return**. See the [low, middle and high examples and strike-distance breakdown (Chinese)](docs/yield-comparison.md#样本分布与分组), or inspect [all 66 pairs individually (Chinese)](docs/yield-comparison-samples.md).

<details>
<summary>See the maximum case: +84.12 percentage points in gross annualized return</summary>

**The largest gross annualized gain among the 66 eligible pairs: on the same 6,425 USDT notional principal, this one-day sample has 14.81 USDT more gross income (+37.78%), a gross annualized difference of 84.12 percentage points.** The comparison excludes trading costs and the capital buffer, and assumes no exercise or currency conversion at expiry. It is the maximum observed in this capture.

![Largest gross annualized gain among 66 eligible pairs. Same 6,425 USDT notional: Dual Investment term reward of 39.19 USDT versus 54.00 USDT gross put premium. Extra gross income: 14.81 USDT, or 37.78%. Gross annualized: 222.65% versus 306.77%, an 84.12 percentage point gain. Quotes from 16 November 2026; no exercise assumed. Trading costs and the capital buffer are excluded.](docs/images/yield-comparison-en.svg)

Captured from Binance on **16 November 2026, 10:52:31–10:52:40 UTC**. Both quotes have a **64,250 USDT** target/strike and expire on **17 November 2026 at 08:00 UTC**, using a **one-day** term.

| Same notional principal, gross income comparison | Binance Dual Investment: Buy Low BTC | Sell the put through this app |
| --- | --- | --- |
| Notional principal | Subscribe 6,425 USDT | Strike × BTC amount = 6,425 USDT |
| BTC amount, excluding rewards | 0.10 BTC | 0.10 BTC, or 0.10 contracts |
| Term reward / gross premium | 39.19 USDT | **54.00 USDT** |
| Simple gross annualized return on notional | 222.65% | **306.77%** |

The put's best bid was 540 USDT, giving **54.00 USDT gross premium** for 0.10 contracts. Dual Investment quoted 223.55% APR; the table annualizes its term reward after the official [rounding rule](https://www.binance.com/en/dual-investment). Differences use unrounded values. Actual trading costs and funding requirements are covered in the [calculation notes (Chinese)](docs/yield-comparison.md#实际费用与备资).

</details>

**This measures a gross quoted income difference.** Binance describes Dual Investment as having [no trading fees](https://www.binance.com/en/support/faq/detail/09911be9f5754c34806bd3ab4a427e0e); these observations do not establish an embedded commission. Cashflows at exercise differ, and actual fees and slippage affect net returns. Quotes move, and short-term annualized rates are not promised year-round returns.

Of 67 pairs with matching terms, one had less than 0.10 contracts at the best bid and was excluded under the same size rule, leaving 66. Read the [selection and calculations (Chinese)](docs/yield-comparison.md). No trades were executed during collection.

## From contract selection to settlement

Dual Investment Assistant is for BTC/USDT short-option strategies on a dedicated Binance account. You select contracts and confirm new orders; the app automatically synchronizes short net positions and handles delivery for obligations registered before expiry.

| Capability | What you can do |
| --- | --- |
| Contract selection | Switch between selling puts to buy low and selling calls to sell high. Filter by expiry, annualized estimate, exercise probability, and distance from spot. |
| Order review | Review estimated premium and breakeven before confirming the contract and quantity. Contracts with an active position are marked. |
| Net position tracking | Automatically synchronize BTC/USDT short net positions on connection. Overview, holding badges, and the expiry calendar use contract positions; order history stays separate. |
| Automatic delivery | Confirm exercise facts for obligations registered before expiry, then perform the required spot trades and funding steps. Follow delivery tasks and recovery records. |
| Premium statistics | View locally recorded and today's sell premiums, with a Put/Call breakdown and fill history. |
| Local storage | Store credentials in the system keyring. Each local SQLite database is bound to one account and stores orders, positions, delivery tasks, and recovery records. |
| Multiple languages | Use any of 13 interface languages. Switching languages preserves forms and trading state. |

> [!IMPORTANT]
> **This is a live-trading client. Use a dedicated account.** Connecting automatically tracks BTC/USDT short net positions, excluding long positions. Obligations registered before expiry can trigger real spot orders in the background to replicate physical delivery. New option orders still require manual confirmation. Do not manually fulfill the same delivery through another terminal: the app may execute it again. Read the [trading safety guide](docs/trading-safety.md) before use.

## Quick start

### Install and check platform status

Installers are distributed through [GitHub Releases](https://github.com/crablet/dual-investment/releases). If no matching release is available yet, build from source using the instructions below.

| Platform | Installation and runtime | Validation status |
| --- | --- | --- |
| Linux x64 | Install `.deb` / `.rpm` with your package manager. Requires WebKitGTK 4.1 and a Secret Service / keyring. | Used and developed on Linux; individual distributions still need compatibility checks. |
| Windows 10 / 11 x64 | Run the `-setup.exe` installer. It downloads WebView2 if the runtime is missing. | Platform integration and packaging are configured; CI builds and real-device validation are still pending. |
| macOS 13.3+, Apple Silicon / Intel | Open the Universal `.dmg`, drag the app to Applications, then launch it. | Platform integration and packaging are configured; CI builds and real-device validation are still pending. |

Windows installers are unsigned. macOS uses a free ad-hoc signature without Apple notarization, so the first launch may require system approval. See [platform setup](docs/development.md#平台环境).

### Run from source

Use the same commands on all three platforms. Install **Node.js 24+, pnpm 12.9.1, and Rust 1.95+**, plus the [platform build dependencies](docs/development.md#平台环境). Windows requires the MSVC toolchain; macOS requires Xcode Command Line Tools.

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

Build installers for the current operating system:

```bash
pnpm tauri:build
```

Packages default to `src-tauri/target/release/bundle/`. Tauri selects the platform configuration automatically. Trading requires the Tauri desktop runtime; `pnpm dev` is for browser UI previews only.

The [build workflow](.github/workflows/build-desktop.yml) creates installers for all three platforms when a version tag is pushed and attaches them to a release draft for manual publication. It uses standard runners in public repositories only; private repositories are skipped to keep the workflow free. See the [release guide](docs/releases.md) for setup and cost boundaries.

## Workflow

If you're signing up for Binance, you're welcome to [use our referral link](https://www.bsmkweb.net/register?ref=ZEABG36I) or enter referral code `ZEABG36I` to receive the applicable trading fee discounts.

1. **Connect a dedicated account** — Enter API credentials under **Settings → Binance connection**. Disable withdrawals and configure the required read, trading, Simple Earn, and Universal Transfer permissions. The app automatically synchronizes unexpired BTC/USDT short net positions.
2. **Choose a strategy** — Open **Dual investment**, choose buy-low or sell-high, filter contracts, and enter a quantity.
3. **Review and submit** — Check the estimated premium and breakeven, then confirm in the review dialog to send a live order.
4. **Follow automatic delivery** — See net positions in **Overview**, and separate local order history and delivery tasks in **Strategy orders**. Keep the app running and connected to handle registered obligations. Use the recovery center to review exceptions.

Binance options are cash-settled; the client executes separate real spot trades to replicate physical delivery. Registered obligations can resume after the app was offline across expiry. Positions or exercise history first discovered after expiry do not trigger retrospective delivery. Displayed annualized returns and premium estimates are not final fills or promised returns. See the [usage guide](docs/usage.md) for details.

## Use your preferred language

简体中文 · English · 繁體中文 · Português (Brasil) · Español · Tiếng Việt · Türkçe · Bahasa Indonesia · 日本語 · 한국어 · ไทย · Esperanto · Français

Choose a language under **Settings → General settings**. The app follows system preferences by default and falls back to English if none match. Language resources ship with the app, so switching does not require a download. Dates retain the system's local time zone and the Gregorian calendar.

## Documentation

The detailed guides are currently in Chinese; this README covers the essential setup and operating boundaries in English.

| Learn about | Guide |
| --- | --- |
| Real quote comparisons, costs, and calculation methods | [Return comparison and data notes](docs/yield-comparison.md) |
| Connecting, selecting contracts, tracking orders, and viewing returns | [Usage guide](docs/usage.md) |
| Authorization, reserved funds, unconfirmed requests, and recovery | [Trading safety](docs/trading-safety.md) |
| Running from source, builds, architecture, and localization | [Development guide](docs/development.md) |
| Automated installers, free build constraints, and releases | [Release guide](docs/releases.md) |
| Repository rules for contributors | [AGENTS.md](AGENTS.md) |

## Development

The frontend uses **Vue 3, TypeScript, and Pinia**, with **Tauri 2** as the desktop runtime. **Rust** handles trading orchestration and financial calculations, using **Decimal** for amounts and **SQLite** for local records.

Report problems through [Issues](https://github.com/crablet/dual-investment/issues), or contribute improvements to the interface, translations, and documentation. Include your environment, reproduction steps, and sanitized error details. Do not attach API credentials, signed requests, or raw account responses.

Read [AGENTS.md](AGENTS.md) and the [development guide](docs/development.md), then complete the [relevant checks](docs/development.md#验证与打包). Changes to orders, funding, or recovery also require reading the [trading safety guide](docs/trading-safety.md) first.

## License

Project-owned content is licensed under [AGPL-3.0-only](LICENSE).
