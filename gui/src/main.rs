#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod charts;
mod icons;
mod settings;
mod theme;

use std::path::PathBuf;

use iced::{window, Size, Task};

use app::{LayaCoreApp, Message};

fn main() -> iced::Result {
    let startup_path = std::env::args().nth(1).map(PathBuf::from);
    let icon = window::icon::from_file_data(app::ICON_PNG_BYTES, None).ok();

    iced::application(
        move || {
            let mut app = LayaCoreApp::new();
            let task = if let Some(path) = startup_path.clone().filter(|p| p.exists()) {
                app.update(Message::AnalyzePath(path))
            } else {
                Task::none()
            };
            (app, task)
        },
        LayaCoreApp::update,
        LayaCoreApp::view,
    )
    .title(LayaCoreApp::title)
    .theme(LayaCoreApp::theme)
    .subscription(LayaCoreApp::subscription)
    .window(window::Settings {
        size: Size::new(1240.0, 820.0),
        min_size: Some(Size::new(940.0, 620.0)),
        position: window::Position::Centered,
        icon,
        ..Default::default()
    })
    .antialiasing(false)
    .run()
}
