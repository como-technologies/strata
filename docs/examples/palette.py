#!/usr/bin/env python3
"""A local stand-in for the Palette API. It serves JSON files from a directory.

The book shows real strata output. docs/examples/run.sh starts this server,
points strata at it with PALETTE_API, and runs each command of the book.

A request reads one file:

    GET <path>                 ProjectUid: <uid>   ->  <root>/<uid>/<path>.json
    GET <path>                 no ProjectUid       ->  <root>/tenant/<path>.json
    GET <path>?<query>         ...                 ->  <root>/.../<path>@<query>.json

When the file does not exist, the answer is HTTP 404 with the body that
Palette gives. The server reads each file at each request, so a script can
change the tenant between two commands.

Usage: palette.py ROOT PORT_FILE

The server listens on a free port of 127.0.0.1 and writes the port number to
PORT_FILE.
"""

import http.server
import json
import pathlib
import sys
import urllib.parse


def file_for(root, project, target):
    parts = urllib.parse.urlsplit(target)
    path = urllib.parse.unquote(parts.path).strip("/")
    if not path or ".." in path.split("/"):
        return None
    name = path + ("@" + urllib.parse.unquote(parts.query) if parts.query else "")
    return root / (project or "tenant") / (name + ".json")


class Handler(http.server.BaseHTTPRequestHandler):
    root = pathlib.Path(".")

    def do_GET(self):
        path = file_for(self.root, self.headers.get("ProjectUid"), self.path)
        if path is not None and path.is_file():
            self.answer(200, path.read_bytes())
            return
        body = {"code": "ResourceNotFound", "message": "no file for " + self.path}
        self.answer(404, json.dumps(body).encode())

    def answer(self, status, body):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: palette.py ROOT PORT_FILE")
    Handler.root = pathlib.Path(sys.argv[1]).resolve()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    pathlib.Path(sys.argv[2]).write_text(str(server.server_address[1]))
    server.serve_forever()


if __name__ == "__main__":
    main()
