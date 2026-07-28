// src/main.rs
extern crate rocket;

mod models;
mod sensors;
mod email;
mod db;

use rocket::{get, post, delete, put, routes, State, catch, catchers};
use rocket::serde::json::Json;
use rocket::fs::{NamedFile, FileServer, relative};
use models::{Device as ModelDevice, SensorType, User, UserRole, UserPermissions, SiteSettings, StatusPage, MaintenanceWindow, AuditLog};
use log::{info, error, debug};
use sensors::*;
use db::Database;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use serde_json::{self, json};
use std::path::Path;
use tokio::time::Duration;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use chrono::{NaiveDate, Local, DateTime};
use rocket::response::content::RawText;
use rocket::http::{Status, CookieJar, Cookie};
use rocket::request::{self, Request, FromRequest};
use rocket::outcome::Outcome;
use serde::Deserialize;
use rocket::serde::Serialize;
use email::EmailService;
use std::sync::atomic::{AtomicPtr, Ordering};

static AUTH_CONFIG: AtomicPtr<serde_json::Value> = AtomicPtr::new(std::ptr::null_mut());

fn init_auth_config() {
    if let Ok(content) = fs::read_to_string("static/config.js") {
        if let Some(config_str) = content.strip_prefix("const AUTH_CONFIG = ") {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(config_str) {
                let config_ptr = Box::into_raw(Box::new(config));
                let old_ptr = AUTH_CONFIG.swap(config_ptr, Ordering::SeqCst);
                if !old_ptr.is_null() {
                    unsafe {
                        drop(Box::from_raw(old_ptr));
                    }
                }
            }
        }
    }
}

// Struct to track device status runtime state
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct DeviceStatus {
    ping_status: Option<String>,
    http_status: Option<String>,
    bandwidth_usage: Option<f64>,
    ssl_status: Option<String>,
    dns_status: Option<String>,
    db_status: Option<String>,
    last_update: DateTime<Local>,
    changed_at: DateTime<Local>,
}

impl DeviceStatus {
    fn new() -> Self {
        let now = Local::now();
        Self {
            ping_status: None,
            http_status: None,
            bandwidth_usage: None,
            ssl_status: None,
            dns_status: None,
            db_status: None,
            last_update: now,
            changed_at: now,
        }
    }

    fn update_ping(&mut self, new_status: String) -> bool {
        let changed = self.ping_status.as_ref() != Some(&new_status);
        if changed {
            self.ping_status = Some(new_status);
            self.changed_at = Local::now();
        }
        self.last_update = Local::now();
        changed
    }
}

type SharedDevices = Arc<Mutex<Vec<ModelDevice>>>;
static LOG_FILE: &str = "rustPing_running.log";

#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("static/app/index.html")).await.ok()
}

#[get("/login")]
async fn login_page() -> Option<NamedFile> {
    NamedFile::open(Path::new("static/app/index.html")).await.ok()
}

// API to get the list of devices from persistent database
#[get("/devices")]
async fn get_devices(_auth: Auth, db: &State<Database>) -> Json<Vec<ModelDevice>> {
    match db.get_devices() {
        Ok(devices) => Json(devices),
        Err(e) => {
            error!("Failed to fetch devices from database: {}", e);
            Json(vec![])
        }
    }
}

/// Export logs filtered by date range and device names
#[get("/export_log?<devices>&<start_date>&<end_date>&<format>")]
async fn export_log(
    devices: Option<&str>,
    start_date: Option<&str>,
    end_date: Option<&str>,
    format: Option<&str>
) -> RawText<String> {
    let mut file_content = String::new();
    if let Ok(mut file) = OpenOptions::new().read(true).open(LOG_FILE) {
        if let Err(e) = file.read_to_string(&mut file_content) {
            error!("Failed to read log file: {}", e);
            return RawText("Failed to read log file".to_string());
        }
    } else {
        return RawText("Log file not found".to_string());
    }
    
    let lines: Vec<&str> = file_content.lines().collect();
    let mut filtered_lines = Vec::new();

    let start_date_parsed: Option<NaiveDate> = start_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let end_date_parsed: Option<NaiveDate> = end_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let device_filters: Option<Vec<String>> = devices.map(|d| d.split(',').map(|s| s.trim().to_lowercase()).collect());
    
    for line in lines {
        if line.starts_with("//") {
            filtered_lines.push(line);
            continue;
        }
        if let Some((timestamp, rest)) = line.split_once(" - ") {
            let parts: Vec<&str> = rest.split(',').collect();
            if parts.len() >= 2 {
                let device_part = parts[0].trim();
                let ip_part = parts[1].trim();
                let device_name = device_part.trim_start_matches("Device:").trim().to_lowercase();
                let device_ip = ip_part.to_lowercase();
                
                let mut include = true;
                let date_str = &timestamp[..10];
                if let Ok(entry_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    if let Some(start) = start_date_parsed {
                        if entry_date < start { include = false; }
                    }
                    if let Some(end) = end_date_parsed {
                        if entry_date > end { include = false; }
                    }
                }
                
                if let Some(ref filters) = device_filters {
                    let mut found = false;
                    for f in filters {
                        if device_name.contains(f) || device_ip.contains(f) {
                            found = true;
                            break;
                        }
                    }
                    if !found { include = false; }
                }

                if include {
                    filtered_lines.push(line);
                }
            }
        }
    }
    
    let output = if let Some(fmt) = format {
        if fmt.to_lowercase() == "csv" {
            let mut csv_lines = vec!["Timestamp,Device Name,IP Address,Ping,HTTP,Bandwidth".to_string()];
            for line in filtered_lines {
                if line.starts_with("//") { continue; }
                if let Some((timestamp, rest)) = line.split_once(" - ") {
                    let parts: Vec<&str> = rest.split(',').collect();
                    if parts.len() >= 2 {
                        let device_name = parts[0].trim().trim_start_matches("Device:").trim();
                        let device_ip = parts[1].trim();
                        let ping = parts.get(2).map(|s| s.replace("Ping:", "").trim().to_string()).unwrap_or_else(|| "N/A".to_string());
                        let http = parts.get(3).map(|s| s.replace("HTTP:", "").trim().to_string()).unwrap_or_else(|| "N/A".to_string());
                        let bandwidth = parts.get(4).map(|s| s.replace("Bandwidth:", "").trim().to_string()).unwrap_or_else(|| "N/A".to_string());
                        let csv_line = format!("{},{},{},{},{},{}", timestamp, device_name, device_ip, ping, http, bandwidth);
                        csv_lines.push(csv_line);
                    }
                }
            }
            csv_lines.join("\n")
        } else {
            filtered_lines.join("\n")
        }
    } else {
        filtered_lines.join("\n")
    };

    RawText(output)
}

#[get("/logs_json")]
async fn logs_json(db: &State<Database>) -> Json<serde_json::Value> {
    if let Ok(db_logs) = db.get_sensor_logs(500) {
        if !db_logs.is_empty() {
            return Json(json!(db_logs));
        }
    }

    let mut file_content = String::new();
    if let Ok(mut file) = OpenOptions::new().read(true).open(LOG_FILE) {
        if let Err(e) = file.read_to_string(&mut file_content) {
            error!("Failed to read log file: {}", e);
            return Json(json!({"error": "Failed to read log file"}));
        }
    } else {
        return Json(json!([]));
    }
    let lines: Vec<&str> = file_content.lines().collect();
    let mut entries = Vec::new();
    for line in lines {
        if line.starts_with("//") || line.trim().is_empty() {
            continue;
        }
        if let Some((timestamp, rest)) = line.split_once(" - ") {
            let parts: Vec<&str> = rest.splitn(2, ": ").collect();
            if parts.len() < 2 { continue; }
            let device = parts[0].trim();
            let statuses = parts[1];
            let mut ping = "";
            let mut http = "";
            let mut bandwidth = "";
            for status in statuses.split(", ") {
                if status.starts_with("Ping:") {
                    ping = status.trim_start_matches("Ping:").trim();
                } else if status.starts_with("HTTP:") {
                    http = status.trim_start_matches("HTTP:").trim();
                } else if status.starts_with("Bandwidth:") {
                    bandwidth = status.trim_start_matches("Bandwidth:").trim();
                }
            }
            let down = ping.to_lowercase() == "fail";
            let ts_parts: Vec<&str> = timestamp.split(' ').collect();
            let date = ts_parts.get(0).unwrap_or(&"").to_string();
            let time = ts_parts.get(1).unwrap_or(&"").to_string();
            entries.push(json!({
                "timestamp": timestamp,
                "date": date,
                "time": time,
                "device": device,
                "ping": ping,
                "http": http,
                "bandwidth": bandwidth,
                "down": down
            }));
        }
    }
    Json(json!(entries))
}

#[delete("/logs")]
async fn delete_logs(_auth: Auth, db: &State<Database>) -> Result<String, Status> {
    let _ = db.clear_sensor_logs();
    if let Err(e) = OpenOptions::new().write(true).truncate(true).open(LOG_FILE) {
        error!("Failed to clear log file: {}", e);
        return Err(Status::InternalServerError);
    }
    let _ = db.log_audit("admin@rustping.local", "DELETE_LOGS", "System logs cleared by administrator");
    Ok("Logs cleared".to_string())
}

struct Auth;

#[allow(dead_code)]
#[derive(Debug)]
enum AuthError {
    Missing,
    Invalid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth {
    type Error = AuthError;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let cookies = request.cookies();
        match cookies.get("auth") {
            Some(cookie) if cookie.value() == "true" => Outcome::Success(Auth),
            _ => Outcome::Forward(Status::Unauthorized)
        }
    }
}

#[catch(401)]
fn unauthorized() -> Status {
    Status::Unauthorized
}

#[derive(Serialize, Deserialize, Clone)]
struct WebDevice {
    name: String,
    ip: String,
    category: String,
    sensors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snmp_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_device: Option<String>,
}

impl From<WebDevice> for ModelDevice {
    fn from(web_device: WebDevice) -> Self {
        ModelDevice {
            id: None,
            name: web_device.name,
            ip: web_device.ip,
            category: web_device.category,
            sensors: web_device.sensors.iter()
                .map(|s| match s.as_str() {
                    "Ping" => SensorType::Ping,
                    "Http" => SensorType::Http,
                    "Https" => SensorType::Https,
                    "Bandwidth" => SensorType::Bandwidth,
                    "Port" => SensorType::Port,
                    "Snmp" => SensorType::Snmp,
                    "SslCert" => SensorType::SslCert,
                    "Dns" => SensorType::Dns,
                    "Database" => SensorType::Database,
                    _ => SensorType::Ping
                })
                .collect(),
            http_path: web_device.http_path,
            port: web_device.port,
            snmp_community: web_device.snmp_community,
            parent_device: web_device.parent_device,
            ping_status: None,
            http_status: None,
            bandwidth_usage: None,
            ssl_status: None,
            dns_status: None,
            db_status: None,
        }
    }
}

#[post("/devices", data = "<device>")]
async fn add_web_device(device: &str, db: &State<Database>, devices: &State<SharedDevices>) -> Status {
    let new_web_device: WebDevice = match serde_json::from_str(device) {
        Ok(dev) => dev,
        Err(e) => {
            error!("Failed to parse device JSON: {}", e);
            return Status::BadRequest;
        }
    };

    let model_dev = ModelDevice::from(new_web_device);
    match db.save_device(&model_dev) {
        Ok(id) => {
            let mut dev_with_id = model_dev.clone();
            dev_with_id.id = Some(id);
            let mut devices_locked = devices.lock().await;
            devices_locked.push(dev_with_id);
            let _ = db.log_audit("admin@rustping.local", "CREATE_DEVICE", &format!("Created device {}", model_dev.name));
            Status::Ok
        }
        Err(e) => {
            error!("Failed to save device in DB: {}", e);
            Status::InternalServerError
        }
    }
}

#[delete("/devices/<index>")]
async fn delete_web_device(index: usize, db: &State<Database>, devices: &State<SharedDevices>) -> Status {
    let mut devices_locked = devices.lock().await;
    if index >= devices_locked.len() {
        return Status::NotFound;
    }
    let dev = devices_locked.remove(index);
    let _ = db.delete_device(&dev.name);
    let _ = db.log_audit("admin@rustping.local", "DELETE_DEVICE", &format!("Deleted device {}", dev.name));
    Status::Ok
}

#[put("/devices/<id>", data = "<device>")]
async fn update_device(id: usize, device: &str, db: &State<Database>, devices: &State<SharedDevices>) -> Status {
    let updated_web_device: WebDevice = match serde_json::from_str(device) {
        Ok(dev) => dev,
        Err(e) => {
            error!("Failed to parse device JSON: {}", e);
            return Status::BadRequest;
        }
    };

    let mut devices_locked = devices.lock().await;
    if id >= devices_locked.len() {
        return Status::NotFound;
    }

    let mut model_dev = ModelDevice::from(updated_web_device);
    model_dev.id = devices_locked[id].id.clone();
    
    if let Err(e) = db.save_device(&model_dev) {
        error!("Failed to update device in database: {}", e);
        return Status::InternalServerError;
    }

    devices_locked[id] = model_dev.clone();
    let _ = db.log_audit("admin@rustping.local", "UPDATE_DEVICE", &format!("Updated device {}", model_dev.name));
    Status::Ok
}

// Authentication & User RBAC Management Endpoints
#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password_hash: String,
}

#[post("/api/login", data = "<login_req>")]
async fn login_user(jar: &CookieJar<'_>, login_req: Json<LoginReq>, db: &State<Database>) -> Result<Json<User>, Status> {
    match db.authenticate_user(&login_req.username, &login_req.password_hash) {
        Ok(Some(user)) => {
            jar.add(Cookie::new("auth", "true"));
            let _ = db.log_audit(&user.email, "LOGIN_SUCCESS", &format!("User {} logged in", user.username));
            Ok(Json(user))
        }
        _ => Err(Status::Unauthorized),
    }
}

#[get("/favicon.ico")]
async fn favicon_ico() -> Option<NamedFile> {
    NamedFile::open(relative!("static/app/favicon.png")).await.ok()
}

#[get("/favicon.png")]
async fn favicon_png() -> Option<NamedFile> {
    NamedFile::open(relative!("static/app/favicon.png")).await.ok()
}

#[derive(Deserialize)]
struct ChangePasswordReq {
    username: String,
    old_password_hash: String,
    new_password_hash: String,
}

#[post("/api/user/change-password", data = "<req>")]
async fn change_password(_auth: Auth, req: Json<ChangePasswordReq>, db: &State<Database>) -> Status {
    info!("Password change request for user: {}", req.username);
    match db.authenticate_user(&req.username, &req.old_password_hash) {
        Ok(Some(_user)) => {
            if let Ok(true) = db.update_user_password(&req.username, &req.new_password_hash) {
                let _ = db.log_audit(&req.username, "CHANGE_PASSWORD", &format!("Password updated for user {}", req.username));
                info!("Password successfully updated for user {}", req.username);
                Status::Ok
            } else {
                error!("Failed to update user password in database for {}", req.username);
                Status::InternalServerError
            }
        }
        Ok(None) => {
            error!("Authentication failed for password change (old password mismatch) for user {}", req.username);
            Status::Unauthorized
        }
        Err(e) => {
            error!("Database error during password change: {}", e);
            Status::InternalServerError
        }
    }
}

#[get("/api/users")]
async fn get_users(_auth: Auth, db: &State<Database>) -> Json<Vec<User>> {
    match db.get_users() {
        Ok(users) => Json(users),
        Err(_) => Json(vec![]),
    }
}

#[derive(Deserialize)]
struct CreateUserReq {
    username: String,
    email: String,
    password_hash: String,
    role: String,
    permissions: UserPermissions,
}

#[post("/api/users", data = "<user_req>")]
async fn create_user(_auth: Auth, user_req: Json<CreateUserReq>, db: &State<Database>) -> Result<Json<User>, Status> {
    let role = UserRole::from_str(&user_req.role);
    match db.add_user(&user_req.username, &user_req.email, &user_req.password_hash, role, &user_req.permissions) {
        Ok(user) => {
            let _ = db.log_audit("admin@rustping.local", "CREATE_USER", &format!("Created operator {} with role {}", user.username, user.role.as_str()));
            Ok(Json(user))
        }
        Err(e) => {
            error!("Failed to create user: {}", e);
            Err(Status::InternalServerError)
        }
    }
}

#[delete("/api/users/<username>")]
async fn delete_user(_auth: Auth, username: &str, db: &State<Database>) -> Status {
    match db.delete_user(username) {
        Ok(_) => {
            let _ = db.log_audit("admin@rustping.local", "DELETE_USER", &format!("Deleted user {}", username));
            Status::Ok
        }
        Err(_) => Status::InternalServerError,
    }
}

#[get("/api/settings")]
async fn get_settings(db: &State<Database>) -> Json<SiteSettings> {
    match db.get_site_settings() {
        Ok(s) => Json(s),
        Err(_) => Json(SiteSettings::default()),
    }
}

#[post("/api/settings", data = "<settings>")]
async fn save_settings(_auth: Auth, settings: Json<SiteSettings>, db: &State<Database>) -> Status {
    match db.save_site_settings(&settings) {
        Ok(_) => {
            let _ = db.log_audit("admin@rustping.local", "UPDATE_SETTINGS", "Site configuration updated");
            Status::Ok
        }
        Err(_) => Status::InternalServerError,
    }
}

#[get("/api/audit-logs")]
async fn get_audit_logs(_auth: Auth, db: &State<Database>) -> Json<Vec<AuditLog>> {
    match db.get_audit_logs() {
        Ok(logs) => Json(logs),
        Err(_) => Json(vec![]),
    }
}

// Public & Private Status Page Endpoints (Feature 4)
#[get("/api/status-pages")]
async fn get_status_pages(db: &State<Database>) -> Json<Vec<StatusPage>> {
    match db.get_status_pages() {
        Ok(pages) => Json(pages),
        Err(_) => Json(vec![]),
    }
}

#[derive(Deserialize)]
struct CreateStatusPageReq {
    title: String,
    slug: String,
    description: String,
    is_public: bool,
}

#[post("/api/status-pages", data = "<sp_req>")]
async fn create_status_page(_auth: Auth, sp_req: Json<CreateStatusPageReq>, db: &State<Database>) -> Result<Json<StatusPage>, Status> {
    match db.create_status_page(&sp_req.title, &sp_req.slug, &sp_req.description, sp_req.is_public) {
        Ok(page) => {
            let _ = db.log_audit("admin@rustping.local", "CREATE_STATUS_PAGE", &format!("Created status page {}", page.title));
            Ok(Json(page))
        }
        Err(e) => {
            error!("Failed to create status page: {}", e);
            Err(Status::InternalServerError)
        }
    }
}

// Public Status View Route
#[get("/status/<slug>")]
async fn render_public_status(slug: &str) -> Option<NamedFile> {
    info!("Rendering public status page for slug: {}", slug);
    NamedFile::open(Path::new("static/app/index.html")).await.ok()
}

// Maintenance Window Management Endpoints
#[get("/api/maintenance")]
async fn get_maintenance_windows(db: &State<Database>) -> Json<Vec<MaintenanceWindow>> {
    match db.get_maintenance_windows() {
        Ok(windows) => Json(windows),
        Err(_) => Json(vec![]),
    }
}

#[derive(Deserialize)]
struct CreateMaintenanceReq {
    title: String,
    start_time: String,
    end_time: String,
}

#[post("/api/maintenance", data = "<m_req>")]
async fn create_maintenance_window(_auth: Auth, m_req: Json<CreateMaintenanceReq>, db: &State<Database>) -> Result<Json<MaintenanceWindow>, Status> {
    match db.add_maintenance_window(&m_req.title, &m_req.start_time, &m_req.end_time) {
        Ok(win) => {
            let _ = db.log_audit("admin@rustping.local", "CREATE_MAINTENANCE", &format!("Scheduled maintenance: {}", win.title));
            Ok(Json(win))
        }
        Err(e) => {
            error!("Failed to create maintenance window: {}", e);
            Err(Status::InternalServerError)
        }
    }
}

// Email configuration REST APIs
#[get("/api/email/config")]
async fn get_email_config(_auth: Auth, email_service: &State<EmailService>) -> Json<email::EmailConfig> {
    let config = email_service.get_config().await;
    Json(config)
}

#[post("/api/email/config", data = "<config>")]
async fn update_email_config(_auth: Auth, config: Json<email::EmailConfig>, email_service: &State<EmailService>) -> Status {
    match email_service.update_config(config.into_inner()).await {
        Ok(_) => Status::Ok,
        Err(e) => {
            error!("Failed to update email config: {}", e);
            Status::InternalServerError
        }
    }
}

#[post("/api/email/config/test")]
async fn send_test_email(_auth: Auth, email_service: &State<EmailService>) -> Status {
    let test_addr = email_service.get_config().await.recipients.first().cloned().unwrap_or_else(|| "admin@rustping.local".to_string());
    match email_service.send_test_email(&test_addr).await {
        Ok(_) => Status::Ok,
        Err(e) => {
            error!("Failed to send test email: {}", e);
            Status::InternalServerError
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting RustPing Enterprise Infrastructure Monitor...");
    init_auth_config();

    let db = Database::new("rustping_enterprise.db").expect("Failed to initialize database engine");
    let initial_devices = db.get_devices().unwrap_or_default();
    let devices: SharedDevices = Arc::new(Mutex::new(initial_devices));
    let email_service = EmailService::new();

    let rocket_instance = rocket::build()
        .manage(devices.clone())
        .manage(db.clone())
        .manage(email_service.clone())
        .mount("/", routes![
            index,
            login_page,
            favicon_ico,
            favicon_png,
            get_devices,
            export_log,
            logs_json,
            delete_logs,
            add_web_device,
            delete_web_device,
            update_device,
            get_email_config,
            update_email_config,
            send_test_email,
            get_users,
            create_user,
            delete_user,
            login_user,
            change_password,
            get_settings,
            save_settings,
            get_audit_logs,
            get_status_pages,
            create_status_page,
            render_public_status,
            get_maintenance_windows,
            create_maintenance_window,
        ])
        .mount("/static", FileServer::from(relative!("static")))
        .register("/", catchers![unauthorized]);

    // Spawn Background Multi-Protocol Synthetic Monitoring Engine Loop
    let devices_clone = devices.clone();
    let email_service_clone = email_service.clone();
    let db_clone = db.clone();

    tokio::spawn(async move {
        let mut device_statuses: HashMap<String, DeviceStatus> = HashMap::new();
        let mut monitor_interval = tokio::time::interval(Duration::from_secs(5));
        monitor_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        
        loop {
            monitor_interval.tick().await;

            let devices_to_monitor: Vec<ModelDevice> = {
                let locked = devices_clone.lock().await;
                locked.clone()
            };

            let mut status_changed = false;

            let parent_statuses: HashMap<String, String> = {
                let locked = devices_clone.lock().await;
                locked.iter().map(|d| (d.name.clone(), d.ping_status.clone().unwrap_or("Checking".to_string()))).collect()
            };

            let check_results = futures::future::join_all(
                devices_to_monitor.into_iter().map(|dev| {
                    let parent_statuses = parent_statuses.clone();
                    async move {
                        if let Some(parent) = &dev.parent_device {
                            if let Some(status) = parent_statuses.get(parent) {
                                if status == "Down" || status == "Unreachable" {
                                    return (dev, "Unreachable".to_string(), Some("Unreachable".to_string()), None, None, None, None);
                                }
                            }
                        }

                        let mut is_up = true;
                        if dev.sensors.contains(&SensorType::Ping) {
                            is_up = is_up && monitor_ping(&dev.ip).await;
                        }
                        if dev.sensors.contains(&SensorType::Port) {
                            if let Some(port) = dev.port {
                                is_up = is_up && monitor_tcp_port(&dev.ip, port).await;
                            }
                        }

                        let ping_result_str = if is_up { "Up".to_string() } else { "Down".to_string() };

                        let has_http_sensor = dev.sensors.contains(&SensorType::Http) || dev.sensors.contains(&SensorType::Https);
                        let (http_status, mut bandwidth_usage) = if is_up && has_http_sensor {
                            if let Some(ref url) = dev.http_path {
                                if monitor_http(url).await {
                                    (Some("Up".to_string()), Some(rand::thread_rng().gen_range(10.0..1000.0)))
                                } else {
                                    (Some("Down".to_string()), None)
                                }
                            } else {
                                (Some("Down".to_string()), None)
                            }
                        } else if has_http_sensor {
                            (Some("Down".to_string()), None)
                        } else {
                            (None, None)
                        };

                        if is_up && dev.sensors.contains(&SensorType::Snmp) {
                            if let Some(community) = &dev.snmp_community {
                                if let Some(bw) = sensors::monitor_snmp_bandwidth(&dev.ip, community).await {
                                    bandwidth_usage = Some(bw);
                                }
                            }
                        }

                        // Synthetic SSL Expiration Probe (Feature 5)
                        let ssl_status = if dev.sensors.contains(&SensorType::SslCert) {
                            Some(monitor_ssl_cert(&dev.ip).await)
                        } else {
                            None
                        };

                        // Synthetic DNS Resolution Probe (Feature 5)
                        let dns_status = if dev.sensors.contains(&SensorType::Dns) {
                            Some(monitor_dns_resolution(&dev.ip).await)
                        } else {
                            None
                        };

                        // Database Response Probe (Feature 5)
                        let db_status = if dev.sensors.contains(&SensorType::Database) {
                            let port = dev.port.unwrap_or(5432);
                            Some(monitor_database_port(&dev.ip, port).await)
                        } else {
                            None
                        };

                        // 1. SSH Server Probe
                        if dev.sensors.contains(&SensorType::Ssh) {
                            let p = dev.port.unwrap_or(22);
                            let res = monitor_ssh(&dev.ip, p).await;
                            debug!("SSH check for {}: {}", dev.name, res);
                        }

                        // 2. SMTP Mail Probe
                        if dev.sensors.contains(&SensorType::Smtp) {
                            let p = dev.port.unwrap_or(25);
                            let res = monitor_smtp(&dev.ip, p).await;
                            debug!("SMTP check for {}: {}", dev.name, res);
                        }

                        // 3. NTP Time Probe
                        if dev.sensors.contains(&SensorType::Ntp) {
                            let res = monitor_ntp(&dev.ip).await;
                            debug!("NTP check for {}: {}", dev.name, res);
                        }

                        // 4. FTP Probe
                        if dev.sensors.contains(&SensorType::Ftp) {
                            let p = dev.port.unwrap_or(21);
                            let res = monitor_ftp(&dev.ip, p).await;
                            debug!("FTP check for {}: {}", dev.name, res);
                        }

                        // 5. Jitter Probe
                        if dev.sensors.contains(&SensorType::Jitter) {
                            let res = monitor_jitter(&dev.ip).await;
                            debug!("Jitter check for {}: {}", dev.name, res);
                        }

                        // 6. HTTP Latency TTFB Probe
                        if dev.sensors.contains(&SensorType::HttpLatency) {
                            let target_url = dev.http_path.as_deref().unwrap_or(&dev.ip);
                            let res = monitor_http_latency(target_url).await;
                            debug!("HTTP TTFB check for {}: {}", dev.name, res);
                        }

                        // 7. Packet Loss Probe
                        if dev.sensors.contains(&SensorType::PacketLoss) {
                            let res = monitor_packet_loss(&dev.ip).await;
                            debug!("Packet loss check for {}: {}", dev.name, res);
                        }

                        // 8. CPU & Memory Load Probe
                        if dev.sensors.contains(&SensorType::CpuLoad) {
                            let res = monitor_cpu_load(&dev.ip).await;
                            debug!("CPU Load check for {}: {}", dev.name, res);
                        }

                        // 9. Disk Space Storage Probe
                        if dev.sensors.contains(&SensorType::DiskSpace) {
                            let res = monitor_disk_space(&dev.ip).await;
                            debug!("Disk space check for {}: {}", dev.name, res);
                        }

                        // 10. WebSocket Endpoint Probe
                        if dev.sensors.contains(&SensorType::WebSocket) {
                            let target_url = dev.http_path.as_deref().unwrap_or(&dev.ip);
                            let res = monitor_websocket(target_url).await;
                            debug!("WebSocket check for {}: {}", dev.name, res);
                        }

                        (dev, ping_result_str, http_status, bandwidth_usage, ssl_status, dns_status, db_status)
                    }
                })
            ).await;

            let mut devices_locked = devices_clone.lock().await;
            for (dev, ping_result, http_status, bandwidth_usage, ssl_status, dns_status, db_status) in check_results {
                let status = device_statuses.entry(dev.ip.clone()).or_insert_with(DeviceStatus::new);
                
                if status.update_ping(ping_result.clone()) {
                    status_changed = true;
                }

                if let Some(device) = devices_locked.iter_mut().find(|d| d.ip == dev.ip) {
                    if device.ping_status != Some(ping_result.clone()) || device.http_status != http_status {
                        status_changed = true;
                    }
                    device.ping_status = Some(ping_result.clone());
                    device.http_status = http_status.clone();
                    device.bandwidth_usage = bandwidth_usage;
                    device.ssl_status = ssl_status.clone();
                    device.dns_status = dns_status.clone();
                    device.db_status = db_status.clone();

                    let _ = db_clone.update_device_statuses(
                        &device.name,
                        Some(&ping_result),
                        http_status.as_deref(),
                        bandwidth_usage,
                        ssl_status.as_deref(),
                        dns_status.as_deref(),
                        db_status.as_deref(),
                    );
                }
            }
            drop(devices_locked);

            let now = Local::now();
            let ts_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

            if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(LOG_FILE) {
                let devices_locked = devices_clone.lock().await;
                
                for dev in devices_locked.iter() {
                    let status = device_statuses.get(&dev.ip).cloned().unwrap_or_else(DeviceStatus::new);
                    let http_status = if dev.sensors.contains(&SensorType::Http) || dev.sensors.contains(&SensorType::Https) {
                        dev.http_status.as_deref().unwrap_or("FAIL")
                    } else {
                        "N/A"
                    };
                    
                    let bandwidth = if (dev.sensors.contains(&SensorType::Http) || dev.sensors.contains(&SensorType::Https) || dev.sensors.contains(&SensorType::Snmp)) && dev.ping_status.as_deref() == Some("Up") {
                        dev.bandwidth_usage.map_or("N/A".to_string(), |b| format!("{:.2} Mbps", b))
                    } else {
                        "N/A".to_string()
                    };
                    
                    let ping_status_str = status.ping_status.as_deref().unwrap_or("N/A");
                    
                    let log_entry = format!(
                        "{} - {} ({}): Ping: {}, HTTP: {}, Bandwidth: {}\n",
                        ts_str,
                        dev.name,
                        dev.ip,
                        ping_status_str,
                        http_status,
                        bandwidth
                    );
                    
                    if let Err(e) = file.write_all(log_entry.as_bytes()) {
                        error!("Failed to write log entry: {}", e);
                    }

                    let _ = db_clone.add_sensor_log(&dev.name, &dev.ip, ping_status_str, http_status, &bandwidth, &ts_str);
                    
                    if status_changed && (ping_status_str == "FAIL" || http_status == "FAIL") {
                        let log_data = email::LogData {
                            date: now.format("%Y-%m-%d").to_string(),
                            time: now.format("%H:%M:%S").to_string(),
                            ping_status: ping_status_str.to_string(),
                            http_status: http_status.to_string(),
                            bandwidth: bandwidth.clone(),
                        };
                        
                        let device_name = dev.name.clone();
                        let email_service_clone = email_service_clone.clone();
                        tokio::spawn(async move {
                            if let Err(e) = email_service_clone.send_email(&device_name, &log_data).await {
                                error!("Failed to send email notification: {}", e);
                            }
                        });
                    }
                }
            }
        }
    });

    if let Err(e) = rocket_instance.launch().await {
        error!("Failed to launch the web server: {}", e);
    }
}
