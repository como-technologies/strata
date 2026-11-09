// Package kube is the one place that calls the Kubernetes API. The serving observer reads the pods of
// the cluster that it runs in, and the seed writes the Secrets of the records: see secrets.go.
//
// The read of the pods sends one request, GET on the list of pods, with paging, and no other. The role
// of the observer holds `get` and `list` on `pods` and no other verb. The types decode the name and the namespace of
// a pod. They decode the name and the image of each container, and the image id of its status. They
// have no field for the environment of a container, so the environment never enters the process.
package kube

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// ServiceDir is the directory of the projected volume that holds the token and the CA of the
// cluster. A pod that the kubelet gives a service account token has the same files at this path.
const ServiceDir = "/var/run/secrets/kubernetes.io/serviceaccount"

// The names of the files in the directory of the volume, and of the variables that the kubelet sets.
const (
	tokenFile = "token"
	caFile    = "ca.crt"
	hostEnv   = "KUBERNETES_SERVICE_HOST"
	portEnv   = "KUBERNETES_SERVICE_PORT"
)

// pageSize is the number of pods in one answer, and requestTimeout limits one request. maxBody limits
// the size of one answer: a page of 500 pods with their environment is a few megabytes.
const (
	pageSize       = 500
	requestTimeout = 30 * time.Second
	maxBody        = 128 << 20
)

// Error is a failed read of the pods. It holds a kind and a status only, as a failed run of strata
// does. The answer of the API server can name an object.
type Error struct {
	Kind   string
	Status int
}

func (e *Error) Error() string {
	if e.Status != 0 {
		return fmt.Sprintf("kube: %s, status %d", e.Kind, e.Status)
	}
	return "kube: " + e.Kind
}

// Container is one container of a pod, with the image that the pod names and the image id that the
// node reports. Pod is namespace/name: it counts the pods of an image and never leaves the process.
// ImageID is empty for a container that never started.
type Container struct {
	Pod     string
	Image   string
	ImageID string
}

// Config says where the API server is and where the credentials are.
type Config struct {
	// Host is the address of the API server, as host:port.
	Host string
	// TokenFile and CAFile are files of the projected volume. The token rotates, so every call reads
	// both again.
	TokenFile, CAFile string
}

// InCluster makes the Config of a pod. It reads the variables that the kubelet sets, and it names the
// files of the volume in dir. The program gives os.Getenv as getenv and ServiceDir as dir. It gives
// the kind kube-config when the variables are absent.
func InCluster(getenv func(string) string, dir string) (Config, error) {
	host, port := getenv(hostEnv), getenv(portEnv)
	if host == "" || port == "" {
		return Config{}, &Error{Kind: docs.KindKubeConfig}
	}
	return Config{Host: net.JoinHostPort(host, port), TokenFile: filepath.Join(dir, tokenFile), CAFile: filepath.Join(dir, caFile)}, nil
}

// Client reads the pods.
type Client struct {
	cfg Config
}

// New makes a Client. It reads no file and makes no request.
func New(cfg Config) *Client {
	return &Client{cfg: cfg}
}

// The wire types. They list the fields that the observer reads and no other.
type podList struct {
	Metadata struct {
		Continue string `json:"continue"`
	} `json:"metadata"`
	Items []podItem `json:"items"`
}

type podItem struct {
	Metadata struct {
		Name      string `json:"name"`
		Namespace string `json:"namespace"`
	} `json:"metadata"`
	Spec struct {
		Containers     []specContainer `json:"containers"`
		InitContainers []specContainer `json:"initContainers"`
	} `json:"spec"`
	Status struct {
		ContainerStatuses     []statusContainer `json:"containerStatuses"`
		InitContainerStatuses []statusContainer `json:"initContainerStatuses"`
	} `json:"status"`
}

type specContainer struct {
	Name  string `json:"name"`
	Image string `json:"image"`
}

type statusContainer struct {
	Name    string `json:"name"`
	ImageID string `json:"imageID"`
}

// containers joins each container of the spec with the status of the same name. The image of the
// status is not read: the runtime can rewrite it, so the match uses the image of the spec.
func (p podItem) containers() []Container {
	pod := p.Metadata.Namespace + "/" + p.Metadata.Name
	var out []Container
	join := func(specs []specContainer, statuses []statusContainer) {
		ids := make(map[string]string, len(statuses))
		for _, s := range statuses {
			ids[s.Name] = s.ImageID
		}
		for _, c := range specs {
			if c.Image != "" {
				out = append(out, Container{Pod: pod, Image: c.Image, ImageID: ids[c.Name]})
			}
		}
	}
	join(p.Spec.InitContainers, p.Status.InitContainerStatuses)
	join(p.Spec.Containers, p.Status.ContainerStatuses)
	return out
}

// Pods lists the containers of every pod of the cluster. It follows `continue` until the list ends.
// The token and the CA are read at each call. A failure gives an *Error with a kind and a status.
func (c *Client) Pods(ctx context.Context) ([]Container, error) {
	client, err := c.httpClient()
	if err != nil {
		return nil, err
	}
	defer client.CloseIdleConnections()
	token, err := readTrimmed(c.cfg.TokenFile)
	if err != nil {
		return nil, err
	}
	var out []Container
	var next string
	for {
		list, err := c.page(ctx, client, token, next)
		if err != nil {
			return nil, err
		}
		for _, item := range list.Items {
			out = append(out, item.containers()...)
		}
		if next = list.Metadata.Continue; next == "" {
			return out, nil
		}
	}
}

func readTrimmed(path string) (string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return "", &Error{Kind: docs.KindKubeConfig}
	}
	text := strings.TrimSpace(string(raw))
	if text == "" {
		return "", &Error{Kind: docs.KindKubeConfig}
	}
	return text, nil
}

// httpClient makes a client that trusts the CA of the file and keeps no connection. A rotated CA
// takes effect at the next call.
func (c *Client) httpClient() (*http.Client, error) {
	raw, err := os.ReadFile(c.cfg.CAFile)
	if err != nil {
		return nil, &Error{Kind: docs.KindKubeConfig}
	}
	pool := x509.NewCertPool()
	if !pool.AppendCertsFromPEM(raw) {
		return nil, &Error{Kind: docs.KindKubeConfig}
	}
	transport := &http.Transport{
		TLSClientConfig:   &tls.Config{RootCAs: pool, MinVersion: tls.VersionTLS12},
		DisableKeepAlives: true,
	}
	return &http.Client{Transport: transport}, nil
}

// page reads one page of the list. A body that is not a list is the kind not-json.
func (c *Client) page(ctx context.Context, client *http.Client, token, next string) (podList, error) {
	query := url.Values{"limit": {strconv.Itoa(pageSize)}}
	if next != "" {
		query.Set("continue", next)
	}
	ctx, cancel := context.WithTimeout(ctx, requestTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, "https://"+c.cfg.Host+"/api/v1/pods?"+query.Encode(), nil)
	if err != nil {
		return podList{}, &Error{Kind: docs.KindKubeConfig}
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/json")
	resp, err := client.Do(req)
	if err != nil {
		return podList{}, transportError(ctx)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return podList{}, &Error{Kind: docs.KindKubeAPI, Status: resp.StatusCode}
	}
	var list podList
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxBody)).Decode(&list); err != nil {
		if ctx.Err() != nil {
			return podList{}, transportError(ctx)
		}
		return podList{}, &Error{Kind: docs.KindNotJSON}
	}
	return list, nil
}

// transportError names a request that got no answer. The text of the error can hold the address of the
// server, so only the kind leaves this function.
func transportError(ctx context.Context) error {
	if ctx.Err() == context.DeadlineExceeded {
		return &Error{Kind: docs.KindTimeout}
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	return &Error{Kind: docs.KindKubeTransport}
}
