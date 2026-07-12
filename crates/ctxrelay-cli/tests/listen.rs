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
        r#"{{"version":"1","token":"{token}","frontend_id":"fe-claude-live","snapshot":{snapshot}}}"#
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
        r#"{"version":"1","token":"wrong-token","frontend_id":"fe-claude-live","snapshot":{}}"#,
    );
    assert_eq!(status, 401);

    child.kill().ok();
    child.wait().ok();
    std::fs::remove_dir_all(&project_dir).ok();
}

/// 回归测试:曾经请求体反序列化失败时,`listen` 会直接用 `?` 把错误甩给
/// `main()`,`request.respond()` 从未被调用,TCP 连接被复位而不是收到一个明确的
/// 400。扩展侧的 `fetch` 在这种情况下会抛异常,badge 显示成 `N/L`(“没连上本地
/// 服务”)——但真实原因是“连上了,body 解析崩了”,会把用户导向错误的排查方向。
#[test]
fn listen_returns_400_on_malformed_json_body() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-malformed-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47901")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut first_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
    let token = first_line
        .split("token: ")
        .nth(1)
        .map(|s| s.trim().to_string())
        .expect("listen should print a token line");

    let (status, body) = post_capture(47901, &token, "{not valid json");
    assert_eq!(status, 400, "response body: {body}");
    assert!(
        body.contains("\"status\":\"error\""),
        "response body: {body}"
    );

    child.wait().ok();
    std::fs::remove_dir_all(&project_dir).ok();
}

/// 回归测试:曾经不管管线是否真的失败,`listen` 一律回 HTTP 200,只在 body 里把
/// `status` 标成 `"error"`——而扩展侧只看 `res.ok`,于是导入管线报错时工具栏仍然
/// 显示 `OK`,用户会以为导入成功。这里用一个未 bootstrap 过的目标目录触发一次
/// 真实的管线失败(`resolve_claude_code_dest` 找不到对应 session 目录、且未传
/// `--bootstrap`),断言响应状态码不是 2xx。
#[test]
fn listen_returns_non_2xx_when_import_pipeline_fails() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-pipeline-fail-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    // 特意不预先在 projects_root 下建对应 slug 目录,且不传 --bootstrap——
    // `resolve_claude_code_dest` 应该会报错,而不是意外成功。
    let projects_root =
        std::env::temp_dir().join("ctxrelay-cli-listen-pipeline-fail-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    std::fs::create_dir_all(&projects_root).unwrap();

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47902")
        .arg("--claude-projects-root")
        .arg(&projects_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

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
        r#"{{"version":"1","token":"{token}","frontend_id":"fe-claude-live","snapshot":{snapshot}}}"#
    );

    let (status, body) = post_capture(47902, &token, &capture_request);
    assert!(
        !(200..300).contains(&status),
        "expected a non-2xx status for a failed import, got {status}, body: {body}"
    );
    assert!(
        body.contains("\"status\":\"error\""),
        "response body: {body}"
    );

    child.wait().ok();
    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}
