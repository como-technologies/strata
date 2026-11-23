// Package media names the media type of a body of JSON. The server, the client of the Kubernetes API, and the
// webhook of the alerts send such a body. The package holds no type and no value of a cluster, so internal/alert
// can import it.
package media

// JSONType is the media type of a body of JSON, in the header Content-Type or Accept.
const JSONType = "application/json"
