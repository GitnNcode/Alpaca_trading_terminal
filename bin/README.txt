ALPACA TRADING TERMINAL
=======================

WHICH FILE DO I RUN?
---------------------
  Windows                  ->  Alpaca_Trading_Terminal_WIN.exe
  Mac (M1 / M2 / M3 / M4) ->  Alpaca_Trading_Terminal_MAC_ARM
  Mac (Intel)              ->  Alpaca_Trading_Terminal_MAC_INTEL
  Linux (64-bit)           ->  Alpaca_Trading_Terminal_LINUX

Not sure which Mac you have? Apple menu -> About This Mac.
If it says "Apple M1 / M2 / M3 / M4" use MAC_ARM. If it says "Intel" use MAC_INTEL.


FIRST-TIME SETUP (ALL PLATFORMS)
---------------------------------
On first launch the app shows a setup screen asking for your Alpaca API
credentials. You only do this once — they are saved securely and never
need to be entered again.

  Step 1.  Go to https://app.alpaca.markets and log in.

  Step 2.  Navigate to:  Home -> API Keys -> Generate New Key

  Step 3.  Copy your API Key and API Secret.
           (The secret is only shown once — save it somewhere safe.)

  Step 4.  Launch the app. The setup screen appears automatically.

  Step 5.  Paste your API Key and API Secret into the form.

  Step 6.  Choose your environment:
             Paper  = simulated trading, no real money (recommended to start)
             Live   = real orders with real money

  Step 7.  Press CONNECT. You're in.

Where your credentials are stored:
  Windows  ->  %APPDATA%\alpaca-tui\credentials.json
  Mac      ->  ~/Library/Application Support/alpaca-tui/credentials.json
  Linux    ->  ~/.config/alpaca-tui/credentials.json

To reset and re-enter your credentials, run with --reset:
  Windows  ->  Alpaca_Trading_Terminal_WIN.exe --reset
  Mac/Linux->  ./Alpaca_Trading_Terminal_MAC_ARM --reset


MAC SETUP (run these in Terminal once)
---------------------------------------
  # Navigate to the folder containing the app
  cd ~/Downloads       (or wherever you saved it)

  # Make the binary executable
  chmod +x Alpaca_Trading_Terminal_MAC_ARM

  # Run it
  ./Alpaca_Trading_Terminal_MAC_ARM

If macOS says "cannot be opened because the developer cannot be verified":
  Option A (easiest):
    - Right-click the file in Finder
    - Click Open
    - Click Open in the dialog that appears
    This permanently allows this binary to run.

  Option B (Terminal):
    xattr -d com.apple.quarantine Alpaca_Trading_Terminal_MAC_ARM
    ./Alpaca_Trading_Terminal_MAC_ARM


LINUX SETUP (run these in Terminal once)
-----------------------------------------
  chmod +x Alpaca_Trading_Terminal_LINUX
  ./Alpaca_Trading_Terminal_LINUX


KEYBOARD SHORTCUTS
------------------
  1 / 2 / 3 / 4   Switch tabs (Positions / Trade / Orders / Activity)
  R  or  F5        Refresh data manually
  X  or  Delete    Cancel selected order (on the Orders tab)
  Q  or  Escape    Quit


NOTES
-----
  - Data auto-refreshes every 10 seconds. Watch the progress bar top-right.
  - All placed orders are logged to trades.csv in the folder you run from.
  - Paper trading is completely safe — no real money is ever used.
  - To switch between Paper and Live accounts, run with --reset and re-enter
    your keys for the other environment.
