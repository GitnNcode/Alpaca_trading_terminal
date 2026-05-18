package main

import "testing"

func TestBollingerWarmupSafe(t *testing.T) {
	// Fewer bars than Period — must return all Holds.
	closes := make([]float64, 15)
	for i := range closes {
		closes[i] = 100 + float64(i)
	}
	bars := barsFromCloses(closes)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)
	if len(signals) != len(bars) {
		t.Fatalf("len = %d, want %d", len(signals), len(bars))
	}
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v during warm-up, want Hold", i, s)
		}
	}
}

func TestBollingerBuysOnLowerBandTouch(t *testing.T) {
	// 30 flat bars then a sharp dip below the lower band. Expect a Buy.
	closes := make([]float64, 40)
	for i := 0; i < 30; i++ {
		closes[i] = 100
	}
	for i := 30; i < 40; i++ {
		closes[i] = 80 // far below mean → outside the lower band
	}
	bars := barsFromCloses(closes)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)

	var sawBuy bool
	for _, s := range signals {
		if s == SignalBuy {
			sawBuy = true
			break
		}
	}
	if !sawBuy {
		t.Fatal("BollingerBands did not emit Buy on an obvious lower-band breach")
	}
}

func TestBollingerShortsOnUpperBandTouch(t *testing.T) {
	closes := make([]float64, 40)
	for i := 0; i < 30; i++ {
		closes[i] = 100
	}
	for i := 30; i < 40; i++ {
		closes[i] = 120
	}
	bars := barsFromCloses(closes)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)

	var sawShort bool
	for _, s := range signals {
		if s == SignalShort {
			sawShort = true
			break
		}
	}
	if !sawShort {
		t.Fatal("BollingerBands did not emit Short on an obvious upper-band breach")
	}
}

func TestBollingerExitsAtMeanAfterLong(t *testing.T) {
	// Flat → dip → revert past mean. The Buy must be followed by a Sell
	// once price climbs back across the middle band.
	closes := make([]float64, 0, 60)
	for i := 0; i < 25; i++ {
		closes = append(closes, 100)
	}
	for i := 0; i < 5; i++ {
		closes = append(closes, 80) // dip below lower band
	}
	for i := 0; i < 30; i++ {
		closes = append(closes, 105) // recover past the moving mean
	}
	bars := barsFromCloses(closes)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)

	var sawBuy, sawSellAfterBuy bool
	for _, s := range signals {
		switch s {
		case SignalBuy:
			sawBuy = true
		case SignalSell:
			if sawBuy {
				sawSellAfterBuy = true
			}
		}
	}
	if !sawBuy || !sawSellAfterBuy {
		t.Fatalf("expected Buy followed by Sell at mean; got Buy=%v, SellAfterBuy=%v",
			sawBuy, sawSellAfterBuy)
	}
}

func TestBollingerPositionStateIsConsistent(t *testing.T) {
	// Whippy alternating overshoots — every entry must have a matching
	// exit before the next entry. No double-entries.
	closes := make([]float64, 0, 200)
	for cycle := 0; cycle < 4; cycle++ {
		for i := 0; i < 25; i++ {
			closes = append(closes, 100)
		}
		for i := 0; i < 5; i++ {
			closes = append(closes, 80)
		}
		for i := 0; i < 5; i++ {
			closes = append(closes, 105)
		}
		for i := 0; i < 5; i++ {
			closes = append(closes, 120)
		}
		for i := 0; i < 5; i++ {
			closes = append(closes, 100)
		}
	}
	bars := barsFromCloses(closes)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)

	position := 0
	for i, s := range signals {
		switch s {
		case SignalBuy:
			if position != 0 {
				t.Fatalf("Buy at idx %d while position=%d", i, position)
			}
			position = 1
		case SignalShort:
			if position != 0 {
				t.Fatalf("Short at idx %d while position=%d", i, position)
			}
			position = -1
		case SignalSell:
			if position == 0 {
				t.Fatalf("Sell at idx %d while flat", i)
			}
			position = 0
		}
	}
}
