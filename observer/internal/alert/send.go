package alert

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"maps"
	"net/http"
	"net/url"
	"os"
	"slices"
	"strings"
	"time"
)

// Sender sends the text of a message to a channel. The Webhook is the sender of the program, and a test
// gives a fake.
type Sender interface {
	Send(ctx context.Context, channel, text string) error
}

// The kinds of a failed send.
const (
	KindAddress   = "address"
	KindTransport = "transport"
	KindStatus    = "status"
)

// SendError is a failed send. It holds the name of the channel, a kind, and the status of the answer. It
// never holds the address or the text: the address of a webhook is a secret, and Go puts it in the
// error of a failed request.
type SendError struct {
	Channel string
	Kind    string
	Status  int
}

func (e *SendError) Error() string {
	return fmt.Sprintf("the send to the channel %q failed: %s %d", e.Channel, e.Kind, e.Status)
}

// faultFields gives the fields of a log line for a failed send: the kind and the status.
func faultFields(err error) []any {
	var se *SendError
	if errors.As(err, &se) {
		return []any{"kind", se.Kind, "status", se.Status}
	}
	return []any{"kind", KindTransport}
}

// sendTimeout is the time limit of one send.
const sendTimeout = 10 * time.Second

// replyLimit is the most bytes that the observer reads of an answer. It reads them to reuse the
// connection, and it keeps none.
const replyLimit = 64 << 10

// Webhook sends a message as a POST of {"text": "..."}, which Slack and Teams both read. It reads the
// address from the file of the channel at each send, because the kubelet renews a Secret. It follows no
// redirect, and it refuses an address that is not https unless AllowHTTP is set, which the tests use.
type Webhook struct {
	channels  map[string]Channel
	allowHTTP bool
	client    *http.Client
}

// NewWebhook makes the sender for the channels of the rules.
func NewWebhook(channels map[string]Channel, allowHTTP bool) *Webhook {
	return &Webhook{channels: channels, allowHTTP: allowHTTP, client: &http.Client{
		Timeout:       sendTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}}
}

// address reads the address of a channel. A fault gives a fixed text and never the address.
func (w *Webhook) address(channel string) (string, error) {
	c, ok := w.channels[channel]
	if !ok {
		return "", errors.New("the channel is not in the file of the rules")
	}
	raw, err := os.ReadFile(c.URLFile)
	if err != nil {
		return "", errors.New("the file of the address cannot be read")
	}
	addr := strings.TrimSpace(string(raw))
	u, err := url.Parse(addr)
	switch {
	case err != nil || u.Host == "":
		return "", errors.New("the file of the address does not hold an address")
	case u.Scheme == "https" || u.Scheme == "http" && w.allowHTTP:
		return addr, nil
	}
	return "", errors.New("the address must use https")
}

// Check reads the address of each channel and gives the first fault. `serve` calls it at the start, so
// a wrong Secret stops the start and not the first alert.
func (w *Webhook) Check() error {
	for _, name := range slices.Sorted(maps.Keys(w.channels)) {
		if _, err := w.address(name); err != nil {
			return fmt.Errorf("channel %q: %w", name, err)
		}
	}
	return nil
}

// Send posts the text. A status that is not 2xx is a failure, also a redirect.
func (w *Webhook) Send(ctx context.Context, channel, text string) error {
	addr, err := w.address(channel)
	if err != nil {
		return &SendError{Channel: channel, Kind: KindAddress}
	}
	body, err := json.Marshal(struct {
		Text string `json:"text"`
	}{text})
	if err != nil {
		return &SendError{Channel: channel, Kind: KindTransport}
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, addr, bytes.NewReader(body))
	if err != nil {
		return &SendError{Channel: channel, Kind: KindAddress}
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := w.client.Do(req)
	if err != nil {
		return &SendError{Channel: channel, Kind: KindTransport}
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, replyLimit)) // the answer is not kept
	if resp.StatusCode < 200 || resp.StatusCode > 299 {
		return &SendError{Channel: channel, Kind: KindStatus, Status: resp.StatusCode}
	}
	return nil
}
