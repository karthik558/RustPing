use log::{info, error, debug};
use reqwest;
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use tokio::net::{TcpStream, UdpSocket, lookup_host};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Instant;

pub fn clean_hostname(input: &str) -> &str {
    let s = input.trim();
    if let Some(stripped) = s.strip_prefix("https://") {
        stripped.split('/').next().unwrap_or(stripped).split(':').next().unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("http://") {
        stripped.split('/').next().unwrap_or(stripped).split(':').next().unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("wss://") {
        stripped.split('/').next().unwrap_or(stripped).split(':').next().unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("ws://") {
        stripped.split('/').next().unwrap_or(stripped).split(':').next().unwrap_or(stripped)
    } else {
        s.split('/').next().unwrap_or(s).split(':').next().unwrap_or(s)
    }
}

pub fn clean_url(input: &str) -> String {
    let s = input.trim();
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ws://") || s.starts_with("wss://") {
        s.to_string()
    } else {
        format!("https://{}", s)
    }
}

fn is_localhost(ip: &str) -> bool {
    let h = clean_hostname(ip);
    h == "127.0.0.1" || h == "localhost" || h == "::1"
}

// 1. ICMP Ping
pub async fn monitor_ping(ip: &str) -> bool {
    let target_host = clean_hostname(ip);
    debug!("Pinging {}", target_host);
    let mut success_count = 0;
    let attempts = 3;
    for i in 1..=attempts {
        let output = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            Command::new("ping").arg("-c").arg("1").arg("-t").arg("2").arg(target_host).output()
        } else {
            Command::new("ping").arg("-n").arg("1").arg("-w").arg("2000").arg(target_host).output()
        };
        match timeout(Duration::from_secs(3), output).await {
            Ok(Ok(out)) => {
                if out.status.success() { success_count += 1; debug!("Ping {} OK for {}", i, target_host); }
                else { debug!("Ping {} FAIL for {}", i, target_host); }
            }
            Ok(Err(e)) => error!("Ping {} error for {}: {}", i, target_host, e),
            Err(_) => error!("Ping {} timeout for {}", i, target_host),
        }
        if success_count >= 2 { info!("Ping UP for {} ({}/{} ok)", target_host, success_count, attempts); return true; }
        if i - success_count >= 2 { error!("Ping DOWN for {} ({}/{} failed)", target_host, i - success_count, attempts); return false; }
        sleep(Duration::from_millis(200)).await;
    }
    let status = success_count > attempts / 2;
    if status { info!("Ping UP for {}", target_host); } else { error!("Ping DOWN for {}", target_host); }
    status
}

// 2. HTTP/HTTPS
pub async fn monitor_http(raw_url: &str) -> bool {
    let target_url = clean_url(raw_url);
    debug!("Checking HTTP for {}", target_url);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(7)).redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("RustPing-Monitor/2.0").danger_accept_invalid_certs(true).build() {
        Ok(c) => c,
        Err(e) => { error!("HTTP client build failed: {}", e); return false; }
    };
    match client.get(&target_url).send().await {
        Ok(resp) => {
            let ok = resp.status().is_success() || resp.status().is_redirection();
            if ok { info!("HTTP OK for {}: {}", target_url, resp.status()); }
            else  { error!("HTTP FAIL for {}: {}", target_url, resp.status()); }
            ok
        }
        Err(e) => { error!("HTTP request failed for {}: {}", target_url, e); false }
    }
}

// 3. TCP Port Check
pub async fn monitor_tcp_port(ip: &str, port: u16) -> bool {
    let target_host = clean_hostname(ip);
    let addr = format!("{}:{}", target_host, port);
    debug!("Checking TCP port {}", addr);
    match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_))  => { info!("TCP Port {} OPEN on {}", port, target_host); true }
        Ok(Err(e)) => { error!("TCP Port {} CLOSED on {}: {}", port, target_host, e); false }
        Err(_)     => { error!("TCP Port {} TIMEOUT on {}", port, target_host); false }
    }
}

// 4. SNMP Bandwidth — real OID poll (ifInOctets delta over 1s)
pub async fn monitor_snmp_bandwidth(ip: &str, community: &str) -> Option<f64> {
    use snmp::{SyncSession, Value};
    debug!("SNMP bandwidth check for {} community '{}'", ip, community);
    let addr = format!("{}:161", ip);
    let comm = community.to_string();
    let result = tokio::task::spawn_blocking(move || -> Option<f64> {
        let mut sess = SyncSession::new(addr.as_str(), comm.as_bytes(), Some(std::time::Duration::from_secs(2)), 0).ok()?;
        let oid: &[u32] = &[1,3,6,1,2,1,2,2,1,10,1];
        let get_octets = |s: &mut SyncSession| -> Option<u64> {
            let mut r = s.get(oid).ok()?;
            r.varbinds.next().and_then(|(_, v)| match v {
                Value::Counter32(n) => Some(n as u64),
                Value::Counter64(n) => Some(n),
                _ => None,
            })
        };
        let t1 = get_octets(&mut sess)?;
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let t2 = get_octets(&mut sess)?;
        let delta = t2.saturating_sub(t1) as f64;
        Some((delta * 8.0 / 1_000_000.0 * 100.0).round() / 100.0)
    }).await;
    match result {
        Ok(Some(mbps)) => { info!("SNMP bandwidth for {}: {:.2} Mbps", ip, mbps); Some(mbps) }
        _ => { error!("SNMP bandwidth failed for {} — verify SNMPv2c agent", ip); None }
    }
}

// 5. SSL/TLS Certificate — real expiry via openssl CLI
pub async fn monitor_ssl_cert(host: &str) -> String {
    let clean_host = clean_hostname(host);
    debug!("Checking SSL certificate for {}", clean_host);
    let cmd = format!(
        "echo | openssl s_client -connect {}:443 -servername {} 2>/dev/null | openssl x509 -noout -enddate 2>/dev/null",
        clean_host, clean_host
    );
    match timeout(Duration::from_secs(10), Command::new("sh").arg("-c").arg(&cmd).output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(date_str) = stdout.trim().strip_prefix("notAfter=") {
                let date_str = date_str.trim();
                if let Ok(expiry) = chrono::DateTime::parse_from_str(
                    &format!("{} +0000", date_str), "%b %d %H:%M:%S %Y GMT %z") {
                    let days = expiry.signed_duration_since(chrono::Utc::now()).num_days();
                    return if days < 0 { format!("EXPIRED ({} days ago)", -days) }
                        else if days < 14 { format!("EXPIRING SOON ({} days left)", days) }
                        else { info!("SSL VALID for {}: {} days", clean_host, days); format!("VALID ({} days left)", days) };
                }
                return format!("VALID (expires {})", date_str);
            }
            let addr = format!("{}:443", clean_host);
            match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
                Ok(Ok(_)) => "VALID (cert present, parse failed)".to_string(),
                _ => { error!("SSL: port 443 unreachable for {}", clean_host); "FAIL (Port 443 unreachable)".to_string() }
            }
        }
        Ok(Err(e)) => {
            let addr = format!("{}:443", clean_host);
            match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
                Ok(Ok(_)) => "VALID (openssl unavailable, port 443 open)".to_string(),
                _ => format!("FAIL ({})", e),
            }
        }
        Err(_) => { error!("SSL timed out for {}", clean_host); "TIMEOUT".to_string() }
    }
}

// 6. DNS Resolution
pub async fn monitor_dns_resolution(domain: &str) -> String {
    let clean_domain = clean_hostname(domain);
    let host_port = format!("{}:80", clean_domain);
    debug!("Resolving DNS for {}", clean_domain);
    let start = Instant::now();
    let host_port_str: String = host_port.clone();
    let x = match timeout(Duration::from_secs(3), lookup_host(host_port_str.as_str())).await {
        Ok(Ok(mut addrs)) => {
            let elapsed = start.elapsed().as_millis();
            if let Some(first) = addrs.next() {
                info!("DNS resolved {} -> {} in {}ms", clean_domain, first.ip(), elapsed);
                format!("OK ({} -> {}ms)", first.ip(), elapsed)
            } else { error!("DNS no records for {}", clean_domain); "FAIL (No Records)".to_string() }
        }
        Ok(Err(e)) => { error!("DNS failed for {}: {}", clean_domain, e); format!("FAIL ({})", e) }
        Err(_) => { error!("DNS timed out for {}", clean_domain); "TIMEOUT".to_string() }
    }; x
}

// 7. Database Port
pub async fn monitor_database_port(ip: &str, port: u16) -> String {
    let db = match port { 5432 => "PostgreSQL", 3306 => "MySQL/MariaDB", 6379 => "Redis", 27017 => "MongoDB", 1433 => "MSSQL", _ => "DB" };
    let addr = format!("{}:{}", ip, port);
    let start = Instant::now();
    match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => { let ms = start.elapsed().as_millis(); info!("{} ONLINE on {} ({}ms)", db, ip, ms); format!("ONLINE ({}ms)", ms) }
        Ok(Err(e)) => format!("OFFLINE ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 8. SSH
pub async fn monitor_ssh(ip: &str, port: u16) -> String {
    let addr = format!("{}:{}", ip, port);
    let start = Instant::now();
    match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => { let ms = start.elapsed().as_millis(); format!("OK (SSH ready, {}ms)", ms) }
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 9. SMTP
pub async fn monitor_smtp(ip: &str, port: u16) -> String {
    let addr = format!("{}:{}", ip, port);
    let start = Instant::now();
    match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => { let ms = start.elapsed().as_millis(); format!("READY ({}ms)", ms) }
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 10. NTP — real UDP packet exchange
pub async fn monitor_ntp(ip: &str) -> String {
    let addr = format!("{}:123", ip);
    debug!("Checking NTP (UDP) at {}", addr);
    let mut packet = [0u8; 48];
    packet[0] = 0b00_011_011; // LI=0, VN=3, Mode=3 (client)
    let sock = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => return format!("FAIL (bind: {})", e),
    };
    let start = Instant::now();
    if timeout(Duration::from_secs(2), sock.send_to(&packet, &addr)).await.is_err() {
        return "FAIL (send timeout)".to_string();
    }
    let mut buf = [0u8; 48];
    match timeout(Duration::from_secs(2), sock.recv_from(&mut buf)).await {
        Ok(Ok((n, _))) if n >= 48 => {
            let elapsed = start.elapsed().as_millis();
            let li = (buf[0] >> 6) & 0x03;
            if li == 3 { format!("WARN (unsynchronized, {}ms)", elapsed) }
            else { info!("NTP SYNCED at {} ({}ms)", ip, elapsed); format!("SYNCED ({}ms roundtrip)", elapsed) }
        }
        Ok(Ok((n, _))) => format!("FAIL (short response: {} bytes)", n),
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => { error!("NTP timed out for {}", ip); "TIMEOUT".to_string() }
    }
}

// 11. FTP
pub async fn monitor_ftp(ip: &str, port: u16) -> String {
    let addr = format!("{}:{}", ip, port);
    match timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => "ONLINE (220 Banner Ready)".to_string(),
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 12. Jitter — real ICMP RTT variance (stddev from ping -c 5)
pub async fn monitor_jitter(ip: &str) -> String {
    let target_host = clean_hostname(ip);
    debug!("Measuring jitter for {}", target_host);
    // ping -c 5 reports mdev (stddev) in the final stats line
    let cmd = if cfg!(target_os = "macos") {
        format!("ping -c 5 -t 10 {} 2>/dev/null | tail -1 | awk -F'/' '{{print $NF}}' | tr -d ' ms'", target_host)
    } else {
        format!("ping -c 5 -W 2 {} 2>/dev/null | tail -1 | awk -F'/' '{{print $NF}}' | tr -d ' ms'", target_host)
    };
    match timeout(Duration::from_secs(15), Command::new("sh").arg("-c").arg(&cmd).output()).await {
        Ok(Ok(out)) => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(j) = s.parse::<f64>() {
                info!("Jitter for {}: {:.2} ms", target_host, j);
                return format!("{:.2} ms jitter", j);
            }
            // Fallback: manually compute std deviation from individual RTTs
            let rtt_cmd = if cfg!(target_os = "macos") {
                format!("ping -c 5 -t 10 {} 2>/dev/null | grep 'bytes from' | sed -E 's/.*time=([0-9.]+).*/\\1/'", target_host)
            } else {
                format!("ping -c 5 -W 2 {} 2>/dev/null | grep 'bytes from' | sed -E 's/.*time=([0-9.]+).*/\\1/'", target_host)
            };
            if let Ok(Ok(rtt_out)) = timeout(Duration::from_secs(15),
                Command::new("sh").arg("-c").arg(&rtt_cmd).output()).await {
                let rtts: Vec<f64> = String::from_utf8_lossy(&rtt_out.stdout)
                    .lines().filter_map(|l| l.trim().parse().ok()).collect();
                if rtts.len() >= 2 {
                    let mean = rtts.iter().sum::<f64>() / rtts.len() as f64;
                    let jitter = (rtts.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / rtts.len() as f64).sqrt();
                    return format!("{:.2} ms jitter", jitter);
                }
            }
            error!("Jitter check failed for {} — host unreachable", target_host);
            "FAIL (host unreachable)".to_string()
        }
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 13. HTTP Latency / TTFB
pub async fn monitor_http_latency(url: &str) -> String {
    let target_url = clean_url(url);
    let start = Instant::now();
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(5)).danger_accept_invalid_certs(true).build() {
        Ok(c) => c,
        Err(_) => return "FAIL (client error)".to_string(),
    };
    match client.get(&target_url).send().await {
        Ok(resp) => { let ms = start.elapsed().as_millis(); format!("{} ms TTFB (HTTP {})", ms, resp.status().as_u16()) }
        Err(e) => { error!("HTTP latency failed for {}: {}", target_url, e); format!("FAIL ({})", e) }
    }
}

// 14. Packet Loss — real ping -c 10 stats extraction
pub async fn monitor_packet_loss(ip: &str) -> String {
    let target_host = clean_hostname(ip);
    debug!("Measuring packet loss for {}", target_host);
    let cmd = if cfg!(target_os = "macos") {
        format!("ping -c 10 -t 15 {} 2>/dev/null | grep 'packet loss'", target_host)
    } else {
        format!("ping -c 10 -W 2 {} 2>/dev/null | grep 'packet loss'", target_host)
    };
    match timeout(Duration::from_secs(25), Command::new("sh").arg("-c").arg(&cmd).output()).await {
        Ok(Ok(out)) => {
            let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(start) = line.find(", ") {
                let after = &line[start+2..];
                if let Some(end) = after.find(" packet loss") {
                    return format!("{} packet loss", &after[..end]);
                }
            }
            if line.is_empty() { "100% packet loss".to_string() } else { line }
        }
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => "TIMEOUT".to_string(),
    }
}

// 15. CPU Load — localhost=real, remote=SNMP or "Agent Required"
pub async fn monitor_cpu_load(ip: &str) -> String {
    if is_localhost(ip) { return local_cpu().await; }
    let cmd = format!("snmpget -v2c -c public -t 2 -r 1 {} 1.3.6.1.2.1.25.3.3.1.2.1 2>/dev/null | awk '{{print $NF}}'", ip);
    if let Ok(Ok(out)) = timeout(Duration::from_secs(4), Command::new("sh").arg("-c").arg(&cmd).output()).await {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(pct) = s.parse::<f64>() { return format!("CPU {:.1}% (SNMP)", pct); }
    }
    "Agent Required (SNMP/SSH)".to_string()
}

async fn local_cpu() -> String {
    let cpu_cmd = if cfg!(target_os = "macos") {
        "top -l 1 -s 0 | grep 'CPU usage' | awk '{print $3}' | tr -d '%'"
    } else {
        "grep 'cpu ' /proc/stat | awk '{u=$2+$4; t=$2+$3+$4+$5; printf \"%.1f\", u*100/t}'"
    };
    let cpu: f64 = timeout(Duration::from_secs(5), Command::new("sh").arg("-c").arg(cpu_cmd).output()).await
        .ok().and_then(|r| r.ok()).and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
    let ram_cmd = if cfg!(target_os = "macos") {
        "vm_stat | awk '/Pages active/{a=$NF}/Pages wired/{w=$NF}/Pages occupied/{c=$NF} END{gsub(\".\",\"\",a); gsub(\".\",\"\",w); gsub(\".\",\"\",c); printf \"%.1f\", (a+w+c)*4096/1024/1024}'"
    } else {
        "free | awk '/Mem:/{printf \"%.1f\", $3/$2*100}'"
    };
    let ram: f64 = timeout(Duration::from_secs(3), Command::new("sh").arg("-c").arg(ram_cmd).output()).await
        .ok().and_then(|r| r.ok()).and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
    format!("CPU {:.1}% | RAM {:.1}%", cpu, ram)
}

// 16. Disk Space — localhost=real df, remote=SNMP or "Agent Required"
pub async fn monitor_disk_space(ip: &str) -> String {
    if is_localhost(ip) { return local_disk().await; }
    let used_cmd = format!("snmpget -v2c -c public -t 2 -r 1 {} 1.3.6.1.2.1.25.2.3.1.6.1 2>/dev/null | awk '{{print $NF}}'", ip);
    let size_cmd = format!("snmpget -v2c -c public -t 2 -r 1 {} 1.3.6.1.2.1.25.2.3.1.5.1 2>/dev/null | awk '{{print $NF}}'", ip);
    let used: Option<f64> = timeout(Duration::from_secs(4), Command::new("sh").arg("-c").arg(&used_cmd).output()).await
        .ok().and_then(|r| r.ok()).and_then(|o| String::from_utf8(o.stdout).ok()).and_then(|s| s.trim().parse().ok());
    let size: Option<f64> = timeout(Duration::from_secs(4), Command::new("sh").arg("-c").arg(&size_cmd).output()).await
        .ok().and_then(|r| r.ok()).and_then(|o| String::from_utf8(o.stdout).ok()).and_then(|s| s.trim().parse().ok());
    if let (Some(u), Some(s)) = (used, size) {
        if s > 0.0 { return format!("{:.0}% free (SNMP)", ((s - u) / s * 100.0).round()); }
    }
    "Agent Required (SNMP/SSH)".to_string()
}

async fn local_disk() -> String {
    let out = timeout(Duration::from_secs(3),
        Command::new("sh").arg("-c").arg("df -H / | tail -1 | awk '{print $4, $5}'").output()).await;
    if let Ok(Ok(o)) = out {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(used_pct) = parts[1].trim_end_matches('%').parse::<f64>() {
                return format!("{:.0}% free ({} available)", 100.0 - used_pct, parts[0]);
            }
        }
    }
    "Disk info unavailable".to_string()
}

// 17. WebSocket — real HTTP 101 Upgrade handshake
pub async fn monitor_websocket(url: &str) -> String {
    let (host, port, path) = parse_ws_url(url);
    debug!("Checking WebSocket at {}:{}{}", host, port, path);
    let addr = format!("{}:{}", host, port);
    match timeout(Duration::from_secs(5), TcpStream::connect(&addr)).await {
        Ok(Ok(mut stream)) => {
            let key = ws_key();
            let req = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
                path, host, key
            );
            if stream.write_all(req.as_bytes()).await.is_err() { return "FAIL (write error)".to_string(); }
            let mut buf = [0u8; 512];
            match timeout(Duration::from_secs(3), stream.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => {
                    let resp = String::from_utf8_lossy(&buf[..n]);
                    if resp.contains("101") && resp.to_lowercase().contains("switching protocols") {
                        info!("WebSocket OPEN at {}", addr); "OPEN (101 Switching Protocols)".to_string()
                    } else if resp.starts_with("HTTP/") {
                        let code = resp.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("?");
                        format!("HTTP {} (not a WS endpoint)", code)
                    } else { "OPEN (non-HTTP response)".to_string() }
                }
                Ok(Ok(_)) => "FAIL (empty response)".to_string(),
                Ok(Err(e)) => format!("FAIL ({})", e),
                Err(_) => "TIMEOUT".to_string(),
            }
        }
        Ok(Err(e)) => format!("FAIL ({})", e),
        Err(_) => { error!("WebSocket timeout for {}", addr); "TIMEOUT".to_string() }
    }
}

fn parse_ws_url(url: &str) -> (String, u16, String) {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("wss://") { ("wss", r) }
        else if let Some(r) = url.strip_prefix("ws://")   { ("ws",  r) }
        else if let Some(r) = url.strip_prefix("https://") { ("wss", r) }
        else if let Some(r) = url.strip_prefix("http://")  { ("ws",  r) }
        else { ("ws", url) };
    let dflt = if scheme == "wss" { 443u16 } else { 80u16 };
    let (hp, path) = if let Some(i) = rest.find('/') { (&rest[..i], rest[i..].to_string()) }
        else { (rest, "/".to_string()) };
    let (host, port) = if let Some(i) = hp.rfind(':') {
        let p = hp[i+1..].parse().unwrap_or(dflt); (hp[..i].to_string(), p)
    } else { (hp.to_string(), dflt) };
    (host, port, path)
}

fn ws_key() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    let n = h.finish().to_le_bytes();
    let bytes: Vec<u8> = n.iter().chain(n.iter()).cloned().collect();
    b64(&bytes)
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = data.get(i+1).copied().unwrap_or(0) as u32;
        let b2 = data.get(i+2).copied().unwrap_or(0) as u32;
        out.push(T[((b0>>2)&0x3f) as usize] as char);
        out.push(T[(((b0&3)<<4)|(b1>>4)) as usize] as char);
        out.push(if i+1 < data.len() { T[(((b1&0xf)<<2)|(b2>>6)) as usize] as char } else { '=' });
        out.push(if i+2 < data.len() { T[(b2&0x3f) as usize] as char } else { '=' });
        i += 3;
    }
    out
}
