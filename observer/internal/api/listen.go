package api

import (
	"context"
	"errors"
	"io"
	"log"
	"log/slog"
	"net"
	"net/http"
	"strconv"
	"time"
)

// The limits of the server. A request is a GET with a short header, so the read limits are short.
// The write limit is longer than the limit of a detail request.
const (
	readHeaderTimeout = 5 * time.Second
	readTimeout       = 10 * time.Second
	writeTimeout      = 60 * time.Second
	idleTimeout       = 60 * time.Second
	maxHeaderBytes    = 16 << 10
	shutdownGrace     = 5 * time.Second
)

// ErrNotLoopback says that an address is no loopback address with a port. The server takes only a
// loopback address, because its gate is Kubernetes: `kubectl port-forward` reaches a loopback port.
var ErrNotLoopback = errors.New("the address must be a loopback IP address and a port, such as 127.0.0.1:8080")

// checkLoopback accepts a literal loopback IP address and a port. A name is not accepted, because a
// name can resolve to another address.
func checkLoopback(addr string) error {
	host, port, err := net.SplitHostPort(addr)
	if err != nil {
		return ErrNotLoopback
	}
	ip := net.ParseIP(host)
	if _, err := strconv.ParseUint(port, 10, 16); err != nil || ip == nil || !ip.IsLoopback() {
		return ErrNotLoopback
	}
	return nil
}

// Listen opens the address, when it is a loopback address. It binds nothing otherwise.
func Listen(addr string) (net.Listener, error) {
	if err := checkLoopback(addr); err != nil {
		return nil, err
	}
	return net.Listen("tcp", addr)
}

// httpServer makes the server of the http package, with the limits. Each request context ends when
// ctx ends, so a stop cancels a detail request and the strata process that it started.
func (s *Server) httpServer(ctx context.Context) *http.Server {
	return &http.Server{
		Handler:           s.Handler(),
		BaseContext:       func(net.Listener) context.Context { return ctx },
		ReadHeaderTimeout: readHeaderTimeout,
		ReadTimeout:       readTimeout,
		WriteTimeout:      writeTimeout,
		IdleTimeout:       idleTimeout,
		MaxHeaderBytes:    maxHeaderBytes,
		ErrorLog:          log.New(io.Discard, "", 0),
	}
}

// Serve serves ln until ctx ends. The end of ctx also cancels the requests that run, and the server
// then waits a short grace for their answers. It returns nil after a stop that ctx asked for. The log
// of the http package is off, because a panic there prints the value that the handler panicked with.
func (s *Server) Serve(ctx context.Context, ln net.Listener, logger *slog.Logger) error {
	srv := s.httpServer(ctx)
	done := make(chan error, 1)
	go func() { done <- srv.Serve(ln) }()
	logger.Info("listening", "addr", ln.Addr().String())
	select {
	case err := <-done:
		return err
	case <-ctx.Done():
	}
	stop, cancel := context.WithTimeout(context.Background(), shutdownGrace)
	defer cancel()
	if err := srv.Shutdown(stop); err != nil {
		_ = srv.Close()
		return err
	}
	if err := <-done; !errors.Is(err, http.ErrServerClosed) {
		return err
	}
	return nil
}
