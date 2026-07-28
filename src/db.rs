// src/db.rs
use rusqlite::{params, Connection, Result};
use std::sync::{Arc, Mutex};
use log::info;
use crate::models::{Device, SensorType, User, UserRole, UserPermissions, SiteSettings, StatusPage, MaintenanceWindow, AuditLog};
use uuid::Uuid;
use chrono::Utc;
use std::fs;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Database {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        db.seed_defaults()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS organizations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                org_id TEXT NOT NULL,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                FOREIGN KEY (org_id) REFERENCES organizations(id)
            );

            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                org_id TEXT NOT NULL,
                username TEXT NOT NULL UNIQUE,
                email TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                permissions TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (org_id) REFERENCES organizations(id)
            );

            CREATE TABLE IF NOT EXISTS site_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                name TEXT NOT NULL,
                ip TEXT NOT NULL,
                category TEXT NOT NULL,
                sensors TEXT NOT NULL,
                http_path TEXT,
                port INTEGER,
                snmp_community TEXT,
                parent_device TEXT,
                ping_status TEXT,
                http_status TEXT,
                bandwidth_usage REAL,
                ssl_status TEXT,
                dns_status TEXT,
                db_status TEXT,
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
            );

            CREATE TABLE IF NOT EXISTS status_pages (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                is_public INTEGER NOT NULL DEFAULT 1,
                custom_domain TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
            );

            CREATE TABLE IF NOT EXISTS maintenance_windows (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                title TEXT NOT NULL,
                start_time TEXT NOT NULL,
                end_time TEXT NOT NULL,
                suppress_alerts INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
            );

            CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                org_id TEXT NOT NULL,
                user_email TEXT NOT NULL,
                action TEXT NOT NULL,
                details TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sensor_logs (
                id TEXT PRIMARY KEY,
                device_name TEXT NOT NULL,
                device_ip TEXT NOT NULL,
                ping_status TEXT NOT NULL,
                http_status TEXT NOT NULL,
                bandwidth TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            "
        )?;

        let _ = conn.execute("ALTER TABLE users ADD COLUMN username TEXT", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN password_hash TEXT", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN permissions TEXT", []);

        info!("Database schema initialized successfully.");
        Ok(())
    }

    fn seed_defaults(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Seed default Organization if empty
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))?;
        let org_id = if count == 0 {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO organizations (id, name, created_at) VALUES (?1, ?2, ?3)",
                params![id, "Enterprise Main Org", now],
            )?;
            id
        } else {
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))?
        };

        // Seed default Workspace if empty
        let ws_count: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0))?;
        let ws_id = if ws_count == 0 {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO workspaces (id, org_id, name, slug) VALUES (?1, ?2, ?3, ?4)",
                params![id, org_id, "Default Workspace", "default"],
            )?;
            id
        } else {
            conn.query_row("SELECT id FROM workspaces LIMIT 1", [], |r| r.get(0))?
        };

        // Seed Admin user if empty (default username: admin, default pass_hash for 'admin')
        let default_perms = serde_json::to_string(&UserPermissions::default()).unwrap_or_default();
        let default_hash = "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918"; // SHA256 of 'admin'

        let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap_or(0);
        if user_count == 0 {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO users (id, org_id, username, email, password_hash, role, permissions, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, org_id, "admin", "admin@rustping.local", default_hash, "Admin", default_perms, now],
            )?;
        } else {
            // Upgrade existing admin user record if password_hash or username is NULL
            let _ = conn.execute(
                "UPDATE users SET username='admin', password_hash=?1, permissions=?2 WHERE username IS NULL OR username='' OR email='admin@rustping.local'",
                params![default_hash, default_perms],
            );
        }

        // Seed Default Public Status Page if empty
        let sp_count: i64 = conn.query_row("SELECT COUNT(*) FROM status_pages", [], |r| r.get(0))?;
        if sp_count == 0 {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO status_pages (id, workspace_id, slug, title, description, is_public, custom_domain, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, NULL, ?6)",
                params![id, ws_id, "global-status", "RustPing Global System Status", "Live real-time operational status across all core services", now],
            )?;
        }

        // Seed Default Site Settings if empty
        let settings_count: i64 = conn.query_row("SELECT COUNT(*) FROM site_settings", [], |r| r.get(0))?;
        if settings_count == 0 {
            let defaults = SiteSettings::default();
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["graph_style", defaults.graph_style]);
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["density", defaults.density]);
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["refresh_rate", defaults.refresh_rate.to_string()]);
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["time_format", defaults.time_format]);
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["site_name", defaults.site_name]);
            let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES (?1, ?2)", params!["alert_emails_enabled", "true"]);
        }

        // Migrate devices from devices.json if database devices table is empty
        let dev_count: i64 = conn.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?;
        if dev_count == 0 {
            if let Ok(data) = fs::read_to_string("devices.json") {
                if let Ok(file_devices) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                    for dev in file_devices {
                        let id = Uuid::new_v4().to_string();
                        let name = dev.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown Device");
                        let ip = dev.get("ip").and_then(|v| v.as_str()).unwrap_or("127.0.0.1");
                        let category = dev.get("category").and_then(|v| v.as_str()).unwrap_or("Server");
                        let sensors_json = serde_json::to_string(dev.get("sensors").unwrap_or(&serde_json::json!(["Ping"]))).unwrap_or_default();
                        let http_path = dev.get("http_path").and_then(|v| v.as_str());
                        let port = dev.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
                        let snmp_community = dev.get("snmp_community").and_then(|v| v.as_str());
                        let parent_device = dev.get("parent_device").and_then(|v| v.as_str());

                        let _ = conn.execute(
                            "INSERT INTO devices (id, workspace_id, name, ip, category, sensors, http_path, port, snmp_community, parent_device) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                            params![id, ws_id, name, ip, category, sensors_json, http_path, port, snmp_community, parent_device],
                        );
                    }
                    info!("Successfully migrated devices from devices.json into SQLite database.");
                }
            }
        }

        Ok(())
    }

    pub fn get_devices(&self) -> Result<Vec<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, ip, category, sensors, http_path, port, snmp_community, parent_device, ping_status, http_status, bandwidth_usage, ssl_status, dns_status, db_status FROM devices")?;
        
        let device_rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let ip: String = row.get(2)?;
            let category: String = row.get(3)?;
            let sensors_raw: String = row.get(4)?;
            let http_path: Option<String> = row.get(5)?;
            let port: Option<u16> = row.get(6)?;
            let snmp_community: Option<String> = row.get(7)?;
            let parent_device: Option<String> = row.get(8)?;
            let ping_status: Option<String> = row.get(9)?;
            let http_status: Option<String> = row.get(10)?;
            let bandwidth_usage: Option<f64> = row.get(11)?;
            let ssl_status: Option<String> = row.get(12)?;
            let dns_status: Option<String> = row.get(13)?;
            let db_status: Option<String> = row.get(14)?;

            let sensors: Vec<SensorType> = serde_json::from_str(&sensors_raw).unwrap_or_else(|_| vec![SensorType::Ping]);

            Ok(Device {
                id: Some(id),
                name,
                ip,
                category,
                sensors,
                http_path,
                port,
                snmp_community,
                parent_device,
                ping_status,
                http_status,
                bandwidth_usage,
                ssl_status,
                dns_status,
                db_status,
            })
        })?;

        let mut devices = Vec::new();
        for dev in device_rows {
            devices.push(dev?);
        }
        Ok(devices)
    }

    pub fn save_device(&self, device: &Device) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = device.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        
        let ws_id: String = conn.query_row("SELECT id FROM workspaces LIMIT 1", [], |r| r.get(0))?;
        let sensors_json = serde_json::to_string(&device.sensors).unwrap_or_default();

        conn.execute(
            "INSERT INTO devices (id, workspace_id, name, ip, category, sensors, http_path, port, snmp_community, parent_device)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                ip=excluded.ip,
                category=excluded.category,
                sensors=excluded.sensors,
                http_path=excluded.http_path,
                port=excluded.port,
                snmp_community=excluded.snmp_community,
                parent_device=excluded.parent_device",
            params![id, ws_id, device.name, device.ip, device.category, sensors_json, device.http_path, device.port, device.snmp_community, device.parent_device],
        )?;

        Ok(id)
    }

    pub fn update_device_statuses(&self, name: &str, ping: Option<&str>, http: Option<&str>, bw: Option<f64>, ssl: Option<&str>, dns: Option<&str>, db: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET ping_status=?1, http_status=?2, bandwidth_usage=?3, ssl_status=?4, dns_status=?5, db_status=?6 WHERE name=?7",
            params![ping, http, bw, ssl, dns, db, name],
        )?;
        Ok(())
    }

    pub fn delete_device(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM devices WHERE name=?1", params![name])?;
        Ok(())
    }

    // User Administration & Password Change Methods
    pub fn get_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, org_id, username, email, password_hash, role, permissions, created_at FROM users")?;
        let rows = stmt.query_map([], |row| {
            let role_str: String = row.get(5)?;
            let perms_str: String = row.get(6)?;
            let permissions: UserPermissions = serde_json::from_str(&perms_str).unwrap_or_default();

            Ok(User {
                id: row.get(0)?,
                org_id: row.get(1)?,
                username: row.get(2)?,
                email: row.get(3)?,
                password_hash: row.get(4)?,
                role: UserRole::from_str(&role_str),
                permissions,
                created_at: row.get(7)?,
            })
        })?;

        let mut users = Vec::new();
        for user in rows {
            users.push(user?);
        }
        Ok(users)
    }

    pub fn add_user(&self, username: &str, email: &str, password_hash: &str, role: UserRole, permissions: &UserPermissions) -> Result<User> {
        let conn = self.conn.lock().unwrap();
        let org_id: String = conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let perms_json = serde_json::to_string(permissions).unwrap_or_default();

        conn.execute(
            "INSERT INTO users (id, org_id, username, email, password_hash, role, permissions, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, org_id, username, email, password_hash, role.as_str(), perms_json, now],
        )?;

        Ok(User {
            id,
            org_id,
            username: username.to_string(),
            email: email.to_string(),
            password_hash: password_hash.to_string(),
            role,
            permissions: permissions.clone(),
            created_at: now,
        })
    }

    pub fn delete_user(&self, username: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM users WHERE username=?1", params![username])?;
        Ok(())
    }

    pub fn update_user_password(&self, username: &str, new_password_hash: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE users SET password_hash=?1 WHERE username=?2",
            params![new_password_hash, username],
        )?;
        Ok(updated > 0)
    }

    pub fn authenticate_user(&self, username: &str, password_hash: &str) -> Result<Option<User>> {
        let users = self.get_users()?;
        for user in users {
            if user.username == username && user.password_hash == password_hash {
                return Ok(Some(user));
            }
        }
        Ok(None)
    }

    // Site Settings Persistence
    pub fn get_site_settings(&self) -> Result<SiteSettings> {
        let conn = self.conn.lock().unwrap();
        let mut settings = SiteSettings::default();

        if let Ok(graph_style) = conn.query_row("SELECT value FROM site_settings WHERE key='graph_style'", [], |r| r.get::<_, String>(0)) {
            settings.graph_style = graph_style;
        }
        if let Ok(density) = conn.query_row("SELECT value FROM site_settings WHERE key='density'", [], |r| r.get::<_, String>(0)) {
            settings.density = density;
        }
        if let Ok(refresh_rate) = conn.query_row("SELECT value FROM site_settings WHERE key='refresh_rate'", [], |r| r.get::<_, String>(0)) {
            if let Ok(rate) = refresh_rate.parse::<u32>() {
                settings.refresh_rate = rate;
            }
        }
        if let Ok(time_format) = conn.query_row("SELECT value FROM site_settings WHERE key='time_format'", [], |r| r.get::<_, String>(0)) {
            settings.time_format = time_format;
        }
        if let Ok(site_name) = conn.query_row("SELECT value FROM site_settings WHERE key='site_name'", [], |r| r.get::<_, String>(0)) {
            settings.site_name = site_name;
        }

        Ok(settings)
    }

    pub fn save_site_settings(&self, settings: &SiteSettings) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES ('graph_style', ?1) ON CONFLICT(key) DO UPDATE SET value=?1", params![settings.graph_style]);
        let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES ('density', ?1) ON CONFLICT(key) DO UPDATE SET value=?1", params![settings.density]);
        let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES ('refresh_rate', ?1) ON CONFLICT(key) DO UPDATE SET value=?1", params![settings.refresh_rate.to_string()]);
        let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES ('time_format', ?1) ON CONFLICT(key) DO UPDATE SET value=?1", params![settings.time_format]);
        let _ = conn.execute("INSERT INTO site_settings (key, value) VALUES ('site_name', ?1) ON CONFLICT(key) DO UPDATE SET value=?1", params![settings.site_name]);
        Ok(())
    }

    pub fn get_status_pages(&self) -> Result<Vec<StatusPage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, workspace_id, slug, title, description, is_public, custom_domain, created_at FROM status_pages")?;
        let rows = stmt.query_map([], |row| {
            let is_pub_num: i32 = row.get(5)?;
            Ok(StatusPage {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                slug: row.get(2)?,
                title: row.get(3)?,
                description: row.get(4)?,
                is_public: is_pub_num == 1,
                custom_domain: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut pages = Vec::new();
        for page in rows {
            pages.push(page?);
        }
        Ok(pages)
    }

    pub fn create_status_page(&self, title: &str, slug: &str, description: &str, is_public: bool) -> Result<StatusPage> {
        let conn = self.conn.lock().unwrap();
        let ws_id: String = conn.query_row("SELECT id FROM workspaces LIMIT 1", [], |r| r.get(0))?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let pub_val = if is_public { 1 } else { 0 };

        conn.execute(
            "INSERT INTO status_pages (id, workspace_id, slug, title, description, is_public, custom_domain, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
            params![id, ws_id, slug, title, description, pub_val, now],
        )?;

        Ok(StatusPage {
            id,
            workspace_id: ws_id,
            slug: slug.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            is_public,
            custom_domain: None,
            created_at: now,
        })
    }

    pub fn get_maintenance_windows(&self) -> Result<Vec<MaintenanceWindow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, workspace_id, title, start_time, end_time, suppress_alerts FROM maintenance_windows")?;
        let rows = stmt.query_map([], |row| {
            let supp_num: i32 = row.get(5)?;
            Ok(MaintenanceWindow {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                title: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                suppress_alerts: supp_num == 1,
            })
        })?;

        let mut windows = Vec::new();
        for w in rows {
            windows.push(w?);
        }
        Ok(windows)
    }

    pub fn add_maintenance_window(&self, title: &str, start_time: &str, end_time: &str) -> Result<MaintenanceWindow> {
        let conn = self.conn.lock().unwrap();
        let ws_id: String = conn.query_row("SELECT id FROM workspaces LIMIT 1", [], |r| r.get(0))?;
        let id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO maintenance_windows (id, workspace_id, title, start_time, end_time, suppress_alerts) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            params![id, ws_id, title, start_time, end_time],
        )?;

        Ok(MaintenanceWindow {
            id,
            workspace_id: ws_id,
            title: title.to_string(),
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            suppress_alerts: true,
        })
    }

    pub fn log_audit(&self, user_email: &str, action: &str, details: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let org_id: String = conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)).unwrap_or_default();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO audit_logs (id, org_id, user_email, action, details, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, org_id, user_email, action, details, now],
        )?;
        Ok(())
    }

    pub fn get_audit_logs(&self) -> Result<Vec<AuditLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, org_id, user_email, action, details, timestamp FROM audit_logs ORDER BY timestamp DESC LIMIT 100")?;
        let rows = stmt.query_map([], |row| {
            Ok(AuditLog {
                id: row.get(0)?,
                org_id: row.get(1)?,
                user_email: row.get(2)?,
                action: row.get(3)?,
                details: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })?;

        let mut logs = Vec::new();
        for l in rows {
            logs.push(l?);
        }
        Ok(logs)
    }

    pub fn add_sensor_log(&self, device_name: &str, device_ip: &str, ping: &str, http: &str, bw: &str, timestamp: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sensor_logs (id, device_name, device_ip, ping_status, http_status, bandwidth, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, device_name, device_ip, ping, http, bw, timestamp],
        )?;
        Ok(())
    }

    pub fn get_sensor_logs(&self, limit: usize) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT device_name, device_ip, ping_status, http_status, bandwidth, timestamp FROM sensor_logs ORDER BY rowid DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let name: String = row.get(0)?;
            let ip: String = row.get(1)?;
            let ping: String = row.get(2)?;
            let http: String = row.get(3)?;
            let bw: String = row.get(4)?;
            let ts: String = row.get(5)?;

            let device = format!("{} ({})", name, ip);
            let down = ping.to_lowercase() == "fail" || ping.to_lowercase() == "down";
            let ts_parts: Vec<&str> = ts.split(' ').collect();
            let date = ts_parts.get(0).unwrap_or(&"").to_string();
            let time = ts_parts.get(1).unwrap_or(&"").to_string();

            Ok(serde_json::json!({
                "timestamp": ts,
                "date": date,
                "time": time,
                "device": device,
                "ping": ping,
                "http": http,
                "bandwidth": bw,
                "down": down
            }))
        })?;

        let mut logs = Vec::new();
        for r in rows {
            logs.push(r?);
        }
        Ok(logs)
    }

    pub fn clear_sensor_logs(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sensor_logs", [])?;
        Ok(())
    }
}
