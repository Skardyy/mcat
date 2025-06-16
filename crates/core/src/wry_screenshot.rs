use std::{borrow::Cow, sync::mpsc::channel};

use base64::Engine;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::WindowBuilder,
};
use wry::{WebViewBuilder, http::Response};

#[cfg(not(any(target_os = "windows", target_os = "macos",)))]
use tao::platform::unix::WindowExtUnix;
#[cfg(not(any(target_os = "windows", target_os = "macos",)))]
use wry::WebViewBuilderExtUnix;

use crate::UnwrapOrExit;

pub fn screenshot_html(data_url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Screenshot")
        .with_visible(false)
        .build(&event_loop)?;

    let (tx, rx) = channel();
    let tx_clone = tx.clone();

    let html_bytes = data_url.as_bytes().to_vec();
    let event_proxy = event_loop.create_proxy();

    let builder = WebViewBuilder::new()
        .with_custom_protocol("wry".into(), {
            move |_, _| {
                Response::builder()
                    .header("Content-Type", "text/html")
                    .body(Cow::Owned(html_bytes.clone()))
                    .unwrap_or_exit()
            }
        })
        .with_url("wry://localhost")
        .with_ipc_handler(move |arg| {
            let body = arg.body();
            let (_, body) = body.split_once(',').unwrap_or_default();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(body)
                .unwrap_or_exit();
            let _ = tx_clone.send(bytes);
            event_proxy.send_event(()).unwrap_or_exit();
        });

    // Handle different platforms like in the example
    #[cfg(any(target_os = "windows", target_os = "macos",))]
    let _webview = builder.build(&window)?;

    #[cfg(not(any(target_os = "windows", target_os = "macos",)))]
    let _webview = {
        let vbox = window.default_vbox().unwrap();
        builder.build_gtk(vbox)?
    };

    // Run the event loop briefly to load the page
    let mut result = None;
    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }

        if let Ok(data) = rx.try_recv() {
            result = Some(data);
            *control_flow = ControlFlow::Exit;
        }
    });
    result.ok_or("No data received".into())
}
