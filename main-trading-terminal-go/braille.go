package main

import "github.com/gdamore/tcell/v2"

// brailleLayer accumulates dots in a 2×4-sub-pixel grid per terminal cell and
// renders them as Unicode Braille characters (U+2800 + 8-bit mask). Used for
// smooth-looking indicator lines (EMA today, MACD/RSI/etc. later) — each
// terminal cell has 8× the resolution of a regular character, so diagonals
// no longer stair-step at cell boundaries.
//
// One layer = one color. To draw multiple indicators in different colors, use
// one layer per indicator and renderAt each with its own style. Where two
// layers overlap on the same cell, whichever was rendered LAST wins (last
// renderAt's color shows for that cell).
type brailleLayer struct {
	cells map[[2]int]byte // key: {termCol, termRow}; value: 8-bit dot mask
}

func newBrailleLayer() *brailleLayer {
	return &brailleLayer{cells: make(map[[2]int]byte)}
}

// brailleBit returns the bit mask for a sub-pixel offset within a cell.
// Sub-coords: subX ∈ {0,1}, subY ∈ {0,1,2,3}.
//
// Unicode Braille layout (dots numbered as they appear in U+2800 docs):
//
//	subX=0 subX=1
//	subY=0:   dot1   dot4   → bits 0, 3
//	subY=1:   dot2   dot5   → bits 1, 4
//	subY=2:   dot3   dot6   → bits 2, 5
//	subY=3:   dot7   dot8   → bits 6, 7
func brailleBit(subX, subY int) byte {
	if subY == 3 {
		return byte(0x40 << subX) // dot7 / dot8
	}
	return byte(1 << (subY + 3*subX))
}

// plot sets a single sub-pixel dot. Out-of-range coords are silently dropped
// — the chart's render loop already filters by price visibility.
func (l *brailleLayer) plot(subX, subY int) {
	if subX < 0 || subY < 0 {
		return
	}
	col := subX / 2
	row := subY / 4
	bit := brailleBit(subX%2, subY%4)
	l.cells[[2]int{col, row}] |= bit
}

// line draws a single-sub-pixel-wide line between two sub-pixel points using
// Bresenham. For most indicators you want thickLine instead — single-pixel
// lines are very faint at typical terminal sizes.
func (l *brailleLayer) line(x1, y1, x2, y2 int) {
	dx, dy := x2-x1, y2-y1
	adx, ady := abs(dx), abs(dy)
	sx, sy := 1, 1
	if dx < 0 {
		sx = -1
	}
	if dy < 0 {
		sy = -1
	}
	err := adx - ady
	for {
		l.plot(x1, y1)
		if x1 == x2 && y1 == y2 {
			return
		}
		e2 := err * 2
		if e2 > -ady {
			err -= ady
			x1 += sx
		}
		if e2 < adx {
			err += adx
			y1 += sy
		}
	}
}

// thickLine draws a 2-sub-pixel-wide line. The minor axis of the slope picks
// the offset direction so the line stays a constant 2 sub-pixels thick:
//   - mostly-horizontal (|dx| >= |dy|): plot at (x, y) AND (x, y+1)
//   - mostly-vertical:                 plot at (x, y) AND (x+1, y)
//
// This is the canonical primitive for visible indicator lines.
func (l *brailleLayer) thickLine(x1, y1, x2, y2 int) {
	dx, dy := x2-x1, y2-y1
	adx, ady := abs(dx), abs(dy)
	horizontal := adx >= ady

	sx, sy := 1, 1
	if dx < 0 {
		sx = -1
	}
	if dy < 0 {
		sy = -1
	}
	err := adx - ady
	for {
		l.plot(x1, y1)
		if horizontal {
			l.plot(x1, y1+1)
		} else {
			l.plot(x1+1, y1)
		}
		if x1 == x2 && y1 == y2 {
			return
		}
		e2 := err * 2
		if e2 > -ady {
			err -= ady
			x1 += sx
		}
		if e2 < adx {
			err += adx
			y1 += sy
		}
	}
}

// renderAt emits one Braille rune per occupied cell, anchored at the given
// terminal origin. Sub-pixel (0,0) becomes terminal cell (originX, originY).
func (l *brailleLayer) renderAt(screen tcell.Screen, originX, originY int, style tcell.Style) {
	for pos, bits := range l.cells {
		if bits == 0 {
			continue
		}
		screen.SetContent(originX+pos[0], originY+pos[1], rune(0x2800+int(bits)), nil, style)
	}
}

func abs(v int) int {
	if v < 0 {
		return -v
	}
	return v
}
