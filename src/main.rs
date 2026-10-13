#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod install;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{Context, FontData, FontDefinitions, FontFamily, Key, OutputCommand, ViewportBuilder};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

const APP_NAME: &str = "mdviewer";
const MAX_CONTENT_WIDTH: f32 = 920.0;
const MD_EXTS: [&str; 3] = ["md", "markdown", "mdown"];

fn main() {
    let arg = std::env::args_os().nth(1);
    if let Some(cmd) = arg.as_ref().and_then(|a| a.to_str()) {
        match cmd {
            "--install" => return install::install(),
            "--uninstall" => return install::uninstall(),
            _ => {}
        }
    } else if install::offer_install() {
        return;
    }
    let initial = arg.map(PathBuf::from);

    // O cache carrega as gramáticas do syntect (dezenas de ms): roda em paralelo
    // com a criação da janela.
    let cache = std::thread::spawn(CommonMarkCache::default);
    let mut cache = Some(cache);
    let mut initial = Some(initial);

    // Render por software (CPU): evita inicializar driver de GPU, que custava
    // 250–400ms com OpenGL e ~2s com DX12.
    let config = SoftwareBackendAppConfiguration {
        viewport_builder: ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([960.0, 820.0])
            .with_drag_and_drop(true)
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let _ = egui_software_backend::run_app_with_software_backend(config, |ctx| {
        let c = cache.take().and_then(|h| h.join().ok()).unwrap_or_default();
        App::new(&ctx, initial.take().flatten(), c)
    });
}

struct App {
    path: Option<PathBuf>,
    text: String,
    error: Option<String>,
    cache: CommonMarkCache,
    history: Vec<PathBuf>,
    /// Muda a cada navegação para resetar o scroll (reload mantém a posição).
    doc_id: u64,
    watcher: Option<RecommendedWatcher>,
    changes: Option<Receiver<()>>,
}

impl App {
    fn new(ctx: &Context, initial: Option<PathBuf>, cache: CommonMarkCache) -> Self {
        egui_extras::install_image_loaders(ctx);
        setup_fonts(ctx);
        ctx.all_styles_mut(|s| {
            s.url_in_tooltip = true;
            for (style, font) in s.text_styles.iter_mut() {
                font.size = match style {
                    egui::TextStyle::Body | egui::TextStyle::Button => 16.0,
                    egui::TextStyle::Monospace => 14.0,
                    // egui_commonmark escala H1..H6 entre Body e Heading.
                    egui::TextStyle::Heading => 30.0,
                    _ => font.size,
                };
            }
        });

        let mut app = Self {
            path: None,
            text: String::new(),
            error: None,
            cache,
            history: Vec::new(),
            doc_id: 0,
            watcher: None,
            changes: None,
        };
        if let Some(p) = initial {
            app.open(ctx, p, false);
        }
        app
    }

    /// Abre um arquivo. `push_history` guarda o atual para voltar com Alt+←.
    fn open(&mut self, ctx: &Context, path: PathBuf, push_history: bool) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if push_history && let Some(cur) = self.path.take() {
            self.history.push(cur);
        }
        if let Some(dir) = path.parent() {
            // Imagens e links relativos são resolvidos a partir do diretório do arquivo.
            let _ = std::env::set_current_dir(dir);
        }
        self.path = Some(path.clone());
        self.doc_id += 1;
        self.reload(ctx);
        self.watch(ctx, &path);

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name} — {APP_NAME}")));
    }

    fn reload(&mut self, ctx: &Context) {
        let Some(path) = &self.path else { return };
        match std::fs::read(path) {
            Ok(bytes) => {
                let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
                self.text = String::from_utf8_lossy(bytes).into_owned();
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Erro ao abrir {}: {e}", path.display())),
        }
        ctx.forget_all_images();
    }

    /// Observa o diretório (e não o arquivo) porque editores costumam salvar
    /// substituindo o arquivo, o que quebraria um watch direto nele.
    fn watch(&mut self, ctx: &Context, path: &Path) {
        self.watcher = None;
        self.changes = None;
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let name = name.to_owned();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if ev.kind.is_access() || !ev.paths.iter().any(|p| p.file_name() == Some(&name)) {
                return;
            }
            let _ = tx.send(());
            ctx.request_repaint();
        });
        if let Ok(mut w) = watcher
            && w.watch(dir, RecursiveMode::NonRecursive).is_ok()
        {
            self.watcher = Some(w);
            self.changes = Some(rx);
        }
    }

    fn open_dialog(&mut self, ctx: &Context) {
        let mut dialog = rfd::FileDialog::new().add_filter("Markdown", &MD_EXTS);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(p) = dialog.pick_file() {
            self.open(ctx, p, true);
        }
    }

    fn handle_input(&mut self, ctx: &Context) {
        if self
            .changes
            .as_ref()
            .is_some_and(|rx| rx.try_iter().count() > 0)
        {
            self.reload(ctx);
        }

        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.open(ctx, p, true);
        }

        let (esc, open, f5, back, zoom) = ctx.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.modifiers.command && i.key_pressed(Key::O),
                i.key_pressed(Key::F5),
                (i.modifiers.alt && i.key_pressed(Key::ArrowLeft)) || i.key_pressed(Key::Backspace),
                i.zoom_delta(),
            )
        });
        if esc {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if open {
            self.open_dialog(ctx);
        }
        if f5 {
            self.reload(ctx);
        }
        if back && let Some(prev) = self.history.pop() {
            self.open(ctx, prev, false);
        }
        if zoom != 1.0 {
            ctx.set_zoom_factor((ctx.zoom_factor() * zoom).clamp(0.5, 3.0));
        }
    }

    /// Intercepta cliques em links: .md locais abrem na própria janela,
    /// outros arquivos locais viram caminho absoluto, URLs seguem para o sistema.
    fn handle_links(&mut self, ctx: &Context) {
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        let mut navigate = None;
        ctx.output_mut(|o| {
            o.commands.retain_mut(|cmd| {
                let OutputCommand::OpenUrl(open) = cmd else {
                    return true;
                };
                let Some(target) = resolve_local_link(&open.url, base.as_deref()) else {
                    return true;
                };
                if is_markdown(&target) {
                    navigate = Some(target);
                    return false;
                }
                open.url = target.to_string_lossy().into_owned();
                true
            });
        });
        if let Some(p) = navigate {
            self.open(ctx, p, true);
        }
    }
}

impl egui_software_backend::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _backend: &mut SoftwareBackend) {
        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.path.is_none() || self.error.is_some() {
                ui.centered_and_justified(|ui| {
                    let msg = self
                        .error
                        .as_deref()
                        .unwrap_or("Arraste um arquivo .md aqui ou pressione Ctrl+O");
                    ui.label(egui::RichText::new(msg).size(18.0).weak());
                });
                return;
            }

            egui::ScrollArea::vertical()
                .id_salt(self.doc_id)
                .auto_shrink(false)
                .show(ui, |ui| {
                    let dy = keyboard_scroll(ui);
                    if dy != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, dy));
                    }
                    let width = ui.available_width().min(MAX_CONTENT_WIDTH);
                    let pad = ((ui.available_width() - width) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ui.add_space(12.0);
                            CommonMarkViewer::new()
                                .max_image_width(Some(width as usize))
                                .syntax_theme_dark("base16-ocean.dark")
                                .syntax_theme_light("InspiredGitHub")
                                .show(ui, &mut self.cache, &self.text);
                            ui.add_space(40.0);
                        });
                    });
                });
        });

        self.handle_links(&ctx);
    }
}

/// Deslocamento vertical pedido pelo teclado (positivo = sobe).
fn keyboard_scroll(ui: &egui::Ui) -> f32 {
    const LINE: f32 = 60.0;
    let page = (ui.clip_rect().height() - LINE).max(LINE);
    ui.input(|i| {
        if i.key_pressed(Key::Home) {
            1e7
        } else if i.key_pressed(Key::End) {
            -1e7
        } else if i.key_pressed(Key::PageUp) || (i.modifiers.shift && i.key_pressed(Key::Space)) {
            page
        } else if i.key_pressed(Key::PageDown) || i.key_pressed(Key::Space) {
            -page
        } else if i.key_pressed(Key::ArrowUp) {
            LINE
        } else if i.key_pressed(Key::ArrowDown) {
            -LINE
        } else {
            0.0
        }
    })
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MD_EXTS.iter().any(|m| m.eq_ignore_ascii_case(e)))
}

/// Converte um link do documento em caminho local, se ele for um.
fn resolve_local_link(url: &str, base: Option<&Path>) -> Option<PathBuf> {
    let url = url.strip_prefix("file://").unwrap_or(url);
    if url.is_empty() || url.starts_with('#') || url.contains("://") || url.starts_with("mailto:") {
        return None;
    }
    let url = url.split('#').next().unwrap_or(url);
    let url = url.replace("%20", " ");
    let path = Path::new(&url);
    Some(match base {
        Some(b) if path.is_relative() => b.join(path),
        _ => path.to_path_buf(),
    })
}

/// Usa as fontes do Windows (Segoe UI / Consolas) e mantém as do egui como fallback.
fn setup_fonts(ctx: &Context) {
    let Some(windir) = std::env::var_os("WINDIR") else {
        return;
    };
    let fonts_dir = Path::new(&windir).join("Fonts");
    let mut fonts = FontDefinitions::default();
    for (name, file, family) in [
        ("segoe", "segoeui.ttf", FontFamily::Proportional),
        ("consolas", "consola.ttf", FontFamily::Monospace),
    ] {
        if let Ok(bytes) = std::fs::read(fonts_dir.join(file)) {
            fonts
                .font_data
                .insert(name.into(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.into());
        }
    }
    ctx.set_fonts(fonts);
}
