// src/models.rs
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SensorType {
    Ping,
    Http,
    Https,
    Bandwidth,
    Port,
    Snmp,
    SslCert,
    Dns,
    Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    pub ip: String,
    pub category: String,
    pub sensors: Vec<SensorType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snmp_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth_usage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_status: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UserRole {
    Owner,
    Admin,
    Operator,
    Viewer,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Owner => "Owner",
            UserRole::Admin => "Admin",
            UserRole::Operator => "Operator",
            UserRole::Viewer => "Viewer",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "owner" => UserRole::Owner,
            "admin" => UserRole::Admin,
            "operator" => UserRole::Operator,
            _ => UserRole::Viewer,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub role: UserRole,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusPage {
    pub id: String,
    pub workspace_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub is_public: bool,
    pub custom_domain: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MaintenanceWindow {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub suppress_alerts: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditLog {
    pub id: String,
    pub org_id: String,
    pub user_email: String,
    pub action: String,
    pub details: String,
    pub timestamp: String,
}