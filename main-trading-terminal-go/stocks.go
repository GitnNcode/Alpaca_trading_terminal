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

// getCompanyName returns the full company name for a ticker symbol, or "".
func getCompanyName(sym string) string {
	assetMu.RLock()
	defer assetMu.RUnlock()
	return assetNames[sym]
}

// filterStocks returns up to limit autocomplete entries matching prefix.
// It first does a fast binary-search for ticker prefix matches, then falls
// back to a linear company-name substring scan to support typing full names.
// Each entry is "SYMBOL  Company Name"; the caller strips to the ticker on selection.
func filterStocks(prefix string, limit int) []string {
	prefix = strings.ToUpper(prefix)

	assetMu.RLock()
	defer assetMu.RUnlock()

	if len(assetSymbols) == 0 {
		return nil
	}

	seen := make(map[string]bool)
	var out []string

	// Fast path: binary search for ticker-prefix matches
	start := sort.SearchStrings(assetSymbols, prefix)
	for i := start; i < len(assetSymbols) && len(out) < limit; i++ {
		sym := assetSymbols[i]
		if !strings.HasPrefix(sym, prefix) {
			break
		}
		seen[sym] = true
		name := assetNames[sym]
		if len(name) > 38 {
			name = name[:35] + "…"
		}
		out = append(out, sym+"  "+name)
	}

	// Slow path: scan company names for substring matches (supports typing "Apple" etc.)
	if len(out) < limit {
		lower := strings.ToLower(prefix)
		for _, sym := range assetSymbols {
			if len(out) >= limit {
				break
			}
			if seen[sym] {
				continue
			}
			name := assetNames[sym]
			if strings.Contains(strings.ToLower(name), lower) {
				seen[sym] = true
				display := name
				if len(display) > 38 {
					display = display[:35] + "…"
				}
				out = append(out, sym+"  "+display)
			}
		}
	}

	return out
}
