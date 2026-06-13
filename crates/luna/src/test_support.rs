use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

#[derive(Clone, Debug)]
pub(crate) struct RecordedRequest {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct MockResponse {
    status: u16,
    body: String,
}

impl MockResponse {
    pub(crate) fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }
}

pub(crate) struct MockHttpServer {
    pub(crate) endpoint: String,
    records: Arc<Mutex<Vec<RecordedRequest>>>,
    handle: JoinHandle<()>,
}

impl MockHttpServer {
    pub(crate) async fn spawn(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let records = Arc::new(Mutex::new(Vec::new()));
        let server_records = Arc::clone(&records);
        let handle = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                server_records.lock().unwrap().push(request);
                write_response(&mut stream, response).await;
            }
        });

        Self {
            endpoint,
            records,
            handle,
        }
    }

    pub(crate) async fn recorded_requests(self) -> Vec<RecordedRequest> {
        self.handle.await.unwrap();
        self.records.lock().unwrap().clone()
    }
}

pub(crate) fn issue_json(
    id: &str,
    identifier: &str,
    state: &str,
    project_slug: Option<&str>,
) -> Value {
    let project = project_slug.map(|slug| {
        json!({
            "id": format!("project-{slug}"),
            "slug": slug,
            "name": slug,
            "state": "Active",
            "priority": null
        })
    });

    json!({
        "id": id,
        "identifier": identifier,
        "title": format!("Issue {identifier}"),
        "description": "Detailed description",
        "priority": 1,
        "state": state,
        "branch_name": "feature/test",
        "url": "https://example.test/issue",
        "labels": ["backend", "codex"],
        "blocked_by": [],
        "created_at": null,
        "updated_at": null,
        "project": project
    })
}

async fn write_response(stream: &mut TcpStream, response: MockResponse) {
    let reason = match response.status {
        200..=299 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        reason,
        response.body.len()
    );
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.write_all(response.body.as_bytes()).await.unwrap();
}

async fn read_request(stream: &mut TcpStream) -> RecordedRequest {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        let Some(header_end) = find_bytes(&buffer, b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if buffer.len() >= header_end + 4 + content_length {
            break;
        }
    }

    let header_end = find_bytes(&buffer, b"\r\n\r\n").unwrap_or(buffer.len());
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let request_line = headers.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let body = if header_end + 4 <= buffer.len() {
        String::from_utf8_lossy(&buffer[header_end + 4..]).to_string()
    } else {
        String::new()
    };

    RecordedRequest {
        method,
        target,
        body,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
