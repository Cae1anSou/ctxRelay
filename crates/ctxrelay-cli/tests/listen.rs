use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn ctxrelay_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ctxrelay"))
}

/// 连接重试:`cargo test` 本身的调度/编译开销会让子进程 bind 端口的时机比
/// 一次性的 `sleep` 更晚,单次连接偶尔会撞上服务还没起来的窗口,退化成一个跟
/// 被测代码无关的计时 flake。这里改成短间隔重试连接,而不是指望一次 sleep
/// 刚好够——只要服务最终起来了,测试就该通过。
fn connect_with_retry(port: u16) -> TcpStream {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => return stream,
            Err(e) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
                let _ = e;
            }
            Err(e) => panic!("connect to ctxrelay listen: {e}"),
        }
    }
}

/// 手写一个最小的 HTTP/1.1 POST 客户端(不引入额外的 HTTP client 依赖,测试范围内
/// 够用):连上 `listen` 起的服务,发一个 CaptureRequest,读回响应体。
fn post_capture(port: u16, token: &str, body: &str) -> (u16, String) {
    let mut stream = connect_with_retry(port);
    let request = format!(
        "POST /capture HTTP/1.1\r\nHost: 127.0.0.1\r\nX-CtxRelay-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();

    let status_line = response.lines().next().unwrap_or("");
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body_start = response
        .find("\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(response.len());
    (status_code, response[body_start..].to_string())
}

#[test]
fn listen_accepts_one_capture_and_exits() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-cli-listen-test-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    let slug = canonical.display().to_string().replace('/', "-");
    std::fs::create_dir_all(projects_root.join(&slug)).unwrap();

    let manifest_path = project_dir.join("manifest.json");

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47899")
        .arg("--claude-projects-root")
        .arg(&projects_root)
        .arg("--manifest-out")
        .arg(&manifest_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

    // 从 stdout 里读出 listen 打印的 token(格式:"token: <uuid>")——`read_line` 本身
    // 会阻塞到有数据可读,不需要额外 sleep 等服务起来;真正的连接重试兜底在
    // `post_capture` 内部的 `connect_with_retry`。
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut first_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
    let token = first_line
        .split("token: ")
        .nth(1)
        .map(|s| s.trim().to_string())
        .expect("listen should print a token line");

    let snapshot =
        std::fs::read_to_string("../fe-claude-live/tests/fixtures/sample_live_conversation.json")
            .unwrap();
    let capture_request = format!(
        r#"{{"version":"1","token":"{token}","conversation_id":"fca79960-3026-40e1-beba-6abb33fe20d5","org_id":"ed9a9a3c-9d81-43a0-b974-3aa686e20a87","snapshot":{snapshot}}}"#
    );

    let (status, body) = post_capture(47899, &token, &capture_request);

    assert_eq!(status, 200, "response body: {body}");
    assert!(body.contains("\"status\":\"ok\""), "response body: {body}");

    let exit_status = child
        .wait()
        .expect("listen process should exit after one capture");
    assert!(exit_status.success());

    assert!(manifest_path.exists());

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}

#[test]
fn listen_rejects_wrong_token() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-badtoken-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47900")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

    let (status, _body) = post_capture(
        47900,
        "wrong-token",
        r#"{"version":"1","token":"wrong-token","conversation_id":"x","org_id":"y","snapshot":{}}"#,
    );
    assert_eq!(status, 401);

    child.kill().ok();
    child.wait().ok();
    std::fs::remove_dir_all(&project_dir).ok();
}
