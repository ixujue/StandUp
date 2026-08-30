//! WebDAV 同步(D13):配置按"新时间戳赢"(LWW),事件流水按并集追加合并。
//! 远端布局:`<url>/standup/config.json`、`<url>/standup/events.jsonl`;
//! 凭据存系统凭证管理器(keyring),同步设置本体不参与同步。

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{driver, AppState};

const META_FILE: &str = "sync.json";
const KEYRING_SERVICE: &str = "standup-webdav";
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// 同步设置与状态(本地 `sync.json`,不参与同步;密码只在 keyring)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncSettings {
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// 本地配置最后修改时间(LWW 比较基准)。
    #[serde(default)]
    pub config_updated_at: u64,
    #[serde(default)]
    pub last_sync_at: u64,
    #[serde(default)]
    pub last_sync_ok: bool,
    #[serde(default)]
    pub last_error: String,
}

pub struct SyncMeta {
    pub path: PathBuf,
    pub settings: SyncSettings,
}

impl SyncMeta {
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join(META_FILE);
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { path, settings }
    }

    pub fn save(&self) -> std::io::Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(&self.settings)?)
    }
}

fn keyring_entry(username: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, username).map_err(|e| e.to_string())
}

pub fn get_password(username: &str) -> Result<String, String> {
    let entry = keyring_entry(username)?;
    // 凭据不存在时返回空串而非报错(首次配置)
    entry.get_password().or_else(|e| match e {
        keyring::Error::NoEntry => Ok(String::new()),
        other => Err(other.to_string()),
    })
}

pub fn set_password(username: &str, password: &str) -> Result<(), String> {
    let entry = keyring_entry(username)?;
    if password.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(password).map_err(|e| e.to_string())
    }
}

/// 事件流水并集合并:按 `t|at_ms` 去重,按时间排序。返回合并后行集。
fn merge_event_lines(local: &str, remote: &str) -> Vec<String> {
    fn parse(text: &str) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let ok = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| {
                    let t = v.get("t")?.as_str()?.to_string();
                    let at = v.get("at_ms")?.as_u64()?;
                    Some((at, t))
                });
            if let Some((at, t)) = ok {
                out.push((at, line.to_string()));
                let _ = t;
            }
        }
        out
    }
    let mut seen: Vec<(u64, String)> = Vec::new();
    seen.extend(parse(local));
    seen.extend(parse(remote));
    seen.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    seen.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    seen.into_iter().map(|(_, l)| l).collect()
}

/// 远端配置信封:{"updated_at": ms, "config": {...}}
#[derive(Serialize, Deserialize)]
struct ConfigEnvelope {
    updated_at: u64,
    config: standup_core::Config,
}

fn dav_url(base: &str, file: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/standup/{file}")
}

/// 一次性同步。任何一步失败即整体失败,错误写回 meta 供 UI 展示。
pub fn sync_once(app: &tauri::AppHandle) -> Result<String, String> {
    let started = std::time::Instant::now();
    let state = app.state::<AppState>();
    let (settings, events_path) = {
        let st = state.store.lock().unwrap();
        let meta_dir = st.dir.clone();
        (st.sync_meta.settings.clone(), meta_dir.join("events.jsonl"))
    };
    if !settings.enabled {
        return Err("同步未启用".into());
    }
    if settings.url.is_empty() || settings.username.is_empty() {
        return Err("请先填写服务器地址与账号".into());
    }
    let password = get_password(&settings.username)?;
    if password.is_empty() {
        return Err("请先填写应用密码".into());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    // 0) 确保远端集合存在(MKCOL;405/301/409 = 已存在,忽略)
    let base = settings.url.trim_end_matches('/');
    let mkcol = reqwest::Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;
    let _ = client
        .request(mkcol, format!("{base}/standup"))
        .basic_auth(&settings.username, Some(&password))
        .send();

    // 1) 配置:LWW
    let local_config = state.store.lock().unwrap().config.clone();
    let remote_cfg_url = dav_url(&settings.url, "config.json");
    let resp = client
        .get(&remote_cfg_url)
        .basic_auth(&settings.username, Some(&password))
        .send()
        .map_err(|e| format!("网络错误: {e}"))?;
    match resp.status() {
        reqwest::StatusCode::OK => {
            let env: ConfigEnvelope = resp.json().map_err(|e| format!("远端配置解析失败: {e}"))?;
            if env.updated_at > settings.config_updated_at {
                // 远端较新:采用远端配置(走与 save_config 相同的生效路径)
                crate::commands::apply_remote_config(app, env.config.clone())?;
                let mut meta_lock = state.store.lock().unwrap();
                meta_lock.sync_meta.settings.config_updated_at = env.updated_at;
                let _ = meta_lock.sync_meta.save();
                log::info!("sync: 采用远端配置(updated_at={})", env.updated_at);
            } else {
                let env = ConfigEnvelope {
                    updated_at: settings.config_updated_at,
                    config: local_config,
                };
                let body = serde_json::to_string(&env).map_err(|e| e.to_string())?;
                client
                    .put(&remote_cfg_url)
                    .basic_auth(&settings.username, Some(&password))
                    .header("Content-Type", "application/json")
                    .body(body)
                    .send()
                    .map_err(|e| format!("上传配置失败: {e}"))?
                    .error_for_status()
                    .map_err(|e| format!("上传配置失败: {e}"))?;
            }
        }
        reqwest::StatusCode::NOT_FOUND => {
            let env = ConfigEnvelope {
                updated_at: settings.config_updated_at,
                config: local_config,
            };
            let body = serde_json::to_string(&env).map_err(|e| e.to_string())?;
            client
                .put(&remote_cfg_url)
                .basic_auth(&settings.username, Some(&password))
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .map_err(|e| format!("上传配置失败: {e}"))?
                .error_for_status()
                .map_err(|e| format!("上传配置失败: {e}"))?;
        }
        s => return Err(format!("服务器返回 {s}(检查地址/账号/应用密码)")),
    }

    // 2) 事件流水:并集合并
    let local_events = fs::read_to_string(&events_path).unwrap_or_default();
    let remote_events_url = dav_url(&settings.url, "events.jsonl");
    let resp = client
        .get(&remote_events_url)
        .basic_auth(&settings.username, Some(&password))
        .send()
        .map_err(|e| format!("网络错误: {e}"))?;
    let remote_text = match resp.status() {
        reqwest::StatusCode::OK => resp.text().map_err(|e| e.to_string())?,
        reqwest::StatusCode::NOT_FOUND => String::new(),
        s => return Err(format!("服务器返回 {s}")),
    };
    let merged = merge_event_lines(&local_events, &remote_text);
    let merged_text = if merged.is_empty() {
        String::new()
    } else {
        let mut t = merged.join("\n");
        t.push('\n');
        t
    };
    if merged_text != local_events {
        fs::write(&events_path, &merged_text).map_err(|e| format!("写本地流水失败: {e}"))?;
        state.store.lock().unwrap().reload_events();
    }
    if merged_text != remote_text {
        client
            .put(&remote_events_url)
            .basic_auth(&settings.username, Some(&password))
            .header("Content-Type", "application/octet-stream")
            .body(merged_text)
            .send()
            .map_err(|e| format!("上传流水失败: {e}"))?
            .error_for_status()
            .map_err(|e| format!("上传流水失败: {e}"))?;
    }

    // 3) 记录成功状态
    {
        let mut st = state.store.lock().unwrap();
        st.sync_meta.settings.last_sync_at = driver::now_ms();
        st.sync_meta.settings.last_sync_ok = true;
        st.sync_meta.settings.last_error.clear();
        let _ = st.sync_meta.save();
    }
    let _ = started;
    Ok("同步完成".into())
}

/// 同步失败时记录错误(供 UI 展示),不动其他状态。
pub fn record_failure(app: &tauri::AppHandle, err: &str) {
    let state = app.state::<AppState>();
    let mut st = state.store.lock().unwrap();
    st.sync_meta.settings.last_sync_at = driver::now_ms();
    st.sync_meta.settings.last_sync_ok = false;
    st.sync_meta.settings.last_error = err.to_string();
    let _ = st.sync_meta.save();
}

/// 周期同步线程:启用时每 15 分钟一次,启动后先等 10 秒做首推。
pub fn spawn_timer(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        run_silent(&app);
        loop {
            std::thread::sleep(Duration::from_secs(15 * 60));
            run_silent(&app);
        }
    });
}

fn run_silent(app: &tauri::AppHandle) {
    let enabled = app
        .try_state::<AppState>()
        .map(|st| st.store.lock().unwrap().sync_meta.settings.enabled)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    match sync_once(app) {
        Ok(msg) => {
            log::info!("sync: {msg}");
            let _ = tauri::Emitter::emit(app, "sync-changed", ());
        }
        Err(e) => {
            log::warn!("sync 失败: {e}");
            record_failure(app, &e);
            let _ = tauri::Emitter::emit(app, "sync-changed", ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::merge_event_lines;

    #[test]
    fn merge_unions_and_dedupes() {
        let a = "{\"t\":\"reminder\",\"at_ms\":100}\n{\"t\":\"away\",\"at_ms\":200}";
        let b = "{\"t\":\"away\",\"at_ms\":200}\n{\"t\":\"break\",\"at_ms\":150}";
        let m = merge_event_lines(a, b);
        assert_eq!(
            m,
            vec![
                "{\"t\":\"reminder\",\"at_ms\":100}",
                "{\"t\":\"break\",\"at_ms\":150}",
                "{\"t\":\"away\",\"at_ms\":200}",
            ]
        );
    }

    #[test]
    fn merge_tolerates_garbage_lines() {
        let a = "not-json\n{\"t\":\"reminder\",\"at_ms\":1}";
        let b = "";
        assert_eq!(merge_event_lines(a, b), vec!["{\"t\":\"reminder\",\"at_ms\":1}"]);
    }
}
