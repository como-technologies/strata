package api

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"time"
)

// healthTimeout is the time limit of one health check.
const healthTimeout = 3 * time.Second

// DefaultListen is the address that the server takes when no flag names another one, and that the
// health check asks.
const DefaultListen = "127.0.0.1:8080"

// Health makes a GET to the health route of the server at addr, and gives nil when the answer is 200.
// It is the one HTTP client of the observer, and it calls a loopback address only.
func Health(ctx context.Context, addr string) error {
	if err := checkLoopback(addr); err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(ctx, healthTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, "http://"+addr+patternHealth, nil)
	if err != nil {
		return err
	}
	client := &http.Client{
		Transport:     &http.Transport{}, // no proxy
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}
	resp, err := client.Do(req)
	if err != nil {
		return errors.New("the server does not answer")
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 1<<10))
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("the health route gave the status %d", resp.StatusCode)
	}
	return nil
}
