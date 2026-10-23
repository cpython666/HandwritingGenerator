use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(directory: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xingjian"))
        .current_dir(directory)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_rejects_non_json_preset_before_writing_image() {
    let directory = tempfile::tempdir().unwrap();
    let output = cli(
        directory.path(),
        &[
            "render",
            "--text",
            "星笺",
            "--output",
            "new.PNG",
            "--save-preset",
            "NEW.png",
            "--force",
        ],
        None,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(".json"));
    assert!(!directory.path().join("new.PNG").exists());
    assert!(!directory.path().join("NEW.png").exists());
}

#[test]
fn cli_embedded_assets_presets_formats_and_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let output = cli(directory.path(), &["assets"], None);
    assert_success(&output);
    let assets: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(assets["fonts"].as_array().unwrap().len(), 9);
    assert!(assets["backgrounds"].as_array().unwrap().len() >= 2);

    let output = cli(
        directory.path(),
        &[
            "render",
            "--text-file",
            "-",
            "--output",
            "letter.png",
            "--save-preset",
            "style.json",
            "--background",
            "solid:#fffaf0:320:240",
            "--x",
            "10",
            "--y",
            "10",
            "--width",
            "300",
            "--height",
            "220",
            "--font-size",
            "20",
            "--letter-spacing",
            "-0.05",
        ],
        Some("\u{feff}星笺 <>&\n  你好，世界。".as_bytes()),
    );
    assert_success(&output);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        (result["width"].as_u64(), result["height"].as_u64()),
        (Some(320), Some(240))
    );
    let preset: Value =
        serde_json::from_slice(&fs::read(directory.path().join("style.json")).unwrap()).unwrap();
    assert_eq!(preset["text"], "星笺 <>&\n  你好，世界。");
    let original = fs::read(directory.path().join("letter.png")).unwrap();
    let output = cli(
        directory.path(),
        &[
            "render",
            "--text",
            "do not replace",
            "--output",
            "letter.png",
        ],
        None,
    );
    assert!(!output.status.success());
    assert_eq!(
        original,
        fs::read(directory.path().join("letter.png")).unwrap()
    );

    for (file, expected) in [
        ("letter.jpg", image::ImageFormat::Jpeg),
        ("letter.webp", image::ImageFormat::WebP),
    ] {
        let output = cli(
            directory.path(),
            &["render", "--preset", "style.json", "--output", file],
            None,
        );
        assert_success(&output);
        let bytes = fs::read(directory.path().join(file)).unwrap();
        assert_eq!(image::guess_format(&bytes).unwrap(), expected);
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 240));
    }
    let output = cli(
        directory.path(),
        &[
            "render",
            "--preset",
            "style.json",
            "--output",
            "letter.png",
            "--force",
        ],
        None,
    );
    assert_success(&output);
    assert_eq!(
        original,
        fs::read(directory.path().join("letter.png")).unwrap()
    );
    let output = cli(
        directory.path(),
        &[
            "render",
            "--text",
            "a",
            "--output",
            "same.png",
            "--save-preset",
            "same.png",
        ],
        None,
    );
    assert!(!output.status.success());
    assert!(!directory.path().join("same.png").exists());
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}
    }})
}

fn request(id: u32, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

#[test]
fn mcp_stdio_render_parity_errors_and_confinement() {
    let directory = tempfile::tempdir().unwrap();
    let output_dir = directory.path().join("images");
    fs::create_dir(&output_dir).unwrap();
    fs::write(directory.path().join("untouched.png"), b"keep").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        directory.path().join("untouched.png"),
        output_dir.join("escape.png"),
    )
    .unwrap();

    let options =
        json!({"bg_path":"solid:#ffffff:320:240","x":10,"y":10,"rect_width":300,"rect_height":220});
    let messages = vec![
        request(0, "tools/list", json!({})),
        initialize(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        request(2, "tools/list", json!({})),
        request(
            3,
            "tools/call",
            json!({"name":"list_assets","arguments":{}}),
        ),
        request(
            4,
            "tools/call",
            json!({"name":"render_handwriting","arguments":{
                "text":"星笺 <>&\n  原样保留", "output_path":"letter.png", "options":options
            }}),
        ),
        request(
            5,
            "tools/call",
            json!({"name":"render_handwriting","arguments":{
                "text":"overwrite forbidden", "output_path":"letter.png", "options":options
            }}),
        ),
        request(
            6,
            "tools/call",
            json!({"name":"render_handwriting","arguments":{
                "text":"escape", "output_path":"../untouched.png", "overwrite":true
            }}),
        ),
        request(
            7,
            "tools/call",
            json!({"name":"render_handwriting","arguments":{
                "text":"bad field", "output_path":"bad.png", "options":{"unknown":1}
            }}),
        ),
        request(8, "tools/call", json!({"name":"missing","arguments":{}})),
        request(9, "ping", json!({})),
    ];
    #[cfg(unix)]
    let messages = {
        let mut messages = messages;
        messages.push(request(
            10,
            "tools/call",
            json!({"name":"render_handwriting","arguments":{
                "text":"symlink escape", "output_path":"escape.png", "overwrite":true
            }}),
        ));
        messages
    };
    // A parser error must not corrupt subsequent requests.
    let mut input = String::from("{invalid json}\n");
    for message in messages {
        input.push_str(&message.to_string());
        input.push('\n');
    }
    let output = cli(
        directory.path(),
        &["mcp", "--output-dir", "images"],
        Some(input.as_bytes()),
    );
    assert_success(&output);
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses[0]["error"]["code"], -32700);
    let response = |id: u32| {
        responses
            .iter()
            .find(|message| message["id"] == id)
            .unwrap()
    };
    assert_eq!(response(0)["error"]["code"], -32002);
    assert_eq!(response(1)["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(response(2)["result"]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(response(3)["result"]["isError"], false);
    assert_eq!(response(4)["result"]["isError"], false);
    for id in [5, 6, 7] {
        assert_eq!(response(id)["result"]["isError"], true);
    }
    assert_eq!(response(8)["error"]["code"], -32602);
    assert_eq!(response(9)["result"], json!({}));
    #[cfg(unix)]
    assert_eq!(response(10)["result"]["isError"], true);
    assert_eq!(
        fs::read(directory.path().join("untouched.png")).unwrap(),
        b"keep"
    );
    assert!(!output_dir.join("bad.png").exists());

    let output = cli(
        directory.path(),
        &[
            "render",
            "--text",
            "星笺 <>&\n  原样保留",
            "--output",
            "cli.png",
            "--background",
            "solid:#ffffff:320:240",
            "--x",
            "10",
            "--y",
            "10",
            "--width",
            "300",
            "--height",
            "220",
        ],
        None,
    );
    assert_success(&output);
    assert_eq!(
        fs::read(output_dir.join("letter.png")).unwrap(),
        fs::read(directory.path().join("cli.png")).unwrap()
    );
}

#[test]
fn mcp_oversized_request_is_drained_and_next_request_works() {
    let directory = tempfile::tempdir().unwrap();
    let mut input = vec![b' '; 2_000_100];
    input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    let output = cli(
        directory.path(),
        &["mcp", "--output-dir", "images"],
        Some(&input),
    );
    assert_success(&output);
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["error"]["code"], -32600);
    assert_eq!(responses[1]["result"], json!({}));
}
