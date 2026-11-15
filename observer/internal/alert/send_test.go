package alert

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

// platformOncall is the channel of the webhooks and of the rules of these tests.
const platformOncall = "platform-oncall"

// hook makes a webhook with one channel, platformOncall, whose address file holds addr. When srv is a
// TLS server, the webhook trusts its certificate and keeps its own rules about redirects.
func hook(t *testing.T, addr string, allowHTTP bool, srv *httptest.Server) (*Webhook, string) {
	t.Helper()
	file := testkit.AddressFile(t, platformOncall, addr+"\n")
	w := webhookOf(file, allowHTTP)
	if srv != nil {
		w.client.Transport = srv.Client().Transport
	}
	return w, file
}

// webhookOf makes a webhook with one channel, platformOncall, whose address is in the file.
func webhookOf(file string, allowHTTP bool) *Webhook {
	return NewWebhook(map[string]Channel{platformOncall: {URLFile: file}}, allowHTTP)
}

func TestAMessageIsAPostOfJSONWithTheTextToAnHttpsAddress(t *testing.T) {
	r := testkit.NewTLSReceiver(t)
	w, _ := hook(t, r.URL+"/services/T0/B0/TOKEN", false, r.Server)
	const text = testkit.QuarryDrift
	if err := w.Send(context.Background(), platformOncall, text); err != nil {
		t.Fatal(err)
	}
	var sent struct{ Text string }
	got := r.Got()
	if len(got) != 1 || got[0].Method != http.MethodPost || json.Unmarshal([]byte(got[0].Body), &sent) != nil || sent.Text != text || got[0].ContentType != testkit.JSONType {
		t.Errorf("requests %q", got)
	}
}

func TestTheAddressIsReadAgainAtEachSendBecauseTheKubeletRenewsASecret(t *testing.T) {
	first, second := testkit.NewTLSReceiver(t), testkit.NewTLSReceiver(t)
	w, file := hook(t, first.URL, false, first.Server)
	if err := w.Send(context.Background(), platformOncall, "one"); err != nil {
		t.Fatal(err)
	}
	testkit.WriteFile(t, file, second.URL)
	if err := w.Send(context.Background(), platformOncall, "two"); err != nil {
		t.Fatal(err)
	}
	if first.Count() != 1 || second.Count() != 1 {
		t.Errorf("the first address got %d requests, the second %d", first.Count(), second.Count())
	}
}

func TestAnAddressThatIsNotHttpsGetsNoRequestUnlessTheTestsAllowIt(t *testing.T) {
	r := testkit.NewReceiver(t)
	w, _ := hook(t, r.URL, false, nil)
	err := w.Send(context.Background(), platformOncall, "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindAddress || r.Count() != 0 {
		t.Fatalf(errorAndRequests, err, r.Count())
	}
	if checked := w.Check(); checked == nil || !strings.Contains(checked.Error(), "https") {
		t.Errorf("check %v", checked)
	}
	allowed, _ := hook(t, r.URL, true, nil)
	if err := allowed.Send(context.Background(), platformOncall, "x"); err != nil || r.Count() != 1 {
		t.Errorf(errorAndRequests, err, r.Count())
	}
}

// errorAndRequests is the failure of a check of the error of a send and of the requests that the receiver got.
const errorAndRequests = "error %v, %d requests"

func TestARedirectIsNotFollowedAndIsAFailure(t *testing.T) {
	target := testkit.NewTLSReceiver(t)
	redirect := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		http.Redirect(w, req, target.URL, http.StatusFound)
	}))
	t.Cleanup(redirect.Close)
	w, _ := hook(t, redirect.URL, false, redirect)
	err := w.Send(context.Background(), platformOncall, "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindStatus || se.Status != http.StatusFound || target.Count() != 0 {
		t.Errorf(errorAndRequests+" to the target", err, target.Count())
	}
}

func TestAStatusThatIsNotSuccessIsAFailureThatNamesTheChannelAndTheStatus(t *testing.T) {
	r := testkit.NewTLSReceiver(t)
	r.SetStatus(http.StatusServiceUnavailable)
	w, _ := hook(t, r.URL, false, r.Server)
	err := w.Send(context.Background(), platformOncall, "x")
	var se *SendError
	if !errors.As(err, &se) || se.Channel != platformOncall || se.Status != 503 || se.Kind != KindStatus {
		t.Errorf("error %v", err)
	}
}

func TestAFailedSendNamesNoAddress(t *testing.T) {
	// The send goes to testkit.PortZero, because another process can take the port of a closed receiver.
	w, _ := hook(t, "https://"+testkit.PortZero+"/services/"+testkit.AddressMarker, false, nil)
	err := w.Send(context.Background(), platformOncall, "x")
	var se *SendError
	if !errors.As(err, &se) || se.Kind != KindTransport {
		t.Fatalf("error %v", err)
	}
	for _, shown := range []string{err.Error(), fmt.Sprintf("%+v %#v %v", err, err, faultFields(err))} {
		testkit.HoldsNone(t, "the error", shown, testkit.AddressMarker, "127.0.0.1")
	}
}

func TestACheckGivesTheFirstChannelWhoseAddressCannotBeUsedAndNamesNoAddress(t *testing.T) {
	const noAddress = "does not hold an address"
	dir := t.TempDir()
	cases := []struct {
		name string
		file string
		want string
	}{
		{"a file that is absent", filepath.Join(dir, "absent"), "cannot be read"},
		{"a file that holds no address", testkit.WriteIn(t, dir, "empty", "  \n"), noAddress},
		{"a file that holds a plain word", testkit.WriteIn(t, dir, "word", "SECRET-WORD"), noAddress},
		{"an address that is not https", testkit.WriteIn(t, dir, "plain", "http://"+testkit.HooksExample+"SECRET-PATH"), "must use https"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := webhookOf(c.file, false).Check()
			if err == nil || !strings.Contains(err.Error(), c.want) || !strings.Contains(err.Error(), strconv.Quote(platformOncall)) || strings.Contains(err.Error(), "SECRET") {
				t.Errorf("check %v", err)
			}
		})
	}
	good, _ := hook(t, "https://"+testkit.HooksExample+"SECRET-PATH", false, nil)
	if err := good.Check(); err != nil {
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
