package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// Credentials holds everything needed to connect to Alpaca.
// Persisted as JSON under the OS user-config directory.
type Credentials struct {
	APIKey    string `json:"api_key"`
	APISecret string `json:"api_secret"`
	BaseURL   string `json:"base_url"`
}

func configPath() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "alpaca-tui", "credentials.json"), nil
}

func loadCredentials() (Credentials, error) {
	path, err := configPath()
	if err != nil {
		return Credentials{}, err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return Credentials{}, err
	}
	var creds Credentials
	return creds, json.Unmarshal(data, &creds)
}

func saveCredentials(creds Credentials) error {
	path, err := configPath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(creds, "", "  ")
	if err != nil {
		return err
	}
	// 0600 = owner read/write only
	return os.WriteFile(path, data, 0600)
}

func deleteCredentials() {
	path, _ := configPath()
	os.Remove(path)
}

// runSetup shows a first-run credential form and returns the entered values.
// It runs its own tview application, blocking until the user hits CONNECT.
func runSetup() Credentials {
	tview.Styles = tview.Theme{
		PrimitiveBackgroundColor:    cBlack,
		ContrastBackgroundColor:     tcell.NewRGBColor(55, 55, 55),
		MoreContrastBackgroundColor: cBlack,
		BorderColor:                 cOrange,
		TitleColor:                  cOrange,
		GraphicsColor:               cOrange,
		PrimaryTextColor:            cWhite,
		SecondaryTextColor:          cOrange,
		TertiaryTextColor:           cGray2,
		InverseTextColor:            cBlack,
		ContrastSecondaryTextColor:  cCyan,
	}

	var creds Credentials
	envOpts := []string{
		"Paper  (paper-api.alpaca.markets)",
		"Live   (api.alpaca.markets)",
	}

	app := tview.NewApplication()

	errTV := tview.NewTextView().SetDynamicColors(true)
	errTV.SetBackgroundColor(cBlack)

	form := tview.NewForm()
	form.SetBackgroundColor(cBlack)
	form.
		SetFieldBackgroundColor(cDark).
		SetFieldTextColor(cWhite).
		SetLabelColor(cOrange).
		SetButtonBackgroundColor(cOrange).
		SetButtonTextColor(cBlack).
		SetBorder(true).
		SetBorderColor(cOrange).
		SetTitle(" [#FF6600::b]ALPACA TUI — FIRST-TIME SETUP[-] ").
		SetTitleAlign(tview.AlignLeft)

	form.
		AddInputField("  API Key     ", "", 44, nil, nil).
		AddPasswordField("  API Secret  ", "", 44, '*', nil).
		AddDropDown("  Environment ", envOpts, 0, nil).
		AddButton("   CONNECT   ", func() {
			key := strings.TrimSpace(form.GetFormItem(0).(*tview.InputField).GetText())
			secret := strings.TrimSpace(form.GetFormItem(1).(*tview.InputField).GetText())
			if key == "" || secret == "" {
				errTV.SetText("  [#FF3131]>> API Key and Secret are both required.[-]")
				return
			}
			_, env := form.GetFormItem(2).(*tview.DropDown).GetCurrentOption()
			creds.APIKey = key
			creds.APISecret = secret
			if strings.Contains(env, "Live") {
				creds.BaseURL = "https://api.alpaca.markets"
			} else {
				creds.BaseURL = "https://paper-api.alpaca.markets"
			}
			app.Stop()
		}).
		AddButton("   QUIT   ", func() { os.Exit(0) })

	// Style the environment dropdown list
	dd := form.GetFormItem(2).(*tview.DropDown)
	dd.SetListStyles(
		tcell.StyleDefault.Foreground(cWhite).Background(cDark),
		tcell.StyleDefault.Foreground(cBlack).Background(cCyan).Attributes(tcell.AttrBold),
	)

	hintTV := tview.NewTextView().SetDynamicColors(true).
		SetText("  [#555555]Credentials saved to your OS config directory. Run with --reset to re-enter.[-]")
	hintTV.SetBackgroundColor(cBlack)

	inner := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(form, 0, 1, true).
		AddItem(errTV, 1, 0, false).
		AddItem(hintTV, 1, 0, false)

	centered := tview.NewFlex().
		AddItem(nil, 0, 1, false).
		AddItem(
			tview.NewFlex().SetDirection(tview.FlexRow).
				AddItem(nil, 0, 1, false).
				AddItem(inner, 0, 2, true).
				AddItem(nil, 0, 1, false),
			64, 0, true).
		AddItem(nil, 0, 1, false)
	centered.SetBackgroundColor(cBlack)

	app.SetRoot(centered, true).Run()
	return creds
}
