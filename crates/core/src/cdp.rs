use base64::Engine;
use base64::engine::general_purpose;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::UnwrapOrExit;
use crate::fetch_manager::BrowserConfig;

pub struct ChromeHeadless {
    process: Arc<Mutex<Child>>,
    port: u16,
}

fn find_available_port() -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

impl ChromeHeadless {
    pub async fn new(uri: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let browser_config = BrowserConfig::default().ok_or("chromium isn't installed. either install it manually (chrome/msedge will do so too) or call `mcat --fetch-chromium`")?;
        let path = browser_config.path;
        let port = find_available_port()?;
        let process = Command::new(path)
            .args(&[
                // Core headless setup
                "--headless=new",
                &format!("--remote-debugging-port={}", port),
                "--disable-gpu",
                "--hide-scrollbars",
                "--force-color-profile=srgb",
                "--disable-dev-shm-usage",
                "--single-process",
                "--no-zygote",
                "--no-sandbox",
                uri,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        // make sure the process is always killed 100% (windows suck)
        let shutdown = rasteroid::term_misc::setup_signal_handler();
        let pc_arc = Arc::new(Mutex::new(process));
        let shutdown_arc = pc_arc.clone();
        tokio::spawn(async move {
            loop {
                if shutdown.load(Ordering::SeqCst) {
                    let mut process = shutdown_arc.lock().unwrap_or_exit();
                    process.kill().unwrap_or_exit();
                    std::process::exit(1);
                };
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });

        let instance = Self {
            process: pc_arc,
            port,
        };

        instance.wait_for_server().await?;
        Ok(instance)
    }

    async fn wait_for_server(&self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            match timeout(
                Duration::from_millis(2000),
                TcpStream::connect(format!("127.0.0.1:{}", self.port)),
            )
            .await
            {
                Ok(Ok(_)) => {
                    return Ok(());
                }
                Ok(Err(_)) | Err(_) => {
                    tokio::task::yield_now().await;
                }
            }
        }
    }

    pub async fn capture_screenshot(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let endpoint = self.get_websocket_endpoint().await?;
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&endpoint).await?;

        // Get the page ready
        self.send_command(&mut ws_stream, 1, "Page.enable", None, false)
            .await?;
        self.wait_for_load_event(&mut ws_stream).await?;

        //  Get layout metrics
        let metrics = self
            .send_command(&mut ws_stream, 2, "Page.getLayoutMetrics", None, true)
            .await?;
        let width = metrics["contentSize"]["width"].as_f64().unwrap();
        let height = metrics["contentSize"]["height"].as_f64().unwrap();
        let desired_width = 1920.0;
        let desired_height = 1080.0;
        let scalex = if width > desired_width {
            1.0
        } else {
            desired_width / width
        };
        let scaleh = if height > desired_height {
            1.0
        } else {
            desired_height / height
        };
        let scale = scalex.min(scaleh).round() as u16;

        // Set viewport
        self.send_command(
            &mut ws_stream,
            3,
            "Emulation.setDeviceMetricsOverride",
            Some(json!({
                "mobile": false,
                "width": width,
                "height": height,
                "deviceScaleFactor": scale
            })),
            false,
        )
        .await?;

        // Remove background
        self.send_command(
            &mut ws_stream,
            6,
            "Emulation.setDefaultBackgroundColorOverride",
            Some(json!({
                "color": {
                    "r": 0,
                    "g": 0,
                    "b": 0,
                    "a": 0
                }
            })),
            false,
        )
        .await?;

        // Capture screenshot
        let response = self
            .send_command(
                &mut ws_stream,
                4,
                "Page.captureScreenshot",
                Some(json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                })),
                true,
            )
            .await?;

        let screenshot_data = response["data"]
            .as_str()
            .ok_or("failed to get screenshot")?;
        Ok(general_purpose::STANDARD.decode(screenshot_data)?)
    }

    async fn get_websocket_endpoint(&self) -> Result<String, Box<dyn std::error::Error>> {
        // shouldn't really go over 1 and even
        let max_attempts = 10;
        for _ in 1..=max_attempts {
            let url = format!("http://127.0.0.1:{}/json", self.port);
            let body = reqwest::get(&url).await?.text().await?;
            let json: Value = serde_json::from_str(&body)?;
            if let Some(arr) = json.as_array() {
                if let Some(page) = arr.iter().find(|entry| entry["type"] == "page") {
                    if let Some(ws_url) = page["webSocketDebuggerUrl"].as_str() {
                        return Ok(ws_url.to_owned());
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err("Failed to get websocket for headless chrome".into())
    }

    async fn send_command(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        id: u64,
        method: &str,
        params: Option<Value>,
        wait: bool,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let command = match params {
            Some(params) => json!({
                "id": id,
                "method": method,
                "params": params
            }),
            None => json!({
                "id": id,
                "method": method
            }),
        };

        ws_stream
            .send(Message::Text(command.to_string().into()))
            .await?;

        if wait {
            while let Some(msg) = ws_stream.next().await {
                let msg = msg?;
                if let Message::Text(text) = msg {
                    let response: Value = serde_json::from_str(&text)?;
                    if response["id"] == id {
                        if let Some(error) = response.get("error") {
                            return Err(format!("Chrome error: {}", error).into());
                        }
                        return Ok(response["result"].to_owned());
                    }
                }
            }

            Err("WebSocket connection closed unexpectedly".into())
        } else {
            Ok(Value::Null)
        }
    }

    async fn wait_for_load_event(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // perhaps it was already loaded..
        let ready_state = self
            .send_command(
                ws_stream,
                100,
                "Runtime.evaluate",
                Some(json!({
                "expression": "document.readyState"
                })),
                true,
            )
            .await?;
        if let Some(state) = ready_state["result"]["value"].as_str() {
            if state == "complete" {
                return Ok(());
            }
        }

        while let Some(msg) = ws_stream.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    if json["method"] == "Page.loadEventFired" {
                        return Ok(());
                    }
                }
            }
        }

        Err("WebSocket closed before loadEventFired".into())
    }
}

impl Drop for ChromeHeadless {
    fn drop(&mut self) {
        let mut process = self.process.lock().unwrap_or_exit();
        let _ = process.kill();
    }
}
