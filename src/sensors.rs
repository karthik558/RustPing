use log::{info, error, debug};
use reqwest;
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use rand::Rng;

pub async fn monitor_ping(ip: &str) -> bool {
    debug!("Pinging {}", ip);
    
    let mut success_count = 0;
    let attempts = 3; // Try 3 times before deciding status

    for i in 1..=attempts {
        let output = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            Command::new("ping")
                .arg("-c")
                .arg("1")
                .arg("-t")  // Use -t for macOS timeout
                .arg("2")   // 2 second timeout
                .arg(ip)
                .output()
        } else {
            Command::new("ping")
                .arg("-n")
                .arg("1")
                .arg("-w")
                .arg("2000")
                .arg(ip)
                .output()
        };

        match timeout(Duration::from_secs(3), output).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    success_count += 1;
                    debug!("Ping attempt {} successful for {}", i, ip);
                } else {
                    debug!("Ping attempt {} failed for {}", i, ip);
                }
            }
            Ok(Err(e)) => {
                error!("Ping attempt {} error for {}: {}", i, ip, e);
            }
            Err(_) => error!("Ping attempt {} timed out for {}", i, ip),
        }

        // Two matching results are decisive; do not wait for a redundant
        // third process when the device state is already known.
        if success_count >= 2 {
            info!("Ping UP for {} ({}/{} successful)", ip, success_count, attempts);
            return true;
        }
        if i - success_count >= 2 {
            error!("Ping DOWN for {} ({}/{} failed)", ip, i - success_count, attempts);
            return false;
        }

        // Short delay between attempts
        sleep(Duration::from_millis(200)).await;
    }

    // Consider it up if more than 50% attempts succeeded
    let status = success_count > attempts / 2;
    if status {
        info!("Ping UP for {} ({}/{} successful)", ip, success_count, attempts);
    } else {
        error!("Ping DOWN for {} ({}/{} failed)", ip, attempts - success_count, attempts);
    }
    
    status
}

pub async fn monitor_http(url: &str) -> bool {
    debug!("Checking HTTP status for {}", url);

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create HTTP client: {}", e);
            return false;
        }
    };

    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            let success = status.is_success();
            if success {
                info!("HTTP check successful for {}: {}", url, status);
            } else {
                error!("HTTP check failed for {}: {}", url, status);
            }
            success
        },
        Err(e) => {
            error!("HTTP request failed for {}: {}", url, e);
            false
        }
    }
}

pub async fn monitor_tcp_port(ip: &str, port: u16) -> bool {
    debug!("Checking TCP port {}:{}", ip, port);
    let addr = format!("{}:{}", ip, port);
    match timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => {
            info!("TCP Port {} is OPEN on {}", port, ip);
            true
        }
        Ok(Err(e)) => {
            error!("TCP Port {} is CLOSED on {}: {}", port, ip, e);
            false
        }
        Err(_) => {
            error!("TCP Port {} connection timed out on {}", port, ip);
            false
        }
    }
}

pub async fn monitor_snmp_bandwidth(ip: &str, community: &str) -> Option<f64> {
    debug!("Checking SNMP Bandwidth for {} with community '{}'", ip, community);
    // In a real implementation, you would use snmp-parser or an SNMP client crate to query ifInOctets/ifOutOctets
    // For Phase 1, we will mock a realistic bandwidth value if the device responds to ping, 
    // or return None if it doesn't.
    // Assuming for now it returns a mock random bandwidth if called.
    Some(rand::thread_rng().gen_range(1.0..500.0))
}
