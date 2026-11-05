package testkit

import (
	"encoding/pem"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
)

// EnvMarker stands for a value that must not leave the process: the environment of a container, a label,
// and the message of a state. The pods of the stand-in hold it, and no route, log line, or file of state
// can.
const EnvMarker = "ENV-MARKER-VALUE"

// KubeRequest is a request that the stand-in saw.
type KubeRequest struct {
	Method, Path, Query, Auth string
}

// KubeAPI is a stand-in for the API server of Kubernetes. It serves TLS with a certificate of its own, and
// it writes the files that a pod has: the token and the CA. It lists each request that it saw.
type KubeAPI struct {
	// Host is host:port. TokenFile and CAFile are the files of the volume of a pod.
	Host, TokenFile, CAFile string

	t    *testing.T
	srv  *httptest.Server
	mu   sync.Mutex
	seen []KubeRequest
}

// NewKubeAPI starts the stand-in. handler answers each request. The token starts as token-1.
func NewKubeAPI(t *testing.T, handler func(w http.ResponseWriter, r *http.Request)) *KubeAPI {
	t.Helper()
	a := &KubeAPI{t: t}
	a.srv = httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		a.mu.Lock()
		a.seen = append(a.seen, KubeRequest{r.Method, r.URL.Path, r.URL.RawQuery, r.Header.Get("Authorization")})
		a.mu.Unlock()
		handler(w, r)
	}))
	t.Cleanup(a.srv.Close)
	dir := t.TempDir()
	a.Host, a.TokenFile, a.CAFile = a.srv.Listener.Addr().String(), filepath.Join(dir, "token"), filepath.Join(dir, "ca.crt")
	ca := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: a.srv.Certificate().Raw})
	if err := os.WriteFile(a.CAFile, ca, 0o600); err != nil {
		t.Fatal(err)
	}
	a.SetToken("token-1")
	return a
}

// SetToken replaces the token file, as the kubelet does when it rotates the token.
func (a *KubeAPI) SetToken(text string) {
	a.t.Helper()
	if err := os.WriteFile(a.TokenFile, []byte(text+"\n"), 0o600); err != nil {
		a.t.Fatal(err)
	}
}

// Close stops the server.
func (a *KubeAPI) Close() {
	a.srv.Close()
}

// Requests lists the requests that the stand-in saw, in order.
func (a *KubeAPI) Requests() []KubeRequest {
	a.mu.Lock()
	defer a.mu.Unlock()
	return slices.Clone(a.seen)
}

// Env gives the variables of a pod that points at the stand-in, and the flag for the directory with the
// token and the CA is the directory of the two files.
func (a *KubeAPI) Env() map[string]string {
	host, port, _ := strings.Cut(a.Host, ":")
	return map[string]string{"KUBERNETES_SERVICE_HOST": host, "KUBERNETES_SERVICE_PORT": port}
}

// Dir is the directory that holds the token and the CA.
func (a *KubeAPI) Dir() string {
	return filepath.Dir(a.TokenFile)
}

// PodJSON makes the JSON of one pod with an init container, a container that runs the image with the
// image id, and a container that never started. The pod, the labels, the environments, and the state
// hold EnvMarker, as a real pod can hold a secret there.
func PodJSON(ns, name, image, imageID string) string {
	return `{"metadata":{"name":"` + name + `","namespace":"` + ns + `","labels":{"x":"` + EnvMarker + `"}},
	 "spec":{"nodeName":"node-1","initContainers":[{"name":"init","image":"` + image + `-init","env":[{"name":"K","value":"` + EnvMarker + `"}]}],
	  "containers":[{"name":"main","image":"` + image + `","env":[{"name":"PASSWORD","value":"` + EnvMarker + `"}]},{"name":"idle","image":"idle:1"}]},
	 "status":{"initContainerStatuses":[{"name":"init","image":"rewritten","imageID":"` + imageID + `-init"}],
	  "containerStatuses":[{"name":"main","image":"rewritten","imageID":"` + imageID + `","state":{"waiting":{"message":"` + EnvMarker + `"}}}]}}`
}

// PodList makes the JSON of a page of the list. next is the continue token, empty on the last page.
func PodList(next string, items ...string) string {
	return `{"metadata":{"continue":"` + next + `","resourceVersion":"1"},"items":[` + strings.Join(items, ",") + `]}`
}
