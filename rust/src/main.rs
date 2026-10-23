use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, Subcommand};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf};
use xingjian::{assets, mcp, rendering};

#[derive(Parser)]
#[command(
    name = "xingjian",
    version,
    about = "星笺 · 本地手写图片生成器",
    after_help = "不带参数启动桌面编辑器。所有字体和图片处理均在本地完成。"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 列出随程序内置的全部字体与纸张（JSON）
    Assets,
    /// 无界面生成 PNG、JPEG 或 WebP 图片
    Render(Box<RenderArgs>),
    /// 启动本地 stdio MCP 服务；stdout 专用于协议
    Mcp {
        #[arg(long, default_value = "outputs")]
        output_dir: PathBuf,
    },
}

#[derive(clap::Args)]
#[command(group(ArgGroup::new("source").args(["text", "text_file", "preset"]).required(true).multiple(true)))]
struct RenderArgs {
    #[arg(long, conflicts_with = "text_file")]
    text: Option<String>,
    /// UTF-8 文本文件；- 表示标准输入
    #[arg(long)]
    text_file: Option<PathBuf>,
    #[arg(long)]
    preset: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    /// 显式允许覆盖已有图片或预设
    #[arg(long)]
    force: bool,
    #[arg(long)]
    save_preset: Option<PathBuf>,
    #[arg(long)]
    font: Option<String>,
    #[arg(long)]
    background: Option<String>,
    #[arg(long)]
    font_size: Option<u32>,
    #[arg(long)]
    color: Option<String>,
    #[arg(long)]
    font_weight: Option<u32>,
    #[arg(long)]
    x: Option<i32>,
    #[arg(long)]
    y: Option<i32>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long, allow_hyphen_values = true)]
    letter_spacing: Option<f64>,
    #[arg(long)]
    line_spacing: Option<f64>,
}

fn read_text(path: &std::path::Path) -> Result<String> {
    // UTF-8 has at most four bytes per character; stop before allocating an unbounded input.
    let mut bytes = Vec::new();
    if path == std::path::Path::new("-") {
        std::io::stdin().take(400_004).read_to_end(&mut bytes)?;
    } else {
        std::fs::File::open(path)
            .with_context(|| format!("无法读取文本：{}", path.display()))?
            .take(400_004)
            .read_to_end(&mut bytes)?;
    }
    if bytes.len() >= 400_004 {
        bail!("正文超过 100000 字符限制");
    }
    let text = String::from_utf8(bytes).context("文本文件必须使用 UTF-8 编码")?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

fn render(args: RenderArgs) -> Result<()> {
    let config = match &args.preset {
        Some(path) => rendering::load_preset(path)?,
        None => rendering::RenderConfig::default(),
    };
    let mut config = serde_json::to_value(config)?;
    let fields = config.as_object_mut().expect("RenderConfig is an object");
    macro_rules! replace {
        ($name:literal, $value:expr) => {
            if let Some(value) = $value {
                fields.insert($name.to_owned(), json!(value));
            }
        };
    }
    replace!("text", args.text);
    if let Some(path) = &args.text_file {
        fields.insert("text".to_owned(), Value::String(read_text(path)?));
    }
    replace!("font_path", args.font);
    replace!("bg_path", args.background);
    replace!("font_size", args.font_size);
    replace!("font_color", args.color);
    replace!("font_weight", args.font_weight);
    replace!("x", args.x);
    replace!("y", args.y);
    replace!("rect_width", args.width);
    replace!("rect_height", args.height);
    replace!("font_spacing", args.letter_spacing);
    replace!("line_spacing", args.line_spacing);
    let config: rendering::RenderConfig = serde_json::from_value(config)?;
    if let Some(path) = &args.save_preset {
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            bail!("预设必须保存为 .json 文件");
        }
        if mcp::resolved_path(path)? == mcp::resolved_path(&args.output)? {
            bail!("图片和预设必须使用不同的输出路径");
        }
        if path.exists() && !args.force {
            bail!("预设已存在：{}（使用 --force 覆盖）", path.display());
        }
    }
    let image = rendering::render_image(&config)?;
    let saved = rendering::save_image(&image, &args.output, args.force)?;
    if let Some(path) = &args.save_preset {
        rendering::save_preset(&config, path, args.force)?;
    }
    println!(
        "{}",
        json!({"path": saved, "width": image.width(), "height": image.height()})
    );
    Ok(())
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Some(Command::Assets) => {
            println!("{}", serde_json::to_string_pretty(&assets::list_assets())?)
        }
        Some(Command::Render(args)) => render(*args)?,
        Some(Command::Mcp { output_dir }) => mcp::run(&output_dir)?,
        None => {
            #[cfg(feature = "gui")]
            {
                #[cfg(windows)]
                // Only detach the desktop launch; CLI and MCP keep their inherited streams.
                unsafe {
                    windows_sys::Win32::System::Console::FreeConsole();
                }
                xingjian::gui::run()?;
            }
            #[cfg(not(feature = "gui"))]
            bail!("此构建不含桌面界面；使用 render、assets 或 mcp 子命令");
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("错误：{error:#}");
        std::process::exit(1);
    }
}
