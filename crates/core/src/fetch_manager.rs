use std::{
    env,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ffmpeg_sidecar::{
    command::FfmpegCommand,
    download::{download_ffmpeg_package, ffmpeg_download_url, unpack_ffmpeg},
};
use tracing::{debug, info, warn};
use zip::ZipArchive;

use crate::prompter::get_rt;

pub fn is_chromium_installed() -> bool {
    BrowserConfig::default().is_some()
}
pub fn is_ffmpeg_installed() -> bool {
    let cache_path = get_cache_path();
    let mut ffmpeg_path = cache_path.join("ffmpeg").join("ffmpeg");
    if cfg!(windows) {
        ffmpeg_path.set_extension("exe");
    }
    if ffmpeg_path.exists() {
        return true;
    }
    ffmpeg_sidecar::command::ffmpeg_is_installed()
}

pub fn fetch_chromium() -> Result<()> {
    let cr = ChromeRevision::default();
    match cr {
        Some(cr) => cr.download(),
        None => anyhow::bail!("Platform isn't supported for fetching chromium"),
    }
}

pub fn is_poppler_installed() -> bool {
    which::which("pdftocairo").is_ok() || which::which("pdftoppm").is_ok()
}

pub fn fetch_ffmpeg() -> Result<()> {
    let cache_path = get_cache_path();
    let des = cache_path.join("ffmpeg");
    if !des.exists() {
        fs::create_dir_all(des.clone())?;
    }
    let download_url = ffmpeg_download_url()?;
    eprintln!(
        "downloading ffmpeg into {}, it may take a while..",
        des.display()
    );
    let archive_path = download_ffmpeg_package(download_url, &des)?;
    eprintln!("unpacking ffmpeg");
    unpack_ffmpeg(&archive_path, &des)?;
    eprintln!("done!");
    Ok(())
}

pub fn clean() -> Result<()> {
    let cache_path = get_cache_path();
    eprintln!("deleting: {}", cache_path.display());
    if cache_path.exists() {
        fs::remove_dir_all(cache_path)?;
    }
    eprintln!("done!");
    Ok(())
}

pub fn get_cache_path() -> PathBuf {
    let base_dir = dirs::cache_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir);

    base_dir.join("mcat")
}

pub fn get_ffmpeg() -> Option<FfmpegCommand> {
    if ffmpeg_sidecar::command::ffmpeg_is_installed() {
        info!("using system ffmpeg");
        return Some(FfmpegCommand::new());
    }
    let cache_path = get_cache_path();
    let mut ffmpeg_path = cache_path.join("ffmpeg").join("ffmpeg");
    if cfg!(windows) {
        ffmpeg_path.set_extension("exe");
    }
    if ffmpeg_path.exists() {
        info!(path = %ffmpeg_path.display(), "using cached ffmpeg");
        return Some(FfmpegCommand::new_with_path(ffmpeg_path));
    }
    debug!("ffmpeg not found");
    None
}

pub struct BrowserConfig {
    pub path: PathBuf,
}

impl BrowserConfig {
    pub fn default() -> Option<Self> {
        let path = BrowserConfig::auto_detect_path()?;
        Some(BrowserConfig { path })
    }

    fn auto_detect_path() -> Option<PathBuf> {
        if let Some(path) = get_by_env_var() {
            info!(path = %path.display(), "found browser via CHROME env var");
            return Some(path);
        }

        if let Some(path) = get_by_name() {
            info!(path = %path.display(), "found browser in PATH");
            return Some(path);
        }

        #[cfg(windows)]
        if let Some(path) = get_by_registry() {
            info!(path = %path.display(), "found browser via registry");
            return Some(path);
        }

        if let Some(path) = get_by_path() {
            info!(path = %path.display(), "found browser at known path");
            return Some(path);
        }

        let cr = ChromeRevision::default()?;
        let p = cr.path();
        if p.exists() {
            info!(path = %p.display(), "found cached chromium");
            return Some(p);
        }

        warn!("no browser found");
        None
    }
}

fn get_by_env_var() -> Option<PathBuf> {
    if let Ok(path) = env::var("CHROME")
        && Path::new(&path).exists()
    {
        return Some(path.into());
    }

    None
}

fn get_by_name() -> Option<PathBuf> {
    let default_apps = [
        ("chrome"),
        ("chrome-browser"),
        ("google-chrome-stable"),
        ("google-chrome-beta"),
        ("google-chrome-dev"),
        ("google-chrome-unstable"),
        ("chromium"),
        ("chromium-browser"),
        ("msedge"),
        ("microsoft-edge"),
        ("microsoft-edge-stable"),
        ("microsoft-edge-beta"),
        ("microsoft-edge-dev"),
    ];
    for app in default_apps {
        if let Ok(path) = which::which(app) {
            return Some(path);
        }
    }

    None
}

fn get_by_path() -> Option<PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    let default_paths: [&str; 2] = ["/opt/chromium.org/chromium", "/opt/google/chrome"];
    #[cfg(windows)]
    let default_paths = [r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"];
    #[cfg(target_os = "macos")]
    let default_paths = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
        "/Applications/Google Chrome Dev.app/Contents/MacOS/Google Chrome Dev",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Microsoft Edge Beta.app/Contents/MacOS/Microsoft Edge Beta",
        "/Applications/Microsoft Edge Dev.app/Contents/MacOS/Microsoft Edge Dev",
        "/Applications/Microsoft Edge Canary.app/Contents/MacOS/Microsoft Edge Canary",
    ];

    for path in default_paths {
        if Path::new(path).exists() {
            return Some(path.into());
        }
    }

    None
}

#[cfg(windows)]
fn get_by_registry() -> Option<PathBuf> {
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe")
        .or_else(|_| {
            winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe")
        })
        .and_then(|key| key.get_value::<String, _>(""))
        .map(PathBuf::from)
        .ok()
}

struct ChromeRevision {
    revision: u32,
    platform: Platform,
    host: String,
}

impl ChromeRevision {
    pub fn default() -> Option<Self> {
        Some(ChromeRevision {
            revision: 1045629,
            platform: Platform::current()?,
            host: "https://storage.googleapis.com".to_owned(),
        })
    }
    pub fn download(&self) -> Result<()> {
        let url = self.platform.as_url(&self.host, self.revision);
        eprintln!("Downloading {url}");

        let cache_path = get_cache_path();
        let mut folder_path = cache_path.join("chromium");
        folder_path.push(self.platform.as_folder(self.revision));
        let archive_path = folder_path.with_extension("zip");
        eprintln!("Location: {}", archive_path.display());
        if !folder_path.exists() {
            fs::create_dir_all(&folder_path)?;
        }

        // Create Archive
        let file = File::create(&archive_path).context("Failed to create archive file")?;
        let mut file = BufWriter::new(file);

        // Download and Unzip
        let url = url.parse::<reqwest::Url>().context("Invalid archive url")?;
        get_rt().block_on(async {
            // Download
            let mut res = reqwest::get(url)
                .await
                .context("Failed to send request to host")?;
            if res.status() != reqwest::StatusCode::OK {
                anyhow::bail!("Invalid archive url");
            }

            while let Some(chunk) = res.chunk().await.context("Failed to read response chunk")? {
                file.write(&chunk)
                    .context("Failed to write to archive file")?;
            }
            file.flush().context("Failed to flush to disk")?;
            eprintln!("Finished Downloading!");

            // Unzip
            eprintln!("Unziping");
            fs::create_dir_all(&folder_path).context("Failed to create folder")?;
            let file = fs::File::open(&archive_path).context("Failed to open archive")?;
            let mut archive = ZipArchive::new(file).context("Failed to unzip archive")?;
            archive.extract(folder_path)?;
            let _ = fs::remove_file(archive_path);

            eprintln!("Done!");
            Ok(())
        })
    }
    pub fn path(&self) -> PathBuf {
        let cache_path = get_cache_path();
        let mut download_path = cache_path.join("chromium");
        download_path.push(self.platform.as_folder(self.revision));
        download_path.push(self.platform.as_archive(self.revision));
        self.platform.as_executable(&download_path)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Platform {
    Linux,
    Mac,
    MacArm,
    Win32,
    Win64,
}

impl Platform {
    pub fn current() -> Option<Self> {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;

        match (os, arch) {
            ("linux", "x86_64") => Some(Self::Linux),
            ("macos", "x86_64") => Some(Self::Mac),
            ("macos", "aarch64") => Some(Self::MacArm),
            ("windows", "x86") => Some(Self::Win32),
            ("windows", "x86_64") => Some(Self::Win64),
            _ => None,
        }
    }

    fn as_archive(&self, revision: u32) -> String {
        match self {
            Self::Linux => "chrome-linux".to_string(),
            Self::Mac | Self::MacArm => "chrome-mac".to_string(),
            Self::Win32 | Self::Win64 => {
                if revision > 591_479 {
                    "chrome-win".to_string()
                } else {
                    "chrome-win32".to_string()
                }
            }
        }
    }

    pub fn as_executable(&self, folder_path: &Path) -> PathBuf {
        let mut path = folder_path.to_path_buf();
        match self {
            Self::Linux => path.push("chrome"),
            Self::Mac | Self::MacArm => {
                path.push("Chromium.app");
                path.push("Contents");
                path.push("MacOS");
                path.push("Chromium")
            }
            Self::Win32 | Self::Win64 => path.push("chrome.exe"),
        }
        path
    }

    pub fn as_url(&self, host: &str, revision: u32) -> String {
        let name = match self {
            Self::Linux => "Linux_x64",
            Self::Mac => "Mac",
            Self::MacArm => "Mac_Arm",
            Self::Win32 => "Win",
            Self::Win64 => "Win_x64",
        };
        let archive = self.as_archive(revision);
        format!(
            "{}/chromium-browser-snapshots/{}/{}/{}.zip",
            host, name, revision, archive
        )
    }

    pub fn as_folder(&self, revision: u32) -> String {
        let platform = match self {
            Self::Linux => "linux",
            Self::Mac => "mac",
            Self::MacArm => "mac_arm",
            Self::Win32 => "win32",
            Self::Win64 => "win64",
        };
        format!("{platform}-{revision}")
    }
}
