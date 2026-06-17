<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="Dual Investment Assistant icon" />
  <h1>Dual Investment Assistant</h1>
  <p><strong>Manage BTC/USDT dual-investment-style strategies from your desktop.</strong></p>
  <p>Work with Binance options and spot in one place: filter contracts, review orders, track positions, and manage expiry.</p>
  <p>Binance · BTC/USDT · 13 interface languages · <a href="LICENSE">AGPL-3.0-only</a></p>
  <p><a href="README.md">简体中文</a> · <strong>English</strong></p>
  <p>
    <a href="#quick-start">Quick start</a> ·
    <a href="#workflow">Workflow</a> ·
    <a href="#documentation">Documentation</a> ·
    <a href="#development">Contribute</a>
  </p>
</div>

![The strategy workbench, with contract filters on the left and order estimates on the right](docs/images/workbench-en.webp)

<p align="center">Strategy workbench · Contract selection and order estimates</p>

## From contract selection to settlement

Dual Investment Assistant is for Binance users who want to select their own contracts, confirm orders, and track what happens through expiry. You choose the direction and quantity; the app brings the subsequent operations and their status into one workspace.

| Capability | What you can do |
| --- | --- |
| Contract selection | Switch between selling puts to buy low and selling calls to sell high. Filter by expiry, annualized estimate, exercise probability, and distance from spot. |
| Order review | Review estimated premium and breakeven before confirming the contract and quantity. Contracts with an active position are marked. |
| Strategy tracking | View active strategies, options and spot positions, returns, and an expiry calendar. Manually adopt existing BTC/USDT short-option positions. |
| Expiry handling | Follow cash settlement, spot rebalancing, and fund sweeps for confirmed strategies. Review exceptions and unconfirmed requests in the recovery center. |
| Local storage | Store credentials in the system keyring, orders and recovery records in local SQLite, and inspect logs in the app. |
| Multiple languages | Use any of 13 interface languages. Switching languages preserves forms and trading state. |

> [!IMPORTANT]
> **This is a live-trading client.** It currently supports Binance and BTC/USDT only, using options and spot trades; there is no paper-trading mode. New strategies require manual confirmation. Recovery, settlement, and funding steps for confirmed strategies may then continue in the background. Read the [trading safety guide](docs/trading-safety.md) before use.

## Quick start

The current packaging configuration targets **Linux (`.deb` / `.rpm`)**. You can run the app from source.

Install **Node.js 24+, pnpm, and Rust 1.95+**, along with the system dependencies in the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/). Linux also needs a working Secret Service / keyring service for credential storage.

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

Build Linux installers:

```bash
pnpm tauri:build
```

Packages are written to `src-tauri/target/release/bundle/`. Trading requires the Tauri desktop runtime; `pnpm dev` is for browser UI previews only. See the [development guide](docs/development.md) for setup, commands, and troubleshooting.

## Workflow

1. **Connect your account** — Enter API credentials under **Settings → Binance connection**. Disable withdrawals and configure the required read, trading, Simple Earn, and Universal Transfer permissions.
2. **Choose a strategy** — Open **Dual investment**, choose buy-low or sell-high, filter contracts, and enter a quantity.
3. **Review and submit** — Check the estimated premium and breakeven, then confirm in the review dialog to send a live order.
4. **Track expiry handling** — Follow status in **Strategy orders** and **Overview**. Keep the app running and connected so it can process settlement and final funding steps. Use the recovery center to review exceptions.

Options are cash-settled; the client performs any spot trades required by the strategy separately. Displayed annualized returns and premiums are estimates, not final fills or promised returns. The [usage guide](docs/usage.md) explains the workflow and calculation conventions.

## Use your preferred language

简体中文 · English · 繁體中文 · Português (Brasil) · Español · Tiếng Việt · Türkçe · Bahasa Indonesia · 日本語 · 한국어 · ไทย · Esperanto · Français

Choose a language under **Settings → General settings**. The app follows system preferences by default and falls back to English if none match. Language resources ship with the app, so switching does not require a download. Dates retain the system's local time zone and the Gregorian calendar.

## Documentation

The detailed guides are currently in Chinese; this README covers the essential setup and operating boundaries in English.

| Learn about | Guide |
| --- | --- |
| Connecting, selecting contracts, tracking orders, and viewing returns | [Usage guide](docs/usage.md) |
| Authorization, reserved funds, unconfirmed requests, and recovery | [Trading safety](docs/trading-safety.md) |
| Running from source, builds, architecture, and localization | [Development guide](docs/development.md) |
| Repository rules for contributors | [AGENTS.md](AGENTS.md) |

## Development

The frontend uses **Vue 3, TypeScript, and Pinia**, with **Tauri 2** as the desktop runtime. **Rust** handles trading orchestration and financial calculations, using **Decimal** for amounts and **SQLite** for local records.

Report problems through [Issues](https://github.com/crablet/dual-investment/issues), or contribute improvements to the interface, translations, and documentation. Include your environment, reproduction steps, and sanitized error details. Do not attach API credentials, signed requests, or raw account responses.

Read [AGENTS.md](AGENTS.md) and the [development guide](docs/development.md), then complete the [relevant checks](docs/development.md#验证与打包). Changes to orders, funding, or recovery also require reading the [trading safety guide](docs/trading-safety.md) first.

## License

Project-owned content is licensed under [AGPL-3.0-only](LICENSE).
