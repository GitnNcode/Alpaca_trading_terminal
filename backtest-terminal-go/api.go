package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

// alpacaDataBase is the market-data host. var (not const) so tests can swap it
// for an httptest server.
var alpacaDataBase = "https://data.alpaca.markets"

// Bar is one OHLCV candle from Alpaca's market-data API.
type Bar struct {
	Time   time.Time `json:"t"`
	Open   float64   `json:"o"`
	High   float64   `json:"h"`
	Low    float64   `json:"l"`
	Close  float64   `json:"c"`
	Volume int64     `json:"v"`
}

type Asset struct {
	Symbol   string `json:"symbol"`
	Name     string `json:"name"`
	Status   string `json:"status"`
	Tradable bool   `json:"tradable"`
}

type AlpacaClient struct {
	BaseURL   string
	APIKey    string
	APISecret string
	HTTP      *http.Client
}

func NewAlpacaClient(creds Credentials) *AlpacaClient {
	baseURL := creds.BaseURL
	if baseURL == "" {
		baseURL = "https://paper-api.alpaca.markets"
	}
	return &AlpacaClient{
		BaseURL:   baseURL,
		APIKey:    creds.APIKey,
		APISecret: creds.APISecret,
		HTTP:      &http.Client{Timeout: 30 * time.Second},
	}
}

type barsResponse struct {
	Bars          []Bar  `json:"bars"`
	NextPageToken string `json:"next_page_token"`
}

// GetBars fetches OHLCV bars from data.alpaca.markets for the given symbol.
// Uses the IEX feed (free tier) and split-adjusted prices. Pages through
// all results, capped at 50k bars as a safety belt.
func (c *AlpacaClient) GetBars(symbol, timeframe string, start, end time.Time) ([]Bar, error) {
	all := make([]Bar, 0, 4096)
	pageToken := ""

	for {
		q := url.Values{}
		q.Set("timeframe", timeframe)
		q.Set("start", start.UTC().Format(time.RFC3339))
		q.Set("end", end.UTC().Format(time.RFC3339))
		q.Set("limit", "10000")
		q.Set("adjustment", "split")
		q.Set("feed", "iex")
		if pageToken != "" {
			q.Set("page_token", pageToken)
		}

		endpoint := alpacaDataBase + "/v2/stocks/" + url.PathEscape(symbol) + "/bars?" + q.Encode()
		req, err := http.NewRequest("GET", endpoint, nil)
		if err != nil {
			return nil, err
		}
		req.Header.Set("APCA-API-KEY-ID", c.APIKey)
		req.Header.Set("APCA-API-SECRET-KEY", c.APISecret)

		resp, err := c.HTTP.Do(req)
		if err != nil {
			return nil, err
		}
		data, err := io.ReadAll(resp.Body)
		resp.Body.Close()
		if err != nil {
			return nil, err
		}
		if resp.StatusCode >= 400 {
			var errResp struct {
				Message string `json:"message"`
			}
			if json.Unmarshal(data, &errResp) == nil && errResp.Message != "" {
				return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, errResp.Message)
			}
			return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(data))
		}

		var br barsResponse
		if err := json.Unmarshal(data, &br); err != nil {
			return nil, err
		}
		all = append(all, br.Bars...)
		if br.NextPageToken == "" {
			break
		}
		pageToken = br.NextPageToken
		if len(all) > 50000 {
			break
		}
	}

	return all, nil
}

// GetAssets returns active US equity assets. Used by the symbol autocomplete.
func (c *AlpacaClient) GetAssets() ([]Asset, error) {
	req, err := http.NewRequest("GET", c.BaseURL+"/v2/assets?status=active&asset_class=us_equity", nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("APCA-API-KEY-ID", c.APIKey)
	req.Header.Set("APCA-API-SECRET-KEY", c.APISecret)
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(data))
	}
	var assets []Asset
	return assets, json.Unmarshal(data, &assets)
}
