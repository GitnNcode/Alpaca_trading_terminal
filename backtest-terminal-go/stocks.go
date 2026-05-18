package main

import (
	"sort"
	"strings"
	"sync"
)

// Asset cache shared between the autocomplete func (called inline on each
// keystroke) and the loadAssets goroutine that fills it. RWMutex because
// reads vastly outnumber writes — the catalogue is fetched once and then
// queried per keystroke.
var (
	assetMu      sync.RWMutex
	assetSymbols []string          // sorted, for binary-search prefix lookup
	assetNames   map[string]string // symbol -> display name
)

// loadAssets pulls all active, tradable US equity symbols from Alpaca and
// caches them. Fire-and-forget from a goroutine at startup — autocomplete
// just returns no matches until the fetch completes (~1 s on a normal
// connection). Returns silently on error so we don't block the UI.
func loadAssets() {
	if client == nil {
		return
	}
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

// filterStocks returns up to `limit` autocomplete entries matching prefix.
// Fast path: binary-search ticker-prefix matches. Slow path: linear scan of
// company-name substrings so typing "apple" surfaces AAPL.
//
// Each returned entry is formatted "SYMBOL  Company Name". Callers should
// strip on the first whitespace to recover the bare ticker.
func filterStocks(prefix string, limit int) []string {
	prefix = strings.ToUpper(prefix)

	assetMu.RLock()
	defer assetMu.RUnlock()

	if len(assetSymbols) == 0 {
		return nil
	}

	seen := make(map[string]bool)
	var out []string

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
