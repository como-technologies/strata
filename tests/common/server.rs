//! A small HTTP server on this machine, the read of a request, and the write of an answer.
//!
//! The tests of each package serve their answers with [`serve`] or [`serve_each`]. The stand-in of
//! `xtask/` reads each request with [`read_request`] and writes each answer with [`write_answer`].
//! So one function reads the head of a request, with its `ApiKey` and its `ProjectUid`. One
//! function writes the form of an answer.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use strata::api::{API_KEY_HEADER, PROJECT_UID_HEADER};

/// The reason phrase of each answer. HTTP gives the phrase no meaning, and no client reads it.
const REASON: &str = "Stand-in";

/// The time that [`serve`] holds a request with no answer, before it reads the next request. It is
/// longer than the timeout of each sender of the tests.
const SILENCE: Duration = Duration::from_secs(20);

/// One request that a server took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, such as `GET`.
    pub method: String,
    /// The target of the request line: the path, and the query when the request has one.
    pub target: String,
    /// The name and the value of each header, in the order of the request.
    pub headers: Vec<(String, String)>,
    /// The body that the header `Content-Length` names, as text.
    pub body: String,
}

impl Request {
    /// A request of `method` for `target` with `body`, as a test gives it to the code of a server.
    /// Each pair of `headers` with a value is a header, and a pair with `None` is no header.
    pub fn new(
        method: &str,
        target: &str,
        headers: &[(&str, Option<&str>)],
        body: &str,
    ) -> Request {
        Request {
            method: method.to_string(),
            target: target.to_string(),
            headers: headers
                .iter()
                .filter_map(|(name, value)| Some((name.to_string(), (*value)?.to_string())))
                .collect(),
            body: body.to_string(),
        }
    }

    /// The value of the first header `name`, or `None` when the request has no such header. The
    /// name of a header has no case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The header `ProjectUid`: the project of the request, or `None` for the tenant scope.
    pub fn project(&self) -> Option<&str> {
        self.header(PROJECT_UID_HEADER)
    }

    /// The header `ApiKey`: the key of the request, or `None` when the request has no key.
    pub fn key(&self) -> Option<&str> {
        self.header(API_KEY_HEADER)
    }

    /// The body, read as JSON.
    ///
    /// # Panics
    ///
    /// When the body is not JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// The answer of a server to one request: its status, the type of its body, and the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpAnswer {
    /// The status, such as 200.
    pub status: u16,
    /// The value of the header `Content-Type`.
    pub kind: &'static str,
    /// The body.
    pub body: Vec<u8>,
}

impl HttpAnswer {
    /// An answer with `status` and the JSON text `body`.
    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> HttpAnswer {
        HttpAnswer {
            status,
            kind: "application/json",
            body: body.into(),
        }
    }
}

/// Read one request from `stream`: the request line, each header, and the body that the header
/// `Content-Length` names.
///
/// `None` when the stream ends before the request line, or when the line has no target.
///
/// # Errors
///
/// An error of the read, also when the stream ends before the end of the body.
pub fn read_request(stream: impl Read) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    let mut request = Request::new(method, target, &[], "");
    // the head ends at the first empty line
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            request
                .headers
                .push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let length = request
        .header("Content-Length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    request.body = String::from_utf8_lossy(&body).into_owned();
    Ok(Some(request))
}

/// Write `answer` on `stream` as one HTTP/1.1 answer that closes the connection.
///
/// # Errors
///
/// An error of the write.
pub fn write_answer(mut stream: impl Write, answer: &HttpAnswer) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {} {REASON}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        answer.status,
        answer.kind,
        answer.body.len()
    )?;
    stream.write_all(&answer.body)?;
    stream.flush()
}

/// Serve on a free port of this machine, one request after the other. `answer` gives the answer
/// to a request, with the count of the requests before it.
///
/// `None` gives no answer, so the sender waits until its timeout. It gives the address, and each
/// request that the server took, in the order of arrival. The server keeps a request before it
/// answers, so the list holds each request that has its answer.
///
/// # Panics
///
/// When no port of this machine is free.
pub fn serve(
    answer: impl Fn(&Request, usize) -> Option<HttpAnswer> + Send + 'static,
) -> (String, Arc<Mutex<Vec<Request>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let Ok(Some(request)) = read_request(&stream) else {
                continue;
            };
            let answered = {
                let mut seen = seen.lock().unwrap();
                let answered = answer(&request, seen.len());
                seen.push(request);
                answered
            };
            match answered {
                // a sender that stops before the answer is no fault of the server
                Some(answered) => {
                    let _ = write_answer(&stream, &answered);
                }
                None => std::thread::sleep(SILENCE),
            }
        }
    });
    (address, requests)
}

/// Serve `answers` with [`serve`], one for each request in turn. A request after the last answer
/// gets no answer.
pub fn serve_each(answers: Vec<HttpAnswer>) -> (String, Arc<Mutex<Vec<Request>>>) {
    serve(move |_, before| answers.get(before).cloned())
}
