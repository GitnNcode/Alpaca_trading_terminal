package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

const alpacaDataBase = "https://data.alpaca.markets"

type AlpacaClient struct {
	BaseURL   string
	APIKey    string
	APISecret string
	HTTP      *http.Client
}

type Position struct {
	Symbol         string `json:"symbol"`
	Qty            string `json:"qty"`
	AvgEntryPrice  string `json:"avg_entry_price"`
	CurrentPrice   string `json:"current_price"`
	MarketValue    string `json:"market_value"`
	UnrealizedPL   string `json:"unrealized_pl"`
	UnrealizedPLPC string `json:"unrealized_plpc"`
	Side           string `json:"side"`
}

type Account struct {
	BuyingPower    string `json:"buying_power"`
	Cash           string `json:"cash"`
	PortfolioValue string `json:"portfolio_value"`
	Equity         string `json:"equity"`
}

type OrderRequest struct {
	Symbol      string `json:"symbol"`
	Qty         string `json:"qty,omitempty"`
	Side        string `json:"side"`
	Type        string `json:"type"`
	TimeInForce string `json:"time_in_force"`
	LimitPrice  string `json:"limit_price,omitempty"`
}

type Order struct {
	ID             string    `json:"id"`
	Symbol         string    `json:"symbol"`
	Side           string    `json:"side"`
	Type           string    `json:"type"`
	Qty            string    `json:"qty"`
	LimitPrice     string    `json:"limit_price"`
	Status         string    `json:"status"`
	FilledQty      string    `json:"filled_qty"`
	FilledAvgPrice string    `json:"filled_avg_price"`
	CreatedAt      time.Time `json:"created_at"`
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
		HTTP:      &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *AlpacaClient) do(method, path string, body interface{}) ([]byte, error) {
	var bodyReader io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		bodyReader = bytes.NewReader(b)
	}

	req, err := http.NewRequest(method, c.BaseURL+path, bodyReader)
	if err != nil {
		return nil, err
	}

	req.Header.Set("APCA-API-KEY-ID", c.APIKey)
	req.Header.Set("APCA-API-SECRET-KEY", c.APISecret)
	req.Header.Set("Content-Type", "application/json")

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
		var errResp struct {
			Message string `json:"message"`
		}
		if json.Unmarshal(data, &errResp) == nil && errResp.Message != "" {
			return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, errResp.Message)
		}
		return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(data))
	}

	return data, nil
}

func (c *AlpacaClient) GetPositions() ([]Position, error) {
	data, err := c.do("GET", "/v2/positions", nil)
	if err != nil {
		return nil, err
	}
	var positions []Position
	return positions, json.Unmarshal(data, &positions)
}

func (c *AlpacaClient) GetAccount() (Account, error) {
	data, err := c.do("GET", "/v2/account", nil)
	if err != nil {
		return Account{}, err
	}
	var account Account
	return account, json.Unmarshal(data, &account)
}

func (c *AlpacaClient) PlaceOrder(req OrderRequest) (Order, error) {
	data, err := c.do("POST", "/v2/orders", req)
	if err != nil {
		return Order{}, err
	}
	var order Order
	return order, json.Unmarshal(data, &order)
}

func (c *AlpacaClient) GetOrders() ([]Order, error) {
	data, err := c.do("GET", "/v2/orders?status=open&limit=50", nil)
	if err != nil {
		return nil, err
	}
	var orders []Order
	return orders, json.Unmarshal(data, &orders)
}

func (c *AlpacaClient) CancelOrder(orderID string) error {
	_, err := c.do("DELETE", "/v2/orders/"+orderID, nil)
	return err
}

type Activity struct {
	ID              string    `json:"id"`
	ActivityType    string    `json:"activity_type"`
	TransactionTime time.Time `json:"transaction_time"` // trade activities
	Date            string    `json:"date"`             // non-trade activities (YYYY-MM-DD)
	// trade activity fields
	Type    string `json:"type"` // "fill" or "partial_fill"
	Price   string `json:"price"`
	Qty     string `json:"qty"`
	CumQty  string `json:"cum_qty"`
	Side    string `json:"side"`
	Symbol  string `json:"symbol"`
	OrderID string `json:"order_id"`
	// non-trade activity fields
	NetAmount      string `json:"net_amount"`
	PerShareAmount string `json:"per_share_amount"`
	Description    string `json:"description"`
}

func (c *AlpacaClient) GetActivities() ([]Activity, error) {
	data, err := c.do("GET", "/v2/account/activities?page_size=100&direction=desc", nil)
	if err != nil {
		return nil, err
	}
	var activities []Activity
	return activities, json.Unmarshal(data, &activities)
}

type Asset struct {
	Symbol   string `json:"symbol"`
	Name     string `json:"name"`
	Status   string `json:"status"`
	Tradable bool   `json:"tradable"`
}

func (c *AlpacaClient) GetAssets() ([]Asset, error) {
	data, err := c.do("GET", "/v2/assets?status=active&asset_class=us_equity", nil)
	if err != nil {
		return nil, err
	}
	var assets []Asset
	return assets, json.Unmarshal(data, &assets)
}

func (c *AlpacaClient) GetClosedOrders() ([]Order, error) {
	data, err := c.do("GET", "/v2/orders?status=closed&limit=100&direction=desc", nil)
	if err != nil {
		return nil, err
	}
	var orders []Order
	return orders, json.Unmarshal(data, &orders)
}

// Bar is one OHLCV candle from Alpaca's market-data API.
type Bar struct {
	Time   time.Time `json:"t"`
	Open   float64   `json:"o"`
	High   float64   `json:"h"`
	Low    float64   `json:"l"`
	Close  float64   `json:"c"`
	Volume int64     `json:"v"`
}

type barsResponse struct {
	Bars          []Bar  `json:"bars"`
	NextPageToken string `json:"next_page_token"`
}

// GetBars fetches OHLCV bars from data.alpaca.markets for the given symbol and
// timeframe ("1Min", "5Min", "15Min", "1Hour", "1Day", "1Week", "1Month").
// Uses the IEX feed (works on free/paper subscriptions) and split-adjusted prices.
func (c *AlpacaClient) GetBars(symbol, timeframe string, start, end time.Time) ([]Bar, error) {
	all := make([]Bar, 0, 1024)
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
		// Safety cap so a broken pagination loop never spins forever
		if len(all) > 50000 {
			break
		}
	}

	return all, nil
}
