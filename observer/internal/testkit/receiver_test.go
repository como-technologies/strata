package testkit

import (
	"net/http"
	"slices"
	"strings"
	"testing"
)

func TestAReceiverKeepsEachRequestInOrderAndAnswersWithTheStatusThatWasSet(t *testing.T) {
	for name, start := range map[string]func(*testing.T) *Receiver{"http": NewReceiver, "https": NewTLSReceiver} {
		r := start(t)
		send := func(body string) int {
			t.Helper()
			resp, err := r.Client().Post(r.URL, "application/json", strings.NewReader(body))
			if err != nil {
				t.Fatalf("%s: %v", name, err)
			}
			_ = resp.Body.Close()
			return resp.StatusCode
		}
		first := send("barney")
		r.SetStatus(http.StatusServiceUnavailable)
		second := send("fred")
		want := []Received{{http.MethodPost, "application/json", "barney"}, {http.MethodPost, "application/json", "fred"}}
		if first != http.StatusOK || second != http.StatusServiceUnavailable || r.Count() != 2 || !slices.Equal(r.Got(), want) || !slices.Equal(r.Bodies(), []string{"barney", "fred"}) {
			t.Errorf("%s: statuses %d %d, got %q", name, first, second, r.Got())
		}
	}
}
