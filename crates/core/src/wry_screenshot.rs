use std::sync::mpsc::channel;

use base64::Engine;
use tao::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::{run_return::EventLoopExtRunReturn, windows::WindowBuilderExtWindows},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

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
        .with_focused(false)
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_bottom(true)
        .with_inner_size(PhysicalSize::new(0, 0))
        .with_always_on_bottom(true)
        .with_no_redirection_bitmap(true)
        .with_skip_taskbar(true)
        .with_visible_on_all_workspaces(false)
        .build(&event_loop)?;

    let (tx, rx) = channel();
    let tx_clone = tx.clone();

    let event_proxy = event_loop.create_proxy();

    let modern_screenshot_script = include_str!("../assets/modern_screenshot.js");

    let initialization_script = format!(
        r#"
{}

// Wait for page load
window.addEventListener("load", async () => {{
    // Fix for detail elements
    const style = document.createElement('style');
    style.textContent = `
      details:not([open]) > *:not(summary) {{
        display: none !important;
      }}
    `;
    document.head.appendChild(style);
    
    // Screenshot and send
    modernScreenshot.domToPng(document.body, {{
      filter: (node) => {{
        if (node.tagName === 'VIDEO') return false;
        return true;
      }}
    }}).then(dataUrl => {{
      window.ipc.postMessage(dataUrl);
    }}).catch(error => {{
      console.error('Screenshot failed:', error);
    }});
}});
"#,
        modern_screenshot_script
    );

    let builder = WebViewBuilder::new()
        .with_html(data_url)
        .with_initialization_script(initialization_script)
        .with_focused(false)
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
