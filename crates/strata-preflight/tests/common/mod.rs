//! The helpers that two test files of strata-preflight share.

use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

/// A stand-in for Palette on this machine. It reads the head of each request: the request line
/// and the headers. It keeps what `keep` gives for the head, and then it writes `answer`.
///
/// `answer` is a whole HTTP answer. The stand-in reads no body, so a test sends it a `GET`. It
/// gives the address for `PALETTE_API`, and what it kept for each request, in order.
pub fn stand_in<T: Send + 'static>(
    answer: String,
    keep: impl Fn(&[String]) -> T + Send + 'static,
) -> (String, Arc<Mutex<Vec<T>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let kept = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&kept);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            // the head ends at the first empty line
            let head: Vec<String> = BufReader::new(&stream)
                .lines()
                .map_while(Result::ok)
                .take_while(|line| !line.is_empty())
                .collect();
            let value = keep(&head);
            seen.lock().unwrap().push(value);
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    (address, kept)
}
