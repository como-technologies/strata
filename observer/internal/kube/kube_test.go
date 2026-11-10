package kube

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"errors"
	"math/big"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func TestThePodsFollowTheContinueTokenUntilTheListEnds(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("continue") == "" {
			_, _ = w.Write([]byte(testkit.PodList("page-2", testkit.PodJSON("ns1", "a", "repo/a:1", "repo/a@sha256:aa"))))
			return
		}
		_, _ = w.Write([]byte(testkit.PodList("", testkit.PodJSON("ns2", "b", "repo/b:2", "repo/b@sha256:bb"))))
	})
	got, err := New(cfgOf(a)).Pods(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	want := []Container{
		{"ns1/a", "repo/a:1-init", "repo/a@sha256:aa-init"}, {"ns1/a", "repo/a:1", "repo/a@sha256:aa"}, {"ns1/a", "idle:1", ""},
		{"ns2/b", "repo/b:2-init", "repo/b@sha256:bb-init"}, {"ns2/b", "repo/b:2", "repo/b@sha256:bb"}, {"ns2/b", "idle:1", ""},
	}
	if !slices.Equal(got, want) {
		t.Errorf("containers %+v, want %+v", got, want)
	}
	seen := a.Requests()
	if len(seen) != 2 || seen[0].Query != "limit=500" || seen[1].Query != "continue=page-2&limit=500" {
		t.Fatalf("requests %+v", seen)
	}
	for _, r := range seen {
		if r.Method != http.MethodGet || r.Path != "/api/v1/pods" || r.Auth != "Bearer token-1" {
			t.Errorf("request %+v", r)
		}
	}
}

// The image of the status differs from the image of the spec for a container that the runtime
// rewrote. The match uses the image of the spec, and the container joins its status by name.
func TestAContainerKeepsTheImageOfItsSpecAndJoinsItsStatusByName(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(testkit.PodList("", `{"metadata":{"name":"p","namespace":"n"},
		 "spec":{"containers":[{"name":"one","image":"docker.io/library/nginx:1"},{"name":"two","image":"two:2"}]},
		 "status":{"containerStatuses":[{"name":"two","image":"rewritten/two:2","imageID":"two@sha256:22"},{"name":"one","image":"nginx:1","imageID":"nginx@sha256:11"}]}}`)))
	})
	got, err := New(cfgOf(a)).Pods(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	want := []Container{{"n/p", "docker.io/library/nginx:1", "nginx@sha256:11"}, {"n/p", "two:2", "two@sha256:22"}}
	if !slices.Equal(got, want) {
		t.Errorf("containers %+v, want %+v", got, want)
	}
}

func TestTheTokenIsReadAtEachCall(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte(testkit.PodList(""))) })
	c := New(cfgOf(a))
	if _, err := c.Pods(context.Background()); err != nil {
		t.Fatal(err)
	}
	a.SetToken("token-2")
	if _, err := c.Pods(context.Background()); err != nil {
		t.Fatal(err)
	}
	seen := a.Requests()
	if len(seen) != 2 || seen[0].Auth != "Bearer token-1" || seen[1].Auth != "Bearer token-2" {
		t.Errorf("requests %+v", seen)
	}
}

func TestTheCAIsReadAtEachCall(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte(testkit.PodList(""))) })
	c := New(cfgOf(a))
	if _, err := c.Pods(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(a.CAFile, []byte("not a certificate"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := c.Pods(context.Background()); kindOf(err) != docs.KindKubeConfig {
		t.Errorf("a CA that is not a certificate: %v", err)
	}
}

// cfgOf gives the Config that points at the stand-in.
func cfgOf(a *testkit.KubeAPI) Config {
	return Config{Host: a.Host, TokenFile: a.TokenFile, CAFile: a.CAFile}
}

func kindOf(err error) string {
	var e *Error
	if errors.As(err, &e) {
		return e.Kind
	}
	return "not a *Error: " + errorText(err)
}

func errorText(err error) string {
	if err == nil {
		return "no error"
	}
	return err.Error()
}

func TestAFailureHasAKindAndAStatusAndNeverTheBodyOfTheAnswer(t *testing.T) {
	for _, c := range []struct {
		name    string
		handler func(w http.ResponseWriter, r *http.Request)
		kind    string
		status  int
	}{
		{"a refusal", func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, `{"message":"pods is forbidden: `+testkit.EnvMarker+`"}`, http.StatusForbidden)
		}, docs.KindKubeAPI, 403},
		{"an expired continue token", func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, testkit.EnvMarker, http.StatusGone)
		}, docs.KindKubeAPI, 410},
		{"an answer that is not JSON", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("<html>" + testkit.EnvMarker)) }, docs.KindNotJSON, 0},
		{"a list that stops", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte(`{"items":[{"metadata":`)) }, docs.KindNotJSON, 0},
	} {
		t.Run(c.name, func(t *testing.T) {
			a := testkit.NewKubeAPI(t, c.handler)
			_, err := New(cfgOf(a)).Pods(context.Background())
			var e *Error
			if !errors.As(err, &e) || e.Kind != c.kind || e.Status != c.status {
				t.Fatalf("error %v", err)
			}
			if strings.Contains(err.Error(), testkit.EnvMarker) {
				t.Errorf("the error holds the body: %v", err)
			}
		})
	}
}

func TestAnAddressThatAnswersNothingIsATransportFailure(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(http.ResponseWriter, *http.Request) {})
	cfg := cfgOf(a)
	// The client asks testkit.PortZero, because another process can take the port of a closed stand-in.
	cfg.Host = testkit.PortZero
	_, err := New(cfg).Pods(context.Background())
	if kindOf(err) != docs.KindKubeTransport || strings.Contains(err.Error(), "127.0.0.1") {
		t.Errorf("error %v", err)
	}
}

// anotherCA makes a self-signed certificate that is not the one of the stand-in.
func anotherCA(t *testing.T) []byte {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	template := &x509.Certificate{SerialNumber: big.NewInt(1), Subject: pkix.Name{CommonName: "another"}, NotBefore: time.Now().Add(-time.Hour),
		NotAfter: time.Now().Add(time.Hour), IsCA: true, BasicConstraintsValid: true, KeyUsage: x509.KeyUsageCertSign}
	der, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		t.Fatal(err)
	}
	return pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: der})
}

func TestAServerThatTheCADoesNotSignIsATransportFailure(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte(testkit.PodList(""))) })
	other := filepath.Join(t.TempDir(), "ca.crt")
	if err := os.WriteFile(other, anotherCA(t), 0o600); err != nil {
		t.Fatal(err)
	}
	cfg := cfgOf(a)
	cfg.CAFile = other
	if _, err := New(cfg).Pods(context.Background()); kindOf(err) != docs.KindKubeTransport {
		t.Errorf("error %v", err)
	}
}

func TestACredentialThatIsMissingIsAConfigFailure(t *testing.T) {
	a := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte(testkit.PodList(""))) })
	for name, edit := range map[string]func(*Config){
		"no token file": func(c *Config) { c.TokenFile += ".missing" },
		"no CA file":    func(c *Config) { c.CAFile += ".missing" },
	} {
		cfg := cfgOf(a)
		edit(&cfg)
		if _, err := New(cfg).Pods(context.Background()); kindOf(err) != docs.KindKubeConfig {
			t.Errorf("%s: %v", name, err)
		}
	}
	if err := os.WriteFile(a.TokenFile, []byte(" \n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := New(cfgOf(a)).Pods(context.Background()); kindOf(err) != docs.KindKubeConfig {
		t.Errorf("an empty token: %v", err)
	}
	if n := len(a.Requests()); n != 0 {
		t.Errorf("%d requests went out with a credential that was missing", n)
	}
}

func TestACallThatTakesTooLongIsATimeoutAndACancelIsNotAFailureOfTheAPI(t *testing.T) {
	release := make(chan struct{})
	a := testkit.NewKubeAPI(t, func(http.ResponseWriter, *http.Request) { <-release })
	t.Cleanup(func() { close(release) })
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	if _, err := New(cfgOf(a)).Pods(ctx); kindOf(err) != docs.KindTimeout {
		t.Errorf("a deadline: %v", err)
	}
	ctx, stop := context.WithCancel(context.Background())
	stop()
	if _, err := New(cfgOf(a)).Pods(ctx); !errors.Is(err, context.Canceled) {
		t.Errorf("a cancel: %v", err)
	}
}

func TestInClusterReadsTheVariablesOfTheKubelet(t *testing.T) {
	env := map[string]string{hostEnv: "10.96.0.1", portEnv: "443"}
	cfg, err := InCluster(func(k string) string { return env[k] }, "/volume")
	if err != nil || cfg.Host != "10.96.0.1:443" || cfg.TokenFile != "/volume/token" || cfg.CAFile != "/volume/ca.crt" {
		t.Errorf("config %+v, error %v", cfg, err)
	}
	env[hostEnv] = "fd00::1"
	if cfg, _ := InCluster(func(k string) string { return env[k] }, "/volume"); cfg.Host != "[fd00::1]:443" {
		t.Errorf("an IPv6 address: %q", cfg.Host)
	}
	for _, drop := range []string{hostEnv, portEnv} {
		short := map[string]string{hostEnv: "10.96.0.1", portEnv: "443"}
		delete(short, drop)
		if _, err := InCluster(func(k string) string { return short[k] }, "/volume"); kindOf(err) != docs.KindKubeConfig {
			t.Errorf("without %s: %v", drop, err)
		}
	}
}
