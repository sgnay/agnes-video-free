//! System Tray implementation using ksni

use ksni::{menu::*, Handle, MenuItem, Tray};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    ToggleWindow,
    QuickOcr,
    OpenFile,
    Quit,
}

pub struct AppTray {
    sender: Sender<TrayCommand>,
}

impl AppTray {
    pub fn new(sender: Sender<TrayCommand>) -> Self {
        Self { sender }
    }
}

impl Tray for AppTray {
    fn id(&self) -> String {
        "simple-ocr".to_string()
    }

    fn title(&self) -> String {
        "Simple OCR".to_string()
    }

    fn icon_name(&self) -> String {
        "document-open-symbolic".to_string()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "显示/隐藏窗口".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::ToggleWindow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "快速剪贴板 OCR".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::QuickOcr);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "打开文件".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::OpenFile);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn_tray(sender: Sender<TrayCommand>) -> Result<Handle<AppTray>, String> {
    let tray = AppTray::new(sender);
    let service = ksni::TrayService::new(tray);
    let handle = service.handle();
    service.spawn();
    Ok(handle)
}
