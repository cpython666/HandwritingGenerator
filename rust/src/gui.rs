//! Native editor. Only the shared renderer creates pixels for preview and export.
use crate::{assets, rendering};
use eframe::egui::{self, Color32, FontFamily, RichText, Vec2};
use image::RgbaImage;
use rendering::RenderConfig;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

const INK: Color32 = Color32::from_rgb(43, 79, 68);
const MUTED: Color32 = Color32::from_rgb(113, 120, 112);
const SAMPLE: &str =
    "把一颗星，藏进字里行间。\n\n愿每一句未曾说出口的话，\n都能在纸上，找到温柔的回响。";

enum WorkerResult {
    Preview(u64, Result<(PreviewLayer, PreviewLayer), String>),
    Export(Result<PathBuf, String>),
    CopyImage(Result<RgbaImage, String>),
}

#[derive(Clone)]
struct PreviewLayer {
    pixels: Arc<egui::ColorImage>,
    size: Vec2,
}

struct LayerTexture {
    layer: PreviewLayer,
    texture: egui::TextureHandle,
}

#[derive(Clone, Copy)]
struct TextDrag {
    pointer: egui::Pos2,
    origin: [u32; 2],
    zoom: f32,
}

impl TextDrag {
    fn position(self, pointer: egui::Pos2) -> [u32; 2] {
        let delta = (pointer - self.pointer) / self.zoom;
        [
            (self.origin[0] as f32 + delta.x).round().clamp(0.0, 8192.0) as u32,
            (self.origin[1] as f32 + delta.y).round().clamp(0.0, 8192.0) as u32,
        ]
    }
}

fn prepare_layer(
    image: Arc<RgbaImage>,
    cache: &mut Option<(Arc<RgbaImage>, PreviewLayer)>,
) -> PreviewLayer {
    if let Some((previous, layer)) = cache {
        if Arc::ptr_eq(previous, &image) {
            return layer.clone();
        }
    }
    // Upload display-sized textures; copying/exporting always uses native pixels.
    let size = Vec2::new(image.width() as f32, image.height() as f32);
    let scale = (2048.0 / size.max_elem()).min(1.0);
    let resized;
    let pixels = if scale < 1.0 {
        resized = image::imageops::resize(
            &*image,
            (size.x * scale).round().max(1.0) as u32,
            (size.y * scale).round().max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        );
        &resized
    } else {
        &*image
    };
    let layer = PreviewLayer {
        pixels: Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            [pixels.width() as usize, pixels.height() as usize],
            pixels.as_raw(),
        )),
        size,
    };
    *cache = Some((image, layer.clone()));
    layer
}

fn update_texture(
    ctx: &egui::Context,
    target: &mut Option<LayerTexture>,
    name: &str,
    layer: PreviewLayer,
) {
    if let Some(target) = target {
        if !Arc::ptr_eq(&target.layer.pixels, &layer.pixels) {
            target.texture.set(
                egui::ImageData::Color(layer.pixels.clone()),
                egui::TextureOptions::LINEAR,
            );
        }
        target.layer = layer;
    } else {
        *target = Some(LayerTexture {
            texture: ctx.load_texture(
                name,
                egui::ImageData::Color(layer.pixels.clone()),
                egui::TextureOptions::LINEAR,
            ),
            layer,
        });
    }
}

fn same_layout(a: &RenderConfig, b: &RenderConfig) -> bool {
    a.text == b.text
        && a.bg_path == b.bg_path
        && a.font_path == b.font_path
        && a.font_size == b.font_size
        && a.font_color == b.font_color
        && a.font_weight == b.font_weight
        && a.font_spacing == b.font_spacing
        && a.line_spacing == b.line_spacing
        && a.rect_width == b.rect_width
        && a.rect_height == b.rect_height
}

enum Action {
    OpenPreset,
    SavePreset,
    Export,
    CopyImage,
}

pub fn run() -> anyhow::Result<()> {
    // eframe also applies this to the macOS Dock, overriding the bundle icon.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../icons/app-icon-256.png"))?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("星笺 · 手写图片生成器")
        .with_icon(icon)
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([840.0, 560.0]);
    if let Ok(size) = std::env::var("XINGJIAN_WINDOW_SIZE") {
        if let Some((w, h)) = size.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<f32>(), h.parse::<f32>()) {
                viewport = viewport.with_inner_size([w, h]);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "星笺",
        options,
        Box::new(|cc| Ok(Box::new(Editor::new(cc)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

struct Editor {
    config: RenderConfig,
    observed_config: RenderConfig,
    fonts: Vec<String>,
    backgrounds: Vec<String>,
    tab: usize,
    revision: u64,
    submitted: u64,
    rendered: u64,
    last_submitted: Instant,
    requests: Sender<(u64, RenderConfig)>,
    results: Receiver<WorkerResult>,
    result_sender: Sender<WorkerResult>,
    background_texture: Option<LayerTexture>,
    text_texture: Option<LayerTexture>,
    text_drag: Option<TextDrag>,
    pan_drag: bool,
    image_size: Vec2,
    fit: bool,
    zoom: f32,
    pan: Vec2,
    show_boundary: bool,
    measurement_start: Option<egui::Pos2>,
    measurement: Option<egui::Rect>,
    status: String,
    error: bool,
    export_busy: bool,
    copy_busy: bool,
    clipboard: crate::clipboard::ImageClipboard,
    paper_color: [u8; 3],
    paper_alpha: u8,
    paper_width: u32,
    paper_height: u32,
    about: bool,
    action: Option<Action>,
    screenshot_requested: bool,
    started: Instant,
}

impl Editor {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        configure_fonts(&cc.egui_ctx);
        let (request_tx, request_rx) = mpsc::channel::<(u64, RenderConfig)>();
        let (result_tx, result_rx) = mpsc::channel();
        let worker_tx = result_tx.clone();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let mut renderer = rendering::Renderer::default();
            let mut background = None;
            let mut text = None;
            while let Ok(mut request) = request_rx.recv() {
                // Keep only the newest pending edit; the running render is bounded by core limits.
                while let Ok(newer) = request_rx.try_recv() {
                    request = newer;
                }
                let result = renderer
                    .render_layers(&request.1)
                    .map(|layers| {
                        (
                            prepare_layer(layers.background, &mut background),
                            prepare_layer(layers.text, &mut text),
                        )
                    })
                    .map_err(|e| e.to_string());
                if worker_tx
                    .send(WorkerResult::Preview(request.0, result))
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        let listed = assets::list_assets();
        let items = |key: &str| -> Vec<String> {
            listed[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        let config = RenderConfig {
            text: SAMPLE.into(),
            y: 128,
            line_spacing: 0.94,
            ..Default::default()
        };
        let mut editor = Self {
            observed_config: config.clone(),
            config,
            fonts: items("fonts"),
            backgrounds: items("backgrounds"),
            tab: std::env::var("XINGJIAN_TAB")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0)
                .min(2),
            revision: 1,
            submitted: 0,
            rendered: 0,
            last_submitted: Instant::now(),
            requests: request_tx,
            results: result_rx,
            result_sender: result_tx,
            background_texture: None,
            text_texture: None,
            text_drag: None,
            pan_drag: false,
            image_size: Vec2::ZERO,
            fit: true,
            zoom: 1.0,
            pan: Vec2::ZERO,
            show_boundary: false,
            measurement_start: None,
            measurement: None,
            status: "正在准备纸张与笔迹…".into(),
            error: false,
            export_busy: false,
            copy_busy: false,
            clipboard: crate::clipboard::ImageClipboard::default(),
            paper_color: [255, 253, 247],
            paper_alpha: 255,
            paper_width: 724,
            paper_height: 1024,
            about: false,
            action: None,
            screenshot_requested: false,
            started: Instant::now(),
        };
        editor.backgrounds.sort_by_key(|path| {
            if path.ends_with("letter.png") {
                0
            } else if path.contains("信笺") {
                1
            } else if path.ends_with("grid.jpeg") {
                2
            } else {
                3
            }
        });
        editor.request_preview();
        editor
    }

    fn request_preview(&mut self) {
        if self
            .requests
            .send((self.revision, self.config.clone()))
            .is_ok()
        {
            self.submitted = self.revision;
            self.last_submitted = Instant::now();
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.results.try_recv() {
            match result {
                WorkerResult::Preview(revision, result) if revision >= self.rendered => {
                    match result {
                        Ok((background, text)) => {
                            if self.image_size != background.size {
                                self.measurement = None;
                                self.measurement_start = None;
                                self.text_drag = None;
                            }
                            self.image_size = background.size;
                            update_texture(ctx, &mut self.background_texture, "paper", background);
                            update_texture(ctx, &mut self.text_texture, "handwriting", text);
                            self.rendered = revision;
                            self.status = if revision == self.revision {
                                "已更新预览 · 拖动文字可调整位置"
                            } else {
                                "正在更新预览…"
                            }
                            .into();
                            self.error = false;
                        }
                        Err(error) if revision == self.revision => {
                            self.status = format!("{error}；当前画布保留上次有效预览");
                            self.error = true;
                        }
                        Err(_) => {}
                    }
                }
                WorkerResult::Preview(..) => {}
                WorkerResult::Export(result) => {
                    self.export_busy = false;
                    self.report(result.map(|path| format!("已导出：{}", path.display())));
                }
                WorkerResult::CopyImage(result) => {
                    self.copy_busy = false;
                    let result = result.and_then(|image| {
                        self.clipboard
                            .copy_image(&image)
                            .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "已复制图片 · {} × {} px，可直接粘贴",
                            image.width(),
                            image.height()
                        ))
                    });
                    self.report(result);
                }
            }
        }
    }

    fn report(&mut self, result: Result<String, String>) {
        self.error = result.is_err();
        self.status = result.unwrap_or_else(|e| e);
    }

    fn import_text(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导入文字")
            .add_filter("文本文件", &["txt", "md"])
            .pick_file()
        {
            let result = (|| -> anyhow::Result<String> {
                anyhow::ensure!(
                    path.metadata()?.len() <= 400_003,
                    "文本文件过大，最多 10 万字"
                );
                let text = std::fs::read_to_string(path)?;
                let text = text.trim_start_matches('\u{feff}').to_owned();
                anyhow::ensure!(text.chars().count() <= 100_000, "正文最多 10 万字");
                self.config.text = text;
                Ok("文字已导入".into())
            })();
            self.report(result.map_err(|e| e.to_string()));
        }
    }

    fn open_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("打开排版预设")
            .add_filter("排版预设", &["json"])
            .pick_file()
        {
            match rendering::load_preset(&path) {
                Ok(config) => {
                    self.config = config;
                    self.text_drag = None;
                    self.pan_drag = false;
                    self.fit = true;
                    self.pan = Vec2::ZERO;
                    self.report(Ok(format!("已打开：{}", path.display())));
                }
                Err(error) => self.report(Err(error.to_string())),
            }
        }
    }

    fn save_preset(&mut self) {
        let selected = rfd::FileDialog::new()
            .set_title("保存排版预设")
            .set_file_name("星笺预设.json")
            .add_filter("排版预设", &["json"])
            .save_file();
        if let Some(path) = confirmed_path(selected, "json") {
            self.report(
                rendering::save_preset(&self.config, &path, true)
                    .map(|path| format!("已保存预设：{}", path.display()))
                    .map_err(|e| e.to_string()),
            );
        }
    }

    fn export(&mut self, ctx: &egui::Context) {
        if self.export_busy {
            return;
        }
        let selected = rfd::FileDialog::new()
            .set_title("导出手写图片")
            .set_file_name("星笺.png")
            .add_filter("图片（PNG / JPEG / WebP）", &["png", "jpg", "jpeg", "webp"])
            .save_file();
        let Some(path) = confirmed_path(selected, "png") else {
            return;
        };
        let config = self.config.clone();
        let tx = self.result_sender.clone();
        let ctx = ctx.clone();
        self.export_busy = true;
        self.status = "正在按当前文字与排版生成图片…".into();
        self.error = false;
        // Export a snapshot of the current settings, never a potentially stale preview texture.
        std::thread::spawn(move || {
            let result = rendering::render_image(&config)
                .and_then(|image| rendering::save_image(&image, &path, true))
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerResult::Export(result));
            ctx.request_repaint();
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let command = egui::Modifiers::COMMAND;
        let shift_command = egui::Modifiers {
            shift: true,
            ..command
        };
        if ctx.input_mut(|i| i.consume_key(shift_command, egui::Key::C)) {
            self.action = Some(Action::CopyImage);
        }
        if ctx.input_mut(|i| i.consume_key(shift_command, egui::Key::S)) {
            self.action = Some(Action::SavePreset);
        } else if ctx.input_mut(|i| i.consume_key(command, egui::Key::S)) {
            self.action = Some(Action::Export);
        }
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::O)) {
            self.action = Some(Action::OpenPreset);
        }
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::W)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn copy_image(&mut self, ctx: &egui::Context) {
        if self.copy_busy {
            return;
        }
        let config = self.config.clone();
        let tx = self.result_sender.clone();
        let ctx = ctx.clone();
        self.copy_busy = true;
        self.status = "正在准备原尺寸图片…".into();
        self.error = false;
        std::thread::spawn(move || {
            let result = rendering::render_image(&config).map_err(|e| e.to_string());
            let _ = tx.send(WorkerResult::CopyImage(result));
            ctx.request_repaint();
        });
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(251, 250, 246))
                    .inner_margin(18.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("星笺").size(28.0).strong().color(INK));
                        ui.label(
                            RichText::new("让文字，带一点星光。")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !self.export_busy,
                                egui::Button::new(
                                    RichText::new(if self.export_busy {
                                        "正在导出…"
                                    } else {
                                        "导出图片"
                                    })
                                    .color(Color32::WHITE),
                                )
                                .fill(INK)
                                .min_size(Vec2::new(104.0, 36.0)),
                            )
                            .on_hover_text("Ctrl / ⌘ + S · PNG / JPEG / WebP")
                            .clicked()
                        {
                            self.action = Some(Action::Export);
                        }
                        if ui
                            .add_enabled(
                                !self.copy_busy,
                                egui::Button::new(if self.copy_busy {
                                    "正在复制…"
                                } else {
                                    "复制为图片"
                                })
                                .min_size(Vec2::new(104.0, 36.0)),
                            )
                            .on_hover_text("Ctrl / ⌘ + Shift + C · 复制原尺寸图片")
                            .clicked()
                        {
                            self.action = Some(Action::CopyImage);
                        }
                        if ui
                            .add_sized([94.0, 36.0], egui::Button::new("保存预设"))
                            .on_hover_text("Ctrl / ⌘ + Shift + S")
                            .clicked()
                        {
                            self.action = Some(Action::SavePreset);
                        }
                        if ui
                            .add_sized([94.0, 36.0], egui::Button::new("打开预设"))
                            .on_hover_text("Ctrl / ⌘ + O")
                            .clicked()
                        {
                            self.action = Some(Action::OpenPreset);
                        }
                    });
                });
            });
    }

    fn controls(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(340.0)
            .width_range(280.0..=440.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(251, 250, 246))
                    .inner_margin(18.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (index, title) in ["内容", "排版", "纸张"].iter().enumerate() {
                        ui.selectable_value(&mut self.tab, index, *title);
                    }
                });
                ui.add_space(14.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        0 => self.content_controls(ui),
                        1 => self.layout_controls(ui),
                        _ => self.paper_controls(ui),
                    });
            });
    }

    fn content_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("写下此刻的心意").strong().color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("导入文字").clicked() {
                    self.import_text();
                }
            });
        });
        ui.add_space(8.0);
        let height = (ui.available_height() - 280.0).clamp(110.0, 360.0);
        ui.add_sized(
            [ui.available_width(), height],
            egui::TextEdit::multiline(&mut self.config.text)
                .font(egui::TextStyle::Body)
                .hint_text("在这里输入文字…")
                .char_limit(100_000)
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new(format!(
                "{} 字 · 保留空格与换行",
                self.config.text.chars().count()
            ))
            .small()
            .color(MUTED),
        );
        ui.add_space(16.0);
        label(ui, "选择一种笔迹");
        egui::ComboBox::from_id_salt("font")
            .selected_text(asset_name(&self.config.font_path))
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for path in &self.fonts {
                    ui.selectable_value(&mut self.config.font_path, path.clone(), asset_name(path));
                }
            });
        if ui
            .button("导入本地字体…")
            .on_hover_text("TTF / OTF / TTC")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("选择手写字体")
                .add_filter("字体", &["ttf", "otf", "ttc"])
                .pick_file()
            {
                self.config.font_path = path.to_string_lossy().into_owned();
            }
        }
        ui.add_space(12.0);
        ui.add(egui::Slider::new(&mut self.config.font_size, 1..=200).text("字号 pt"));
        ui.horizontal(|ui| {
            ui.label("墨色");
            let mut rgb = parse_rgb(&self.config.font_color).unwrap_or([32, 32, 32]);
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                self.config.font_color = hex_color(rgb);
            }
            ui.add(egui::TextEdit::singleline(&mut self.config.font_color).desired_width(100.0));
        });
        ui.add_space(14.0);
        ui.label(
            RichText::new("完整字体随软件提供，离线也能落笔。")
                .small()
                .color(MUTED),
        );
    }

    fn layout_controls(&mut self, ui: &mut egui::Ui) {
        label(ui, "字与字之间");
        typography_controls(ui, &mut self.config);
        ui.add_space(20.0);
        label(ui, "文字的位置与范围");
        ui.label(
            RichText::new("可直接拖动画布中的文字；单击数值可输入。")
                .small()
                .color(MUTED),
        );
        egui::Grid::new("region")
            .num_columns(2)
            .spacing([16.0, 14.0])
            .show(ui, |ui| {
                ui.label("左边距");
                ui.add(
                    egui::DragValue::new(&mut self.config.x)
                        .range(0..=8192)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label("上边距");
                ui.add(
                    egui::DragValue::new(&mut self.config.y)
                        .range(0..=8192)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label("区域宽度");
                ui.add(
                    egui::DragValue::new(&mut self.config.rect_width)
                        .range(1..=8192)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label("区域高度");
                ui.add(
                    egui::DragValue::new(&mut self.config.rect_height)
                        .range(1..=8192)
                        .suffix(" px"),
                );
                ui.end_row();
            });
        ui.add_space(16.0);
        ui.checkbox(&mut self.show_boundary, "显示文字边界");
        ui.label(
            RichText::new("边界线只在预览中显示。超出范围的文字会裁切，不会自动分页。")
                .small()
                .color(MUTED),
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new("画布可拖动。Ctrl / ⌘ + 滚轮缩放，右键拖动测量，双击清除测量标记。")
                .small()
                .color(MUTED),
        );
    }

    fn paper_controls(&mut self, ui: &mut egui::Ui) {
        label(ui, "为文字选一张纸");
        for path in &self.backgrounds {
            if ui
                .add_sized(
                    [ui.available_width(), 32.0],
                    egui::Button::selectable(self.config.bg_path == *path, paper_name(path)),
                )
                .clicked()
            {
                self.config.bg_path = path.clone();
                self.fit = true;
                self.pan = Vec2::ZERO;
            }
        }
        ui.add_space(8.0);
        if ui.button("导入背景图片…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("选择纸张背景")
                .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
            {
                self.config.bg_path = path.to_string_lossy().into_owned();
                self.fit = true;
                self.pan = Vec2::ZERO;
            }
        }
        ui.label(
            RichText::new(format!("当前：{}", asset_name(&self.config.bg_path)))
                .small()
                .color(MUTED),
        );
        ui.add_space(22.0);
        label(ui, "也可以从一张白纸开始");
        ui.horizontal(|ui| {
            ui.label("纸色");
            ui.color_edit_button_srgb(&mut self.paper_color);
        });
        ui.add(egui::Slider::new(&mut self.paper_alpha, 0..=255).text("不透明度"));
        ui.horizontal(|ui| {
            ui.label("宽");
            ui.add(
                egui::DragValue::new(&mut self.paper_width)
                    .range(1..=8192)
                    .suffix(" px"),
            );
            ui.label("高");
            ui.add(
                egui::DragValue::new(&mut self.paper_height)
                    .range(1..=8192)
                    .suffix(" px"),
            );
        });
        if ui.button("使用纯色纸张").clicked() {
            self.config.bg_path = format!(
                "solid:{}{:02x}:{}:{}",
                hex_color(self.paper_color),
                self.paper_alpha,
                self.paper_width,
                self.paper_height
            );
            self.fit = true;
            self.pan = Vec2::ZERO;
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("导出保留纸张原始像素尺寸。画布缩放不会改变图片清晰度。")
                .small()
                .color(MUTED),
        );
    }

    fn preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(232, 235, 228))
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("纸上预览").strong().color(INK));
                    if self.submitted != self.rendered && !self.error {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("适应").clicked() {
                            self.fit = true;
                            self.pan = Vec2::ZERO;
                        }
                        if ui.button("1:1").clicked() {
                            self.fit = false;
                            self.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                        }
                        if ui.button("+").clicked() {
                            self.fit = false;
                            self.zoom = (self.zoom * 1.2).min(8.0);
                        }
                        ui.label(format!("{:.0}%", self.zoom * 100.0));
                        if ui.button("−").clicked() {
                            self.fit = false;
                            self.zoom = (self.zoom / 1.2).max(0.05);
                        }
                    });
                });
                ui.label(
                    RichText::new("拖动文字定位 · 空格＋拖动 / 中键移动纸张")
                        .small()
                        .color(MUTED),
                );
                ui.add_space(8.0);
                let (canvas, response) = ui.allocate_exact_size(
                    ui.available_size().max(Vec2::splat(1.0)),
                    egui::Sense::click_and_drag(),
                );
                let painter = ui.painter_at(canvas);
                if self.fit && self.image_size.x > 0.0 {
                    self.zoom = ((canvas.width() - 48.0) / self.image_size.x)
                        .min((canvas.height() - 48.0) / self.image_size.y)
                        .clamp(0.01, 2.0);
                    self.pan = Vec2::ZERO;
                }
                if response.hovered() && self.text_drag.is_none() {
                    let (scroll, zoom_delta, command) =
                        ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
                    if (zoom_delta - 1.0).abs() > 0.001 || (command && scroll.y != 0.0) {
                        let old = self.zoom;
                        let factor = if (zoom_delta - 1.0).abs() > 0.001 {
                            zoom_delta
                        } else {
                            (scroll.y * 0.004).exp()
                        };
                        self.zoom = (self.zoom * factor).clamp(0.05, 8.0);
                        self.fit = false;
                        if let Some(pointer) = response.hover_pos() {
                            let anchor = pointer - canvas.center();
                            self.pan = anchor + (self.pan - anchor) * (self.zoom / old);
                        }
                    } else if scroll != Vec2::ZERO {
                        self.fit = false;
                        self.pan += scroll;
                    }
                }
                if let Some(texture) = &self.background_texture {
                    let mut rect = egui::Rect::from_center_size(
                        canvas.center() + self.pan,
                        self.image_size * self.zoom,
                    );
                    let region = egui::Rect::from_min_size(
                        rect.min
                            + egui::vec2(self.config.x as f32, self.config.y as f32) * self.zoom,
                        egui::vec2(
                            self.config.rect_width as f32,
                            self.config.rect_height as f32,
                        ) * self.zoom,
                    )
                    .intersect(rect)
                    .intersect(canvas);
                    let space = ui.input(|i| i.key_down(egui::Key::Space));
                    // Capture on press: a quick move + release may arrive in one frame,
                    // before click_and_drag ever emits drag_started.
                    if response.is_pointer_button_down_on()
                        && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                    {
                        if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                            self.pan_drag = space || !region.contains(start);
                            self.text_drag = (!self.pan_drag).then_some(TextDrag {
                                pointer: start,
                                origin: [self.config.x, self.config.y],
                                zoom: self.zoom,
                            });
                        }
                    }
                    if let Some(drag) = self.text_drag {
                        if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                            [self.config.x, self.config.y] = drag.position(pointer);
                            if !self.error {
                                self.status =
                                    format!("文字位置：{}，{} px", self.config.x, self.config.y);
                            }
                            ctx.request_repaint();
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Middle)
                        || (self.pan_drag && response.dragged_by(egui::PointerButton::Primary))
                    {
                        self.fit = false;
                        self.pan += response.drag_delta();
                        rect = egui::Rect::from_center_size(
                            canvas.center() + self.pan,
                            self.image_size * self.zoom,
                        );
                    }
                    let moving_text = self.text_drag.is_some();
                    if !ui.input(|i| i.pointer.primary_down()) {
                        self.text_drag = None;
                        self.pan_drag = false;
                    }
                    let hovering_text =
                        response.hover_pos().is_some_and(|p| region.contains(p)) && !space;
                    if moving_text {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if hovering_text || (response.hovered() && space) {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if response.double_clicked() {
                        self.measurement = None;
                        self.measurement_start = None;
                    }
                    if let Some(pointer) = response.interact_pointer_pos() {
                        let point = ((pointer - rect.min) / self.zoom)
                            .clamp(Vec2::ZERO, self.image_size)
                            .to_pos2();
                        if response.drag_started_by(egui::PointerButton::Secondary)
                            && rect.contains(pointer)
                        {
                            self.measurement_start = Some(point);
                        }
                        if response.dragged_by(egui::PointerButton::Secondary) {
                            if let Some(start) = self.measurement_start {
                                self.measurement = Some(egui::Rect::from_two_pos(start, point));
                            }
                        }
                    }
                    painter.rect_filled(
                        rect.translate(egui::vec2(0.0, 5.0)),
                        2.0,
                        Color32::from_black_alpha(12),
                    );
                    painter.rect_filled(rect, 0.0, Color32::WHITE);
                    painter.image(
                        texture.texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    if let Some(text) = &self.text_texture {
                        let text_rect = egui::Rect::from_min_size(
                            rect.min
                                + egui::vec2(self.config.x as f32, self.config.y as f32)
                                    * self.zoom,
                            text.layer.size * self.zoom,
                        );
                        painter.with_clip_rect(rect.intersect(canvas)).image(
                            text.texture.id(),
                            text_rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, Color32::from_black_alpha(22)),
                        egui::StrokeKind::Inside,
                    );
                    if self.show_boundary || hovering_text || moving_text {
                        let region = egui::Rect::from_min_size(
                            rect.min
                                + egui::vec2(self.config.x as f32, self.config.y as f32)
                                    * self.zoom,
                            egui::vec2(
                                self.config.rect_width as f32,
                                self.config.rect_height as f32,
                            ) * self.zoom,
                        );
                        painter.with_clip_rect(rect.intersect(canvas)).rect_stroke(
                            region,
                            0.0,
                            egui::Stroke::new(1.0, Color32::from_rgb(46, 128, 107)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if let Some(measured) = self.measurement {
                        let bounds = egui::Rect::from_min_size(
                            rect.min + measured.min.to_vec2() * self.zoom,
                            measured.size() * self.zoom,
                        );
                        let overlay = painter.with_clip_rect(rect.intersect(canvas));
                        overlay.rect_stroke(
                            bounds,
                            0.0,
                            egui::Stroke::new(1.0, INK),
                            egui::StrokeKind::Inside,
                        );
                        let text = format!(
                            "{}, {} · {} × {} px",
                            measured.left().round(),
                            measured.top().round(),
                            measured.width().round(),
                            measured.height().round()
                        );
                        let galley = overlay.layout_no_wrap(
                            text,
                            egui::FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                        let label =
                            egui::Rect::from_min_size(bounds.min, galley.size() + Vec2::splat(8.0));
                        overlay.rect_filled(label, 2.0, INK);
                        overlay.galley(label.min + Vec2::splat(4.0), galley, Color32::WHITE);
                    }
                } else {
                    painter.text(
                        canvas.center(),
                        egui::Align2::CENTER_CENTER,
                        "正在展开纸张…",
                        egui::FontId::proportional(16.0),
                        MUTED,
                    );
                }
            });
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("XINGJIAN_SCREENSHOT") else {
            return;
        };
        if !self.screenshot_requested
            && self.rendered == self.revision
            && self.started.elapsed() > Duration::from_millis(600)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_requested = true;
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Some(image) =
                    RgbaImage::from_raw(image.width() as u32, image.height() as u32, rgba)
                {
                    if let Err(error) = image.save(&path) {
                        eprintln!("Cannot save screenshot: {error}");
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if !self.screenshot_requested {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
        self.shortcuts(ctx);
        self.header(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(251, 250, 246))
                    .inner_margin(10.0),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&self.status).small().color(if self.error {
                        Color32::from_rgb(167, 55, 45)
                    } else {
                        MUTED
                    }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("关于星笺").clicked() {
                            self.about = true;
                        }
                        if self.image_size.x > 0.0 {
                            ui.label(
                                RichText::new(format!(
                                    "{} × {} px",
                                    self.image_size.x as u32, self.image_size.y as u32
                                ))
                                .small()
                                .color(MUTED),
                            );
                        }
                    });
                });
            });
        self.controls(ctx);
        self.preview(ctx);
        // Process the action after TextEdit consumes this frame's input, including a final keystroke.
        match self.action.take() {
            Some(Action::OpenPreset) => self.open_preset(),
            Some(Action::SavePreset) => self.save_preset(),
            Some(Action::Export) => self.export(ctx),
            Some(Action::CopyImage) => self.copy_image(ctx),
            None => {}
        }
        if self.observed_config != self.config {
            if !same_layout(&self.observed_config, &self.config) {
                self.revision += 1;
                self.error = false;
            } else if !self.error {
                self.status = format!("文字位置：{}，{} px", self.config.x, self.config.y);
            }
            self.observed_config = self.config.clone();
        }
        if self.revision != self.submitted {
            let interval = Duration::from_millis(33);
            if self.last_submitted.elapsed() >= interval {
                self.request_preview();
            } else {
                ctx.request_repaint_after(interval.saturating_sub(self.last_submitted.elapsed()));
            }
        }
        egui::Window::new("关于星笺").open(&mut self.about).collapsible(false).resizable(false).default_width(340.0).show(ctx, |ui| {
            ui.heading(RichText::new("星笺").color(INK));
            ui.label("让文字，带一点星光。");
            ui.add_space(12.0);
            ui.label("把想说的话，写成一页有温度的纸笺。挑一种合心意的笔迹，慢慢调整字句之间的距离，收藏日常里微小而明亮的心事。");
            ui.add_space(10.0);
            ui.label(RichText::new("文字和图片均在本机处理，可完全离线使用。").small().color(MUTED));
        });
        self.screenshot(ctx);
    }
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(Color32::from_rgb(49, 60, 52));
    style.visuals.panel_fill = Color32::from_rgb(251, 250, 246);
    style.visuals.window_fill = Color32::from_rgb(251, 250, 246);
    style.visuals.extreme_bg_color = Color32::from_rgb(255, 254, 251);
    style.visuals.selection.bg_fill = Color32::from_rgb(210, 227, 215);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, INK);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(242, 243, 236);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(242, 243, 236);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(221, 231, 219);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(204, 221, 203);
    style.spacing.item_spacing = egui::vec2(8.0, 9.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.slider_width = 160.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    ctx.set_style_of(egui::Theme::Light, style);
}

fn typography_controls(ui: &mut egui::Ui, config: &mut RenderConfig) {
    ui.add(egui::Slider::new(&mut config.font_size, 1..=200).text("字号 pt"));
    // Clamp/snap only user edits, so merely opening this tab never changes an imported preset.
    ui.add(
        egui::Slider::new(&mut config.font_weight, 100..=900)
            .step_by(100.0)
            .clamping(egui::SliderClamping::Edits)
            .text("字重"),
    );
    ui.add(
        egui::Slider::new(&mut config.font_spacing, -1.0..=2.0)
            .step_by(0.01)
            .clamping(egui::SliderClamping::Edits)
            .text("字距 em"),
    );
    ui.add(
        egui::Slider::new(&mut config.line_spacing, 0.1..=5.0)
            .step_by(0.05)
            .clamping(egui::SliderClamping::Edits)
            .text("行距倍数"),
    );
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates: Vec<PathBuf> = if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/PingFang.ttc",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    } else if cfg!(target_os = "windows") {
        let root = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        ["msyh.ttc", "simhei.ttf", "simsun.ttc"]
            .into_iter()
            .map(|name| root.join("Fonts").join(name))
            .collect()
    } else {
        [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    };
    let data = candidates
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .or_else(|| assets::font_bytes("builtin:fonts/hand.ttf").ok());
    if let Some(data) = data {
        fonts
            .font_data
            .insert("cjk-ui".into(), egui::FontData::from_owned(data).into());
        // Keep the bundled Latin UI face; use the system CJK face where Latin lacks a glyph.
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk-ui".into());
        }
    }
    ctx.set_fonts(fonts);
}

fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(INK));
    ui.add_space(5.0);
}

fn asset_name(path: &str) -> String {
    if path.starts_with("solid:") {
        return "纯色纸张".into();
    }
    let name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    if name == "hand" {
        "默认手写体".into()
    } else {
        name.into()
    }
}

fn paper_name(path: &str) -> String {
    let name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if let Some((color, size)) = name.strip_prefix('#').and_then(|s| s.split_once('-')) {
        return format!("纯色纸 · #{color} · {}", size.replace('x', " × "));
    }
    match name {
        "letter" => "信笺 · 横线纸".into(),
        "grid" => "方格纸".into(),
        "white" => "留白 · 白纸".into(),
        _ => asset_name(path),
    }
}

fn parse_rgb(value: &str) -> Option<[u8; 3]> {
    let color = rendering::parse_color(value).ok()?;
    Some([color[0], color[1], color[2]])
}

fn hex_color(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn confirmed_path(selected: Option<PathBuf>, extension: &str) -> Option<PathBuf> {
    let mut path = selected?;
    if path.extension().is_none() {
        path.set_extension(extension);
        // The native save dialog confirmed the entered path, not this appended extension.
        if path.exists()
            && rfd::MessageDialog::new()
                .set_title("覆盖已有文件？")
                .set_description(format!("{} 已存在。是否替换？", path.display()))
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return None;
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_uses_original_position_and_image_scale() {
        let mut drag = TextDrag {
            pointer: egui::pos2(200.0, 100.0),
            origin: [80, 128],
            zoom: 0.5,
        };
        assert_eq!(drag.position(egui::pos2(240.0, 120.0)), [160, 168]);
        drag.zoom = 2.0;
        assert_eq!(drag.position(egui::pos2(240.0, 120.0)), [100, 138]);
        // Each event remains relative to the original pointer, so small movements accumulate.
        assert_eq!(drag.position(egui::pos2(200.5, 100.0)), [80, 128]);
        assert_eq!(drag.position(egui::pos2(202.0, 100.0)), [81, 128]);
        assert_eq!(drag.position(egui::pos2(-1000.0, -1000.0)), [0, 0]);
    }

    #[test]
    fn moving_text_does_not_invalidate_the_rendered_layer() {
        let config = RenderConfig::default();
        let mut changed = config.clone();
        changed.x = 200;
        changed.y = 300;
        assert!(same_layout(&config, &changed));
        changed.font_spacing = 0.5;
        assert!(!same_layout(&config, &changed));
    }

    #[test]
    fn preview_preparation_reuses_unchanged_pixels_and_caps_large_uploads() {
        let image = Arc::new(RgbaImage::new(2500, 10));
        let mut cache = None;
        let first = prepare_layer(image.clone(), &mut cache);
        let second = prepare_layer(image, &mut cache);
        assert!(Arc::ptr_eq(&first.pixels, &second.pixels));
        assert_eq!(first.size, Vec2::new(2500.0, 10.0));
        assert_eq!(first.pixels.width(), 2048);
    }

    #[test]
    fn displaying_controls_does_not_round_a_preset() {
        let ctx = egui::Context::default();
        let mut config = RenderConfig {
            font_weight: 550,
            font_spacing: 0.1234,
            line_spacing: 0.94,
            ..Default::default()
        };
        let original = config.clone();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| typography_controls(ui, &mut config));
        });
        assert_eq!(config, original);
    }
}
