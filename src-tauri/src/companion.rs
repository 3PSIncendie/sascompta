//! Pont compagnon mobile — écoute HTTP LAN (Wi‑Fi local, sans cloud).
//!
//! Protocole MVP (`/v1`) :
//! - `GET  /v1/info` — statut public (nom société, appairage requis)
//! - `POST /v1/pair` — échange code court → jeton de session
//! - `POST /v1/upload` — envoi photo/PDF (Bearer) → file d’import documents
//!
//! Le serveur ne démarre que sur demande (workspace ouvert) et s’arrête à la
//! fermeture du workspace ou via `companion_stop`.

use crate::db::{self, AppState};
use chrono::{DateTime, Duration, Utc};
use qrcode::render::svg;
use qrcode::QrCode;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{Cursor, Read};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter, State};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub const DEFAULT_PORT: u16 = 17832;
pub const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
const PAIRING_TTL_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionStatus {
    pub running: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub org_name: Option<String>,
    pub pairing_code: Option<String>,
    pub pairing_expires_at: Option<String>,
    pub session_paired: bool,
    pub qr_payload: Option<String>,
    pub qr_svg: Option<String>,
    pub received_count: u64,
}

impl CompanionStatus {
    fn idle() -> Self {
        Self {
            running: false,
            host: None,
            port: None,
            org_name: None,
            pairing_code: None,
            pairing_expires_at: None,
            session_paired: false,
            qr_payload: None,
            qr_svg: None,
            received_count: 0,
        }
    }
}

#[derive(Clone)]
struct LiveConfig {
    host: String,
    port: u16,
    org_name: String,
    pairing_code: String,
    pairing_expires_at: DateTime<Utc>,
    session_token: String,
    session_paired: bool,
    received_count: u64,
}

struct Running {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    config: Arc<Mutex<LiveConfig>>,
}

pub struct CompanionHub {
    inner: Mutex<Option<Running>>,
}

impl CompanionHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn status_locked(running: &Option<Running>) -> CompanionStatus {
        let Some(r) = running else {
            return CompanionStatus::idle();
        };
        let cfg = match r.config.lock() {
            Ok(c) => c.clone(),
            Err(_) => return CompanionStatus::idle(),
        };
        let qr_payload = pairing_payload(&cfg);
        let qr_svg = render_qr_svg(&qr_payload).ok();
        CompanionStatus {
            running: true,
            host: Some(cfg.host),
            port: Some(cfg.port),
            org_name: Some(cfg.org_name),
            pairing_code: Some(cfg.pairing_code),
            pairing_expires_at: Some(cfg.pairing_expires_at.to_rfc3339()),
            session_paired: cfg.session_paired,
            qr_payload: Some(qr_payload),
            qr_svg,
            received_count: cfg.received_count,
        }
    }

    pub fn status(&self) -> CompanionStatus {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Self::status_locked(&guard)
    }

    pub fn stop(&self) -> CompanionStatus {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut running) = guard.take() {
            running.stop.store(true, Ordering::SeqCst);
            // Débloque accept() en ouvrant une connexion locale.
            if let Ok(cfg) = running.config.lock() {
                let _ = std::net::TcpStream::connect(format!("127.0.0.1:{}", cfg.port));
            }
            if let Some(join) = running.join.take() {
                let _ = join.join();
            }
        }
        CompanionStatus::idle()
    }

    pub fn rotate_pairing(&self) -> Result<CompanionStatus, String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(running) = guard.as_ref() else {
            return Err("Le pont compagnon n’est pas démarré.".into());
        };
        let mut cfg = running
            .config
            .lock()
            .map_err(|e| e.to_string())?;
        cfg.pairing_code = random_pairing_code();
        cfg.pairing_expires_at = Utc::now() + Duration::seconds(PAIRING_TTL_SECS);
        cfg.session_paired = false;
        cfg.session_token = random_token();
        drop(cfg);
        Ok(Self::status_locked(&Some(Running {
            stop: running.stop.clone(),
            join: None,
            config: running.config.clone(),
        })))
    }
}

fn pairing_payload(cfg: &LiveConfig) -> String {
    json!({
        "v": 1,
        "app": "sascompta",
        "host": cfg.host,
        "port": cfg.port,
        "code": cfg.pairing_code,
        "org": cfg.org_name,
        "expiresAt": cfg.pairing_expires_at.to_rfc3339(),
    })
    .to_string()
}

fn render_qr_svg(payload: &str) -> Result<String, String> {
    let code = QrCode::new(payload.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(180, 180)
        .dark_color(svg::Color("#1a3d2e"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

fn random_pairing_code() -> String {
    let mut buf = [0u8; 4];
    OsRng.fill_bytes(&mut buf);
    let n = u32::from_le_bytes(buf) % 1_000_000;
    format!("{n:06}")
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn guess_lan_ip() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if !ip.is_loopback() {
                    return ip.to_string();
                }
            }
        }
    }
    "127.0.0.1".into()
}

fn bind_listener(preferred: u16) -> Result<(TcpListener, u16), String> {
    for port in preferred..preferred.saturating_add(20) {
        match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))) {
            Ok(listener) => {
                listener
                    .set_nonblocking(false)
                    .map_err(|e| e.to_string())?;
                return Ok((listener, port));
            }
            Err(_) => continue,
        }
    }
    Err(format!(
        "Impossible d’ouvrir un port LAN autour de {preferred}."
    ))
}

fn cors_headers() -> Vec<Header> {
    [
        "Access-Control-Allow-Origin: *",
        "Access-Control-Allow-Methods: GET, POST, OPTIONS",
        "Access-Control-Allow-Headers: Authorization, Content-Type, X-Doc-Type, X-File-Name",
        "Access-Control-Max-Age: 600",
    ]
    .into_iter()
    .filter_map(|h| h.parse().ok())
    .collect()
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response<Cursor<Vec<u8>>> {
    let bytes = body.to_string().into_bytes();
    let mut resp = Response::from_data(bytes).with_status_code(status);
    for h in cors_headers() {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..])
    {
        resp.add_header(h);
    }
    resp
}

fn read_body(request: &mut Request, limit: usize) -> Result<Vec<u8>, String> {
    let len = request.body_length().unwrap_or(0);
    if len > limit {
        return Err(format!(
            "Fichier trop volumineux (max {} Mo).",
            limit / (1024 * 1024)
        ));
    }
    let mut buf = Vec::new();
    request
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() > limit {
        return Err(format!(
            "Fichier trop volumineux (max {} Mo).",
            limit / (1024 * 1024)
        ));
    }
    Ok(buf)
}

fn header_field_name(h: &tiny_http::Header) -> String {
    h.field.as_str().to_string().to_ascii_lowercase()
}

fn header_field_value(h: &tiny_http::Header) -> String {
    h.value.as_str().to_string()
}

fn bearer_token(request: &Request) -> Option<String> {
    request.headers().iter().find_map(|h| {
        if header_field_name(h) != "authorization" {
            return None;
        }
        let v = header_field_value(h);
        let v = v.trim();
        v.strip_prefix("Bearer ")
            .or_else(|| v.strip_prefix("bearer "))
            .map(|s| s.trim().to_string())
    })
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    let want = name.to_ascii_lowercase();
    request.headers().iter().find_map(|h| {
        if header_field_name(h) == want {
            Some(header_field_value(h).trim().to_string())
        } else {
            None
        }
    })
}

fn extension_for_mime(mime: &str, fallback_name: &str) -> String {
    if let Some(ext) = PathBuf::from(fallback_name)
        .extension()
        .and_then(|e| e.to_str())
    {
        if !ext.is_empty() {
            return ext.to_ascii_lowercase();
        }
    }
    match mime {
        "application/pdf" => "pdf".into(),
        "image/png" => "png".into(),
        "image/jpeg" | "image/jpg" => "jpg".into(),
        "image/webp" => "webp".into(),
        "image/tiff" => "tiff".into(),
        _ => "bin".into(),
    }
}

fn sanitize_file_name(raw: &str) -> String {
    let leaf = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_start_matches('.');
    let base = leaf
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = base.trim_matches(['_', '.']);
    if trimmed.is_empty() {
        "document".into()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn valid_doc_hint(hint: &str) -> bool {
    matches!(
        hint,
        "supplier_invoice"
            | "customer_invoice"
            | "expense_receipt"
            | "bank_statement"
            | "ticket"
            | "unknown"
    )
}

fn handle_request(
    app: Option<&AppHandle>,
    state: &AppState,
    config: &Arc<Mutex<LiveConfig>>,
    mut request: Request,
) {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let method = request.method().clone();

    if method == Method::Options {
        let mut resp = Response::empty(StatusCode(204));
        for h in cors_headers() {
            resp.add_header(h);
        }
        let _ = request.respond(resp);
        return;
    }

    if path == "/v1/info" && method == Method::Get {
        let cfg = match config.lock() {
            Ok(c) => c.clone(),
            Err(e) => {
                let _ = request.respond(json_response(
                    StatusCode(500),
                    json!({ "error": e.to_string() }),
                ));
                return;
            }
        };
        let pairing_ok = Utc::now() <= cfg.pairing_expires_at;
        let _ = request.respond(json_response(
            StatusCode(200),
            json!({
                "app": "sascompta",
                "v": 1,
                "orgName": cfg.org_name,
                "host": cfg.host,
                "port": cfg.port,
                "pairingRequired": true,
                "pairingOpen": pairing_ok,
                "sessionPaired": cfg.session_paired,
                "receivedCount": cfg.received_count,
            }),
        ));
        return;
    }

    if path == "/v1/pair" && method == Method::Post {
        let body = match read_body(&mut request, 4096) {
            Ok(b) => b,
            Err(e) => {
                let _ = request.respond(json_response(StatusCode(400), json!({ "error": e })));
                return;
            }
        };
        let parsed: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => {
                let _ = request.respond(json_response(
                    StatusCode(400),
                    json!({ "error": "JSON invalide (attendu : {\"code\":\"123456\"})." }),
                ));
                return;
            }
        };
        let code = parsed
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut cfg = match config.lock() {
            Ok(c) => c,
            Err(e) => {
                let _ = request.respond(json_response(
                    StatusCode(500),
                    json!({ "error": e.to_string() }),
                ));
                return;
            }
        };
        if Utc::now() > cfg.pairing_expires_at {
            let _ = request.respond(json_response(
                StatusCode(401),
                json!({ "error": "Code d’appairage expiré. Régénérez-le sur le PC." }),
            ));
            return;
        }
        if code != cfg.pairing_code {
            let _ = request.respond(json_response(
                StatusCode(401),
                json!({ "error": "Code d’appairage incorrect." }),
            ));
            return;
        }
        cfg.session_paired = true;
        let token = cfg.session_token.clone();
        let org = cfg.org_name.clone();
        drop(cfg);
        let _ = request.respond(json_response(
            StatusCode(200),
            json!({
                "token": token,
                "orgName": org,
                "message": "Téléphone appairé. Vous pouvez envoyer des documents."
            }),
        ));
        return;
    }

    if path == "/v1/upload" && method == Method::Post {
        let token = match bearer_token(&request) {
            Some(t) => t,
            None => {
                let _ = request.respond(json_response(
                    StatusCode(401),
                    json!({ "error": "Jeton manquant (Authorization: Bearer …)." }),
                ));
                return;
            }
        };
        let expected = match config.lock() {
            Ok(c) => c.session_token.clone(),
            Err(e) => {
                let _ = request.respond(json_response(
                    StatusCode(500),
                    json!({ "error": e.to_string() }),
                ));
                return;
            }
        };
        if token != expected {
            let _ = request.respond(json_response(
                StatusCode(401),
                json!({ "error": "Jeton invalide. Réappairez le téléphone." }),
            ));
            return;
        }

        let doc_hint = header_value(&request, "X-Doc-Type").unwrap_or_else(|| "unknown".into());
        if !valid_doc_hint(&doc_hint) {
            let _ = request.respond(json_response(
                StatusCode(400),
                json!({ "error": "X-Doc-Type inconnu." }),
            ));
            return;
        }
        let file_name_raw =
            header_value(&request, "X-File-Name").unwrap_or_else(|| "document".into());
        let mime = header_value(&request, "Content-Type")
            .unwrap_or_else(|| "application/octet-stream".into());
        let mime_clean = mime.split(';').next().unwrap_or(&mime).trim().to_string();

        let body = match read_body(&mut request, MAX_UPLOAD_BYTES) {
            Ok(b) => b,
            Err(e) => {
                let _ = request.respond(json_response(StatusCode(413), json!({ "error": e })));
                return;
            }
        };
        if body.is_empty() {
            let _ = request.respond(json_response(
                StatusCode(400),
                json!({ "error": "Corps vide." }),
            ));
            return;
        }

        let ext = extension_for_mime(&mime_clean, &file_name_raw);
        let safe = sanitize_file_name(&file_name_raw);
        let file_name = if safe.contains('.') {
            safe
        } else {
            format!("{safe}.{ext}")
        };

        let tmp_dir = std::env::temp_dir().join("sascompta-companion");
        if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
            let _ = request.respond(json_response(
                StatusCode(500),
                json!({ "error": format!("Temp: {e}") }),
            ));
            return;
        }
        let stamp = Utc::now().format("%Y%m%d%H%M%S%.3f");
        let tmp_path = tmp_dir.join(format!("up_{stamp}_{file_name}"));
        if let Err(e) = std::fs::write(&tmp_path, &body) {
            let _ = request.respond(json_response(
                StatusCode(500),
                json!({ "error": format!("Écriture temp: {e}") }),
            ));
            return;
        }

        let hint = if doc_hint == "unknown" {
            None
        } else {
            Some(doc_hint.as_str())
        };
        let imported = db::with_conn(state, |c| {
            let path_guard = state.path.lock().map_err(|e| e.to_string())?;
            let workspace = path_guard
                .as_ref()
                .ok_or_else(|| "Aucun workspace ouvert".to_string())?
                .clone();
            drop(path_guard);
            db::documents::analyze_document_with_hint(c, &workspace, &tmp_path, hint)
        });
        let _ = std::fs::remove_file(&tmp_path);

        match imported {
            Ok(doc) => {
                if let Ok(mut cfg) = config.lock() {
                    cfg.received_count = cfg.received_count.saturating_add(1);
                    cfg.session_paired = true;
                }
                if let Some(app) = app {
                    let _ = app.emit(
                        "companion-document-received",
                        json!({
                            "id": doc.id,
                            "sourceFileName": doc.source_file_name,
                            "docType": doc.doc_type,
                            "status": doc.status,
                        }),
                    );
                }
                let _ = request.respond(json_response(
                    StatusCode(200),
                    json!({
                        "id": doc.id,
                        "docType": doc.doc_type,
                        "status": doc.status,
                        "sourceFileName": doc.source_file_name,
                        "confidence": doc.confidence,
                        "message": "Document reçu — en revue sur le PC avant écriture."
                    }),
                ));
            }
            Err(e) => {
                let _ = request.respond(json_response(
                    StatusCode(500),
                    json!({ "error": e.to_string() }),
                ));
            }
        }
        return;
    }

    if path == "/" && method == Method::Get {
        let html = "<!doctype html><meta charset=utf-8><title>SasCompta compagnon</title>\
            <body style='font-family:system-ui;padding:2rem'>\
            <h1>SasCompta — pont local</h1>\
            <p>Ce service reçoit photos et PDF depuis l’app compagnon sur le Wi‑Fi local. Aucun cloud.</p>\
            </body>";
        let mut resp = Response::from_string(html).with_status_code(StatusCode(200));
        for h in cors_headers() {
            resp.add_header(h);
        }
        if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
            resp.add_header(h);
        }
        let _ = request.respond(resp);
        return;
    }

    let _ = request.respond(json_response(
        StatusCode(404),
        json!({ "error": "Route inconnue." }),
    ));
}

fn spawn_server(
    app: Option<AppHandle>,
    state: AppState,
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    config: Arc<Mutex<LiveConfig>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let server = match Server::from_listener(listener, None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("companion: échec Server::from_listener: {e}");
                return;
            }
        };
        while !stop.load(Ordering::SeqCst) {
            match server.recv_timeout(StdDuration::from_millis(400)) {
                Ok(Some(req)) => handle_request(app.as_ref(), &state, &config, req),
                Ok(None) => {}
                Err(e) if stop.load(Ordering::SeqCst) => {
                    let _ = e;
                    break;
                }
                Err(_) => {
                    // timeout / interrupt — boucle
                }
            }
        }
    })
}

#[tauri::command]
pub fn companion_status(hub: State<'_, CompanionHub>) -> CompanionStatus {
    hub.status()
}

#[tauri::command]
pub fn companion_stop(hub: State<'_, CompanionHub>) -> CompanionStatus {
    hub.stop()
}

#[tauri::command]
pub fn companion_rotate_pairing(hub: State<'_, CompanionHub>) -> Result<CompanionStatus, String> {
    hub.rotate_pairing()
}

#[tauri::command]
pub fn companion_start(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: State<'_, CompanionHub>,
) -> Result<CompanionStatus, String> {
    {
        let path_guard = state.path.lock().map_err(|e| e.to_string())?;
        if path_guard.is_none() {
            return Err("Ouvrez un workspace avant d’activer le compagnon mobile.".into());
        }
    }

    // Arrête une instance précédente.
    hub.stop();

    let org_name = db::with_conn(&state, |c| {
        let mut stmt = c.prepare("SELECT name FROM organization WHERE id = 1")?;
        let name: String = stmt.query_row([], |r| r.get(0))?;
        Ok(name)
    })
    .unwrap_or_else(|_| "Société".into());

    let host = guess_lan_ip();
    let (listener, port) = bind_listener(DEFAULT_PORT)?;
    let stop = Arc::new(AtomicBool::new(false));
    let config = Arc::new(Mutex::new(LiveConfig {
        host,
        port,
        org_name,
        pairing_code: random_pairing_code(),
        pairing_expires_at: Utc::now() + Duration::seconds(PAIRING_TTL_SECS),
        session_token: random_token(),
        session_paired: false,
        received_count: 0,
    }));

    let app_state = AppState {
        conn: state.conn.clone(),
        path: state.path.clone(),
    };
    let join = spawn_server(
        Some(app.clone()),
        app_state,
        listener,
        stop.clone(),
        config.clone(),
    );

    let mut guard = hub.inner.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(Running {
        stop,
        join: Some(join),
        config,
    });
    Ok(CompanionHub::status_locked(&guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_code_is_six_digits() {
        let c = random_pairing_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn sanitize_keeps_extension() {
        assert_eq!(sanitize_file_name("facture 12.pdf"), "facture_12.pdf");
        assert_eq!(sanitize_file_name("../../../x"), "x");
    }

    #[test]
    fn qr_svg_renders() {
        let svg = render_qr_svg(r#"{"v":1,"host":"192.168.1.10","port":17832}"#).unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn http_pair_and_upload_lands_in_import_queue() {
        use crate::db;
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let state = AppState::new();
        *state.conn.lock().unwrap() = Some(conn);
        *state.path.lock().unwrap() = Some(PathBuf::from(&info.path));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let config = Arc::new(Mutex::new(LiveConfig {
            host: "127.0.0.1".into(),
            port,
            org_name: "Demo SAS".into(),
            pairing_code: "123456".into(),
            pairing_expires_at: Utc::now() + Duration::seconds(300),
            session_token: "tokentest0123456789abcdef01234567".into(),
            session_paired: false,
            received_count: 0,
        }));
        let state2 = AppState {
            conn: state.conn.clone(),
            path: state.path.clone(),
        };
        let join = spawn_server(None, state2, listener, stop.clone(), config.clone());

        // Pair
        let pair_body = br#"{"code":"123456"}"#;
        let pair_req = format!(
            "POST /v1/pair HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            pair_body.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(pair_req.as_bytes()).unwrap();
        stream.write_all(pair_body).unwrap();
        let mut pair_resp = String::new();
        stream.read_to_string(&mut pair_resp).unwrap();
        assert!(pair_resp.contains("200"), "{pair_resp}");
        assert!(pair_resp.contains("tokentest"));

        // Upload tiny PDF-ish bytes
        let file = b"%PDF-1.4 smoke ticket 12,50";
        let up_req = format!(
            "POST /v1/upload HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer tokentest0123456789abcdef01234567\r\nContent-Type: application/pdf\r\nX-Doc-Type: ticket\r\nX-File-Name: ticket-smoke.pdf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            file.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(up_req.as_bytes()).unwrap();
        stream.write_all(file).unwrap();
        let mut up_resp = String::new();
        stream.read_to_string(&mut up_resp).unwrap();
        assert!(up_resp.contains("200"), "{up_resp}");
        assert!(up_resp.contains("pending_review") || up_resp.contains("ticket"));

        stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", port));
        let _ = join.join();

        let cfg = config.lock().unwrap();
        assert_eq!(cfg.received_count, 1);
        assert!(cfg.session_paired);
    }
}
