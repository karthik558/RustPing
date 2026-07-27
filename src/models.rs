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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
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
}