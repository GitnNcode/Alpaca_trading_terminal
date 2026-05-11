package main

import (
	"sort"
	"strings"
	"sync"
)

var (
	assetMu      sync.RWMutex
	assetSymbols []string          // sorted, for binary-search prefix lookup
	assetNames   map[string]string // symbol -> display name
)

// loadAssets fetches all active, tradable US equity symbols from Alpaca and
// caches them. Called once as a goroutine at startup; autocomplete returns
// nothing until the fetch completes (~1 s on a normal connection).
func loadAssets() {
	assets, err := client.GetAssets()
	if err != nil {
		return
	}

	syms := make([]string, 0, len(assets))
	names := make(map[string]string, len(assets))
	for _, a := range assets {
		if a.Tradable {
			syms = append(syms, a.Symbol)
			names[a.Symbol] = a.Name
		}
	}
	sort.Strings(syms)

	assetMu.Lock()
	assetSymbols = syms
	assetNames = names
	assetMu.Unlock()
}

// filterStocks returns up to limit autocomplete entries matching prefix.
// Each entry is "SYMBOL  Company Name" so the caller must strip back to the
// ticker on selection. Uses binary search since the slice is sorted.
func filterStocks(prefix string, limit int) []string {
	prefix = strings.ToUpper(prefix)

	assetMu.RLock()
	defer assetMu.RUnlock()

	if len(assetSymbols) == 0 {
		return nil
	}

	start := sort.SearchStrings(assetSymbols, prefix)
	var out []string
	for i := start; i < len(assetSymbols) && len(out) < limit; i++ {
		sym := assetSymbols[i]
		if !strings.HasPrefix(sym, prefix) {
			break
		}
		display := sym
		if name := assetNames[sym]; name != "" {
			if len(name) > 38 {
				name = name[:35] + "…"
			}
			display = sym + "  " + name
		}
		out = append(out, display)
	}
	return out
}
