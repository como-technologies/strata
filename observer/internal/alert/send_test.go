package alert

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

// hook makes a webhook with one channel, platform-oncall, whose address file holds addr. When srv is a
// TLS server, the webhook trusts its certificate and keeps its own rules about redirects.
func hook(t *testing.T, addr string, allowHTTP bool, srv *httptest.Server) (*Webhook, string) {
	t.Helper()
	file := filepath.Join(t.TempDir(), "platform-oncall")
	if err := os.WriteFile(file, []byte(addr+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	w := NewWebhook(map[string]Channel{"platform-oncall": {URLFile: file}}, allowHTTP)
	if srv != nil {
		w.client.Transport = srv.Client().Transport
	}
	return w, file
}

func TestAMessageIsAPostOfJSONWithTheTextToAnHttpsAddress(t *testing.T) {
	r := testkit.NewTLSReceiver(t)
	w, _ := hook(t, r.URL+"/services/T0/B0/TOKEN", false, r.Server)
	if err := w.Send(context.Background(), "platform-oncall", "strata · quarry-1 · Drift"); err != nil {
		t.Fatal(err)
	}
	var sent struct{ Text string }
	got := r.Got()
	if len(got) != 1 || got[0].Method != http.MethodPost || json.Unmarshal([]byte(got[0].Body), &sent) != nil || sent.Text != "strata · quarry-1 · Drift" || got[0].ContentType != "application/json" {
		t.Errorf("requests %q", got)
	}
}

func TestTheAddressIsReadAgainAtEachSendBecauseTheKubeletRenewsASecret(t *testing.T) {
	first, second := testkit.NewTLSReceiver(t), testkit.NewTLSReceiver(t)
	w, file := hook(t, first.URL, false, first.Server)
	if err := w.Send(context.Background(), "platform-oncall", "one"); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, []byte(second.URL), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := w.Send(context.Background(), "platform-oncall", "two"); err != nil {
		t.Fatal(err)
	}
	if first.Count() != 1 || second.Count() != 1 {
		t.Errorf("the first address got %d requests, the second %d", first.Count(), second.Count())
	}
}

func TestAnAddressThatIsNotHttpsGetsNoRequestUnlessTheTestsAllowIt(t *testing.T) {
	r := testkit.NewReceiver(t)
	w, _ := hook(t, r.URL, false, nil)
	err := w.Send(context.Background(), "platform-oncall", "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindAddress || r.Count() != 0 {
		t.Fatalf("error %v, %d requests", err, r.Count())
	}
	if checked := w.Check(); checked == nil || !strings.Contains(checked.Error(), "https") {
		t.Errorf("check %v", checked)
	}
	allowed, _ := hook(t, r.URL, true, nil)
	if err := allowed.Send(context.Background(), "platform-oncall", "x"); err != nil || r.Count() != 1 {
		t.Errorf("error %v, %d requests", err, r.Count())
	}
}

func TestARedirectIsNotFollowedAndIsAFailure(t *testing.T) {
	target := testkit.NewTLSReceiver(t)
	redirect := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		http.Redirect(w, req, target.URL, http.StatusFound)
	}))
	t.Cleanup(redirect.Close)
	w, _ := hook(t, redirect.URL, false, redirect)
	err := w.Send(context.Background(), "platform-oncall", "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindStatus || se.Status != http.StatusFound || target.Count() != 0 {
		t.Errorf("error %v, %d requests to the target", err, target.Count())
	}
}

func TestAStatusThatIsNotSuccessIsAFailureThatNamesTheChannelAndTheStatus(t *testing.T) {
	r := testkit.NewTLSReceiver(t)
	r.SetStatus(http.StatusServiceUnavailable)
	w, _ := hook(t, r.URL, false, r.Server)
	err := w.Send(context.Background(), "platform-oncall", "x")
	var se *SendError
	if !errors.As(err, &se) || se.Channel != "platform-oncall" || se.Status != 503 || se.Kind != KindStatus {
		t.Errorf("error %v", err)
	}
}

func TestAFailedSendNamesNoAddress(t *testing.T) {
	// The send goes to testkit.PortZero, because another process can take the port of a closed receiver.
	w, _ := hook(t, "https://"+testkit.PortZero+"/services/TOKEN-SECRET", false, nil)
	err := w.Send(context.Background(), "platform-oncall", "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindTransport {
		t.Fatalf("error %v", err)
	}
	for _, shown := range []string{err.Error(), fmt.Sprintf("%+v %#v %v", err, err, faultFields(err))} {
		if strings.Contains(shown, "TOKEN-SECRET") || strings.Contains(shown, "127.0.0.1") {
			t.Errorf("the error shows the address: %s", shown)
		}
	}
}

func TestACheckGivesTheFirstChannelWhoseAddressCannotBeUsedAndNamesNoAddress(t *testing.T) {
	dir := t.TempDir()
	write := func(name, text string) string {
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
			t.Fatal(err)
		}
		return path
	}
	cases := []struct {
		name string
		file string
		want string
	}{
		{"a file that is absent", filepath.Join(dir, "absent"), "cannot be read"},
		{"a file that holds no address", write("empty", "  \n"), "does not hold an address"},
		{"a file that holds a plain word", write("word", "SECRET-WORD"), "does not hold an address"},
		{"an address that is not https", write("plain", "http://hooks.example/SECRET-PATH"), "must use https"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := NewWebhook(map[string]Channel{"platform-oncall": {URLFile: c.file}}, false).Check()
			if err == nil || !strings.Contains(err.Error(), c.want) || !strings.Contains(err.Error(), `"platform-oncall"`) || strings.Contains(err.Error(), "SECRET") {
				t.Errorf("check %v", err)
			}
		})
	}
	good := write("good", "https://hooks.example/SECRET-PATH\n")
	if err := NewWebhook(map[string]Channel{"platform-oncall": {URLFile: good}}, false).Check(); err != nil {
		t.Errorf("check %v", err)
	}
}

func TestAChannelThatTheRulesLackFailsWithTheKindAddress(t *testing.T) {
	w := NewWebhook(map[string]Channel{}, false)
	var se *SendError
	if err := w.Send(context.Background(), "nobody", "x"); !errors.As(err, &se) || se.Kind != KindAddress {
		t.Errorf("error %v", err)
	}
}
