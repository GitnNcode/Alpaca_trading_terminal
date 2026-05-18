package main

import (
	"testing"
	"time"

	"github.com/gdamore/tcell/v2"
)

// startSimApp boots a fresh app on a SimulationScreen and returns it, a
// channel that closes when Run() returns, and a cleanup. Mirrors the helper
// used by the canonical chart_test.go.
func startSimApp(t *testing.T) (*app, <-chan struct{}, func()) {
	t.Helper()
	registerStrategies()
	a := newApp()

	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init simulation screen: %v", err)
	}
	screen.SetSize(120, 40)
	a.tapp.SetScreen(screen)

	runDone := make(chan struct{})
	go func() {
		_ = a.tapp.Run()
		close(runDone)
	}()
	time.Sleep(150 * time.Millisecond) // let the event loop spin up

	cleanup := func() {
		a.tapp.Stop()
		select {
		case <-runDone:
		case <-time.After(2 * time.Second):
		}
	}
	return a, runDone, cleanup
}

// queueRead runs fn on the event-loop goroutine and returns its result with a
// 2-second timeout. Reading state from outside the loop without this is racy.
func queueRead[T any](t *testing.T, a *app, fn func() T) T {
	t.Helper()
	out := make(chan T, 1)
	go a.tapp.QueueUpdate(func() { out <- fn() })
	select {
	case v := <-out:
		return v
	case <-time.After(2 * time.Second):
		t.Fatal("queueRead timed out — event loop stuck?")
		var zero T
		return zero
	}
}

// Q and R must reach the symbol InputField as typed characters — otherwise a
// user typing "QQQ" or "RBLX" cannot enter their ticker.
func TestQAndRTypeIntoSymbolField(t *testing.T) {
	a, _, cleanup := startSimApp(t)
	defer cleanup()

	for _, r := range "QRBLX" {
		a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, r, tcell.ModNone))
	}
	time.Sleep(200 * time.Millisecond)

	got := queueRead(t, a, func() string { return a.symField.GetText() })
	if got != "QRBLX" {
		t.Fatalf("symField = %q, want %q (Q/R should not be intercepted in InputField)", got, "QRBLX")
	}
}

// Lowercase q/r are the same — case shouldn't change interception.
func TestLowercaseQRTypeIntoSymbolField(t *testing.T) {
	a, _, cleanup := startSimApp(t)
	defer cleanup()

	for _, r := range "qrx" {
		a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, r, tcell.ModNone))
	}
	time.Sleep(200 * time.Millisecond)

	got := queueRead(t, a, func() string { return a.symField.GetText() })
	if got != "qrx" {
		t.Fatalf("symField = %q, want %q", got, "qrx")
	}
}

// On Button focus (no text input), Q is a global quit shortcut: Run() returns.
func TestQQuitsFromButtonFocus(t *testing.T) {
	a, runDone, cleanup := startSimApp(t)
	defer cleanup() // safe to call even after Stop()

	a.tapp.QueueUpdate(func() { a.tapp.SetFocus(a.runBtn) })
	time.Sleep(100 * time.Millisecond)
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'Q', tcell.ModNone))

	select {
	case <-runDone:
		// good — Q on Button focus stopped the app
	case <-time.After(2 * time.Second):
		t.Fatal("Q on Button focus did not quit the app")
	}
}

// On Button focus, R re-runs the backtest. With no ticker entered, that
// surfaces the "Enter a ticker" error in the status bar — proof that R was
// intercepted as a shortcut rather than ignored.
func TestRTriggersRerunFromButtonFocus(t *testing.T) {
	a, _, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdate(func() { a.tapp.SetFocus(a.runBtn) })
	time.Sleep(100 * time.Millisecond)

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'R', tcell.ModNone))
	time.Sleep(200 * time.Millisecond)

	status := queueRead(t, a, func() string { return a.statusTV.GetText(true) })
	// The empty-ticker error contains "Enter a ticker"; the success path or
	// idle status do not. Matching on substring keeps the test resilient to
	// minor copy changes.
	if !contains(status, "Enter a ticker") {
		t.Fatalf("R on Button did not trigger runBacktest; status = %q", status)
	}
}

func contains(haystack, needle string) bool {
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if haystack[i:i+len(needle)] == needle {
			return true
		}
	}
	return false
}
