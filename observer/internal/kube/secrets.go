package kube

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// The Secrets of the records. Only the seed calls these: the job that writes the first-seen records
// when a team installs the observer. Its role holds `get` and `update` on the four Secrets by name, and
// no `create` and no `list`. The serving observer never calls them, and its role holds no verb on
// Secrets.

// secretWire is a Secret as the API server sends and takes it. Data holds the decoded bytes: the JSON
// of []byte is base64, as the API wants.
type secretWire struct {
	APIVersion string            `json:"apiVersion"`
	Kind       string            `json:"kind"`
	Metadata   secretMeta        `json:"metadata"`
	Type       string            `json:"type,omitempty"`
	Data       map[string][]byte `json:"data"`
}

type secretMeta struct {
	Name            string            `json:"name"`
	Namespace       string            `json:"namespace"`
	Labels          map[string]string `json:"labels,omitempty"`
	ResourceVersion string            `json:"resourceVersion,omitempty"`
}

// SecretKeys gives the keys of the data of one Secret, and no value. A Secret that does not exist is
// the kind kube-api with the status 404.
func (c *Client) SecretKeys(ctx context.Context, namespace, name string) ([]string, error) {
	var s secretWire
	if err := c.secret(ctx, http.MethodGet, namespace, name, nil, &s); err != nil {
		return nil, err
	}
	keys := make([]string, 0, len(s.Data))
	for k := range s.Data {
		keys = append(keys, k)
	}
	return keys, nil
}

// PutSecret replaces the data and the labels of one Secret that exists. The update names no
// resourceVersion, so it replaces what the Secret holds.
func (c *Client) PutSecret(ctx context.Context, namespace, name string, labels map[string]string, data map[string][]byte) error {
	body, err := json.Marshal(secretWire{APIVersion: "v1", Kind: "Secret", Type: "Opaque",
		Metadata: secretMeta{Name: name, Namespace: namespace, Labels: labels}, Data: data})
	if err != nil {
		return &Error{Kind: docs.KindIO}
	}
	return c.secret(ctx, http.MethodPut, namespace, name, body, nil)
}

// secret sends one request on one Secret. out takes the answer when it is not nil.
func (c *Client) secret(ctx context.Context, method, namespace, name string, body []byte, out any) error {
	client, err := c.httpClient()
	if err != nil {
		return err
	}
	defer client.CloseIdleConnections()
	token, err := readTrimmed(c.cfg.TokenFile)
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(ctx, requestTimeout)
	defer cancel()
	path := "https://" + c.cfg.Host + "/api/v1/namespaces/" + url.PathEscape(namespace) + "/secrets/" + url.PathEscape(name)
	var reader io.Reader
	if body != nil {
		reader = bytes.NewReader(body)
	}
	req, err := http.NewRequestWithContext(ctx, method, path, reader)
	if err != nil {
		return &Error{Kind: docs.KindKubeConfig}
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/json")
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := client.Do(req)
	if err != nil {
		return transportError(ctx)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return &Error{Kind: docs.KindKubeAPI, Status: resp.StatusCode}
	}
	if out == nil {
		return nil
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxBody)).Decode(out); err != nil {
		return &Error{Kind: docs.KindNotJSON}
	}
	return nil
}
