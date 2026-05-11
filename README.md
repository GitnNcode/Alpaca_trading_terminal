# Alpaca Trading Terminal

A Bloomberg-style terminal trading application built in Go. Runs entirely in the terminal — no browser, no GUI, no Electron. Connect your [Alpaca Markets](https://alpaca.markets) account and trade US equities and ETFs from the command line.

![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)
![Go](https://img.shields.io/badge/go-1.22-00ADD8)
![License](https://img.shields.io/badge/license-MIT-green)

---

## Screenshots

<img width="1466" height="702" alt="Screenshot 2026-05-10 235544" src="https://github.com/user-attachments/assets/b3016e77-2fc8-4031-985b-7521ba6f8d00" />
<img width="2558" height="1371" alt="Screenshot 2026-05-10 232340" src="https://github.com/user-attachments/assets/42f8bdd6-bcd0-4a1e-a647-95bda92956f6" />

---

## Features

- **4-tab interface** — Positions, Trade, Orders, Activity
- **Live positions** — symbol, quantity, average entry, current price, market value, unrealized P&L ($ and %) color-coded green/red
- **Order placement** — market and limit orders with a confirmation modal before submission
- **Smart autocomplete** — searches by ticker prefix _and_ full company name (type `apple` to find `AAPL`); company name shown in blue after selection
- **Order management** — view all pending orders; cancel any with `X` or `Delete`
- **Full activity log** — fills, partial fills, dividends, journal entries, withdrawals, ACAT transfers, and closed orders — merged and sorted chronologically
- **Auto-refresh** — all data refreshes every 10 seconds; animated progress bar in the top-right corner
- **Trade logging** — every placed order is appended to `trades.csv` in the working directory
- **Secure credential storage** — API key and secret saved to the OS user-config directory on first launch; never stored in plaintext in the project folder
- **Paper & Live modes** — switch between simulated and real-money trading via `--reset`
- **Zero dependencies at runtime** — single static binary, no install required

---

## Tech Stack

| Layer | Library / Tool |
|-------|---------------|
| Language | Go 1.22 |
| TUI framework | [rivo/tview](https://github.com/rivo/tview) |
| Terminal backend | [gdamore/tcell v2](https://github.com/gdamore/tcell) |
| Broker API | [Alpaca Markets REST API v2](https://docs.alpaca.markets/reference/getallassets) |

---

## Folder Structure

```
alpaca-tcell/
├── main.go        # Core TUI app — tabs, tables, form, modals, key handling, auto-refresh
├── api.go         # Alpaca REST client — positions, orders, account, activities, assets
├── config.go      # Credential storage and first-run setup screen
├── stocks.go      # Asset list loader; ticker-prefix + company-name autocomplete
├── go.mod
├── go.sum
├── .gitignore
├── bin/
│   ├── Alpaca_Trading_Terminal_WIN.exe
│   ├── Alpaca_Trading_Terminal_MAC_ARM
│   ├── Alpaca_Trading_Terminal_MAC_INTEL
│   ├── Alpaca_Trading_Terminal_LINUX
│   └── README.txt         # End-user quickstart (platform-specific instructions)
└── trades.csv             # Created at runtime — gitignored
```

---

## Getting an Alpaca API Key

1. Go to [app.alpaca.markets](https://app.alpaca.markets) and log in (or create a free account).
2. Navigate to **Home → API Keys → Generate New Key**.
3. Copy your **API Key** and **API Secret** — the secret is shown only once.
4. Choose an environment:
   - **Paper** — simulated trading, no real money (recommended to start)
   - **Live** — real orders with real money

---

## Running a Pre-Built Binary

Download the `bin/` folder and run the binary for your platform:

| Platform | Binary |
|----------|--------|
| Windows | `Alpaca_Trading_Terminal_WIN.exe` |
| macOS (M1 / M2 / M3 / M4) | `Alpaca_Trading_Terminal_MAC_ARM` |
| macOS (Intel) | `Alpaca_Trading_Terminal_MAC_INTEL` |
| Linux 64-bit | `Alpaca_Trading_Terminal_LINUX` |

Not sure which Mac you have? **Apple menu → About This Mac**. "Apple M_" = ARM, "Intel" = INTEL.

### Windows
Double-click `Alpaca_Trading_Terminal_WIN.exe` or run it from a terminal.

### macOS
```bash
cd ~/Downloads          # or wherever you saved it
chmod +x Alpaca_Trading_Terminal_MAC_ARM
./Alpaca_Trading_Terminal_MAC_ARM
```

If macOS blocks the binary ("developer cannot be verified"):
- **Option A** — Right-click → Open → click Open in the dialog. This permanently allows the binary.
- **Option B** — Run `xattr -d com.apple.quarantine Alpaca_Trading_Terminal_MAC_ARM` then relaunch.

### Linux
```bash
chmod +x Alpaca_Trading_Terminal_LINUX
./Alpaca_Trading_Terminal_LINUX
```

### First Launch (all platforms)
On first run a setup screen appears automatically. Enter your API Key, API Secret, and choose Paper or Live. Credentials are saved to:

| OS | Path |
|----|------|
| Windows | `%APPDATA%\alpaca-tui\credentials.json` |
| macOS | `~/Library/Application Support/alpaca-tui/credentials.json` |
| Linux | `~/.config/alpaca-tui/credentials.json` |

To reset credentials and re-enter them:
```bash
./Alpaca_Trading_Terminal_MAC_ARM --reset
Alpaca_Trading_Terminal_WIN.exe --reset
```

---

## Building from Source

**Prerequisites:** Go 1.22+

```bash
git clone <repo-url>
cd alpaca-tcell

# Run locally
go run .

# Build for your current platform
go build -o alpaca-tui .
```

### Production Builds (all platforms)

```bash
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_WIN.exe .
CGO_ENABLED=0 GOOS=darwin  GOARCH=arm64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_ARM .
CGO_ENABLED=0 GOOS=darwin  GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_INTEL .
CGO_ENABLED=0 GOOS=linux   GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_LINUX .
```

Flags used:
- `-ldflags="-s -w"` — strips debug symbols and DWARF info
- `-trimpath` — removes local file system paths from the binary
- `CGO_ENABLED=0` — fully static binary, no C dependencies

---

## Credentials / Environment Variables

There are **no environment variables** to set. API credentials are entered once through the in-app setup screen and stored in the OS user-config directory with `0600` permissions (owner read/write only).

The `--reset` flag clears stored credentials so you can re-enter them (e.g., to switch between Paper and Live).

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `1` `2` `3` `4` | Switch tabs (Positions / Trade / Orders / Activity) |
| `←` `→` | Switch tabs |
| `↑` `↓` | Navigate form fields (Trade tab) or autocomplete list |
| `Tab` / `Shift-Tab` | Navigate between form fields (Trade tab) |
| `Enter` | Open dropdown / confirm |
| `R` or `F5` | Refresh data manually |
| `X` or `Delete` | Cancel selected order (Orders tab) |
| `Q` or `Escape` | Quit |

---

## Trade Tab Usage

1. Switch to the Trade tab with `2` or `→`.
2. Press `Enter` on **ACTION** to choose BUY or SELL.
3. Press `↓` to move to **TYPE** — choose MARKET or LIMIT.
4. Press `↓` to move to **SYMBOL** — type a ticker (`AAPL`) or company name (`apple`). Select from the autocomplete list with arrow keys and `Enter`.
5. Enter **QUANTITY** (shares).
6. For LIMIT orders, enter a **LIMIT PX**.
7. Tab to **PLACE ORDER** and press `Enter`. A confirmation modal shows the full order details before submission.

---

## Trade Log

Every successfully placed order is appended to `trades.csv` in the directory you run the binary from. The file is created automatically on first use.

```
timestamp,symbol,side,type,qty,limit_price,order_id,status
2024-01-15T14:32:01Z,AAPL,buy,market,10,,abc123,accepted
```

`trades.csv` is gitignored and never committed to the repository.

---

## Contributing

1. Fork the repository.
2. Create a feature branch: `git checkout -b feature/your-feature`.
3. Commit your changes: `git commit -m "Add your feature"`.
4. Push to the branch: `git push origin feature/your-feature`.
5. Open a pull request.

Please keep the zero-external-dependency philosophy — the only runtime dependencies are the Go standard library, tview, and tcell.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
