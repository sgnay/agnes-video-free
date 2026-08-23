//! Simple OCR Desktop Application using GPUI

use gpui::{
    px, size, App, AppContext, Bounds, SharedString, TitlebarOptions, WindowBounds, WindowOptions,
};
use gpui_platform::application;
use std::path::PathBuf;
use std::sync::mpsc::channel;

mod app;
mod sources;
mod state;
mod tray;

use app::{perform_quick_clipboard_ocr, OcrAppView};
use tray::{spawn_tray, TrayCommand};

fn main() {
    let input_path = std::env::args().nth(1).map(PathBuf::from);

    let (tray_tx, tray_rx) = channel::<TrayCommand>();

    let _tray_handle = match spawn_tray(tray_tx) {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("Warning: Could not start tray service: {}", err);
            None
        }
    };

    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(SharedString::from("Simple OCR")),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_window, cx| {
                    cx.new(|_cx| {
                        let mut view = OcrAppView::new();
                        if let Some(path) = &input_path {
                            let is_pdf = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.to_lowercase())
                                == Some("pdf".to_string());
                            if is_pdf {
                                view.set_pdf(path.clone());
                            } else {
                                view.set_image(path.clone());
                            }
                        }
                        view
                    })
                },
            )
            .unwrap();

        cx.spawn(async move |cx| {
            loop {
                if let Ok(cmd) = tray_rx.try_recv() {
                    match cmd {
                        TrayCommand::ToggleWindow => {
                            let _ = window.update(cx, |_view, _window, cx| {
                                cx.activate(true);
                            });
                        }
                        TrayCommand::QuickOcr => {
                            let _ = cx
                                .background_executor()
                                .spawn(async move {
                                    let _ = perform_quick_clipboard_ocr();
                                })
                                .await;
                        }
                        TrayCommand::OpenFile => {
                            let _ = window.update(cx, |_view, _window, cx| {
                                cx.activate(true);
                            });
                        }
                        TrayCommand::Quit => {
                            let _ = window.update(cx, |_view, _window, cx| {
                                cx.quit();
                            });
                            break;
                        }
                    }
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();

        cx.activate(true);
    });
}