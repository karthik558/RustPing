<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import {
  Activity, ArrowRight, Bell, Check, ChevronDown, ChevronRight, CircleGauge,
  CloudCog, Database, Download, FileClock, Gauge, LayoutDashboard, LogOut,
  Menu, Moon, Network, Plus, Radio, RefreshCw, Router, Search, Server, ShieldCheck,
  Signal, Sun, TerminalSquare, Trash2, X, Zap, Settings, Sliders, Layout, Clock,
  PieChart, Wifi, Users
} from 'lucide-vue-next'

const appRoutes = ['dashboard', 'devices', 'logs', 'settings', 'users']
const brandLogo = '/static/app/rustping-logo.png'
const brandIcon = '/static/app/favicon.png'
const route = ref(window.location.hash.replace('#/', '') || 'login')
const theme = ref(localStorage.getItem('rustping-theme') || 'dark')
const currentUser = ref(JSON.parse(sessionStorage.getItem('currentUser') || 'null'))
const devices = ref([])
const logs = ref([])
const loading = ref(false)
const notice = ref('')
const search = ref('')
const statusFilter = ref('all')
const logSearch = ref('')
const logStatusFilter = ref('all')
const faqOpen = ref(0)
const mobileMenu = ref(false)
const showDeviceModal = ref(false)
const showUserModal = ref(false)
const appUsers = ref(JSON.parse(localStorage.getItem('users') || 'null') || [])

if (!appUsers.value || appUsers.value.length === 0) {
  appUsers.value = [{
    username: 'admin',
    role: 'Admin',
    passwordHash: '8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918',
    permissions: { manage_devices: true, view_logs: true, manage_settings: true, manage_users: true }
  }]
  localStorage.setItem('users', JSON.stringify(appUsers.value))
}

const loginForm = reactive({ username: 'admin', password: '', error: '' })
const deviceForm = reactive({ name: '', ip: '', category: 'Network', sensors: ['Ping'], http_path: '' })
const userForm = reactive({ username: '', password: '', role: 'Operator', permissions: { manage_devices: false, view_logs: true, manage_settings: false, manage_users: false } })
const emailForm = reactive({
  smtp_server: '', smtp_port: '587', smtp_username: '', smtp_password: '',
  from_email: '', to_email: '', test_email: ''
})
const defaultSettings = {
  graphStyle: 'Bar',
  density: 'Comfortable',
  refreshRate: 5000,
  timeFormat: '24h'
}
const userSettings = reactive(JSON.parse(localStorage.getItem('rustping-settings') || JSON.stringify(defaultSettings)))

const throughputData = [22,31,27,46,39,58,51,72,64,81,70,88,77,91,83,96,86,92,79,89,72,84,68,76]
const polylinePoints = computed(() => throughputData.map((n, i) => `${(i / (throughputData.length - 1)) * 100},${100 - n}`).join(' '))
const polygonPoints = computed(() => `0,100 ${polylinePoints.value} 100,100`)

const latencyData = [12,15,14,18,22,25,20,19,15,14,12,18,35,42,30,22,18,15,14,16,14,15,13,12]
const latencyPolyline = computed(() => latencyData.map((n, i) => `${(i / (latencyData.length - 1)) * 100},${100 - (n * 2)}`).join(' '))
const latencyPolygon = computed(() => `0,100 ${latencyPolyline.value} 100,100`)

const uptimeHistory = Array.from({length: 90}, (_, i) => Math.random() > 0.05)

const trafficIngress = [42,51,47,66,59,78,71,92,84,101,90,108,97,111,103,116,106,112,99,109,92,104,88,96]
const trafficEgress = [22,31,27,46,39,58,51,72,64,81,70,88,77,91,83,96,86,92,79,89,72,84,68,76]
const ingressPolyline = computed(() => trafficIngress.map((n, i) => `${(i / (trafficIngress.length - 1)) * 100},${100 - (n * 0.8)}`).join(' '))
const ingressPolygon = computed(() => `0,100 ${ingressPolyline.value} 100,100`)
const egressPolyline = computed(() => trafficEgress.map((n, i) => `${(i / (trafficEgress.length - 1)) * 100},${100 - (n * 0.8)}`).join(' '))
const egressPolygon = computed(() => `0,100 ${egressPolyline.value} 100,100`)

const categoryStats = computed(() => {
  const counts = devices.value.reduce((acc, d) => { acc[d.category] = (acc[d.category] || 0) + 1; return acc; }, {})
  return Object.entries(counts).map(([name, count]) => ({ name, count, percent: Math.round((count / devices.value.length) * 100) })).sort((a,b) => b.count - a.count)
})

const sensorStats = computed(() => {
  let ping = 0, http = 0;
  devices.value.forEach(d => {
    if (d.sensors.includes('Ping')) ping++;
    if (d.sensors.includes('Http') || d.sensors.includes('Https')) http++;
  })
  return { ping, http, total: ping + http }
})

let refreshTimer
let inactivityTimer

const sampleDevices = [
  { name: 'Core Gateway', ip: '10.0.0.1', category: 'Network', sensors: ['Ping', 'Http'], ping_status: true, http_status: true, bandwidth_usage: 84.2 },
  { name: 'Production API', ip: '10.0.1.22', category: 'Services', sensors: ['Ping', 'Https'], ping_status: true, http_status: true, bandwidth_usage: 61.8 },
  { name: 'Primary NAS', ip: '10.0.2.14', category: 'Storage', sensors: ['Ping'], ping_status: true, http_status: null, bandwidth_usage: 46.4 },
  { name: 'Office Gateway', ip: '10.0.3.1', category: 'Network', sensors: ['Ping', 'Http'], ping_status: false, http_status: false, bandwidth_usage: 12.1 },
  { name: 'Cloudflare DNS', ip: '1.1.1.1', category: 'External', sensors: ['Ping', 'Https'], ping_status: true, http_status: true, bandwidth_usage: 32.8 },
]

const features = [
  { icon: Zap, label: 'Async-native engine', tag: 'CORE', copy: 'Concurrent checks with minimal overhead.' },
  { icon: FileClock, label: 'Live event stream', tag: 'REAL TIME', copy: 'Status changes without page refreshes.' },
  { icon: ShieldCheck, label: 'Protected access', tag: 'SECURITY', copy: 'Operational controls stay behind login.' },
  { icon: Database, label: 'Exportable history', tag: 'DATA', copy: 'Filtered evidence in CSV or TXT.' },
  { icon: Bell, label: 'Email alerts', tag: 'ALERTS', copy: 'Failures reach the people who can act.' },
  { icon: Radio, label: 'TCP · UDP probes', tag: 'SOON', copy: 'More sensors as your network evolves.' },
]

const faqs = [
  ['What can RustPing monitor?', 'Any reachable device or service using ICMP Ping, HTTP, and HTTPS checks, with more sensor types planned.'],
  ['Which systems can run it?', 'RustPing is self-hosted and runs on Linux, macOS, and Windows wherever the Rust toolchain is supported.'],
  ['Does it require a hosted account?', 'No. Your device data, credentials, and monitoring history stay in infrastructure you control.'],
  ['Can I export monitoring history?', 'Yes. Filter logs by device and date, then export TXT or CSV files for analysis.'],
]

const isAuthenticated = computed(() => Boolean(currentUser.value) && document.cookie.split(';').some(cookie => cookie.trim() === 'auth=true'))
const isApp = computed(() => appRoutes.includes(route.value))
const onlineCount = computed(() => devices.value.filter(d => d.ping_status !== false).length)
const offlineCount = computed(() => devices.value.filter(d => d.ping_status === false).length)
const health = computed(() => devices.value.length ? Math.round((onlineCount.value / devices.value.length) * 100) : 100)
const filteredDevices = computed(() => devices.value.filter(device => {
  const matchText = `${device.name} ${device.ip} ${device.category}`.toLowerCase().includes(search.value.toLowerCase())
  const matchState = statusFilter.value === 'all'
    || (statusFilter.value === 'online' && device.ping_status !== false)
    || (statusFilter.value === 'offline' && device.ping_status === false)
  return matchText && matchState
}))

const filteredLogs = computed(() => {
  let result = logs.value;
  if (logStatusFilter.value === 'success') {
    result = result.filter(l => !l.down);
  } else if (logStatusFilter.value === 'fail') {
    result = result.filter(l => l.down);
  }
  
  if (logSearch.value) {
    const term = logSearch.value.toLowerCase();
    result = result.filter(l => 
      l.device.toLowerCase().includes(term) || 
      l.timestamp.toLowerCase().includes(term)
    );
  }
  return result;
})

function go(next) {
  if (next === 'home') next = isAuthenticated.value ? 'dashboard' : 'login'
  if (appRoutes.includes(next) && !isAuthenticated.value) next = 'login'
  window.location.hash = `/${next}`
  route.value = next
  mobileMenu.value = false
  window.scrollTo({ top: 0, behavior: 'smooth' })
}

function toggleTheme() {
  theme.value = theme.value === 'dark' ? 'light' : 'dark'
  localStorage.setItem('rustping-theme', theme.value)
}

function flash(message) {
  notice.value = message
  window.setTimeout(() => { notice.value = '' }, 3200)
}

async function loadDevices() {
  if (!isAuthenticated.value) return
  loading.value = true
  try {
    const response = await fetch('/devices', { headers: { Accept: 'application/json' } })
    if (!response.ok) throw new Error()
    const data = await response.json()
    devices.value = data.length ? data : sampleDevices
  } catch {
    devices.value = sampleDevices
  } finally {
    loading.value = false
  }
}

async function loadLogs() {
  try {
    const response = await fetch('/logs_json', { headers: { Accept: 'application/json' } })
    if (!response.ok) throw new Error()
    logs.value = await response.json()
  } catch {
    logs.value = [
      { timestamp: '2026-07-27 17:11:24', device: 'Core Gateway', ping: 'OK', http: 'OK', bandwidth: '84.2 Mbps', down: false },
      { timestamp: '2026-07-27 17:11:19', device: 'Office Gateway', ping: 'FAIL', http: 'FAIL', bandwidth: '12.1 Mbps', down: true },
      { timestamp: '2026-07-27 17:11:14', device: 'Primary NAS', ping: 'OK', http: 'N/A', bandwidth: '46.4 Mbps', down: false },
    ]
  }
}

async function loadConfiguredUsers() {
  try {
    const response = await fetch('/static/config.js')
    const source = await response.text()
    const json = source.replace(/^const AUTH_CONFIG\s*=\s*/, '').replace(/;\s*$/, '')
    return JSON.parse(json).users || []
  } catch {
    return []
  }
}

async function login() {
  loginForm.error = ''
  const storedUsers = JSON.parse(localStorage.getItem('users') || '[]')
  const users = storedUsers.length ? storedUsers : await loadConfiguredUsers()
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(loginForm.password))
  const hash = [...new Uint8Array(digest)].map(value => value.toString(16).padStart(2, '0')).join('')
  const match = users.find(user => user.username === loginForm.username && user.passwordHash === hash)
  const isDefaultAdmin = loginForm.username === 'admin' && hash === '8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918'
  if (!match && !isDefaultAdmin) {
    loginForm.error = 'Those credentials do not match an active account.'
    return
  }
  const user = match || { username: 'admin', role: 'admin' }
  const defaultPermissions = { manage_devices: true, view_logs: true, manage_settings: true, manage_users: true }
  currentUser.value = { 
    username: user.username, 
    role: user.role || 'admin', 
    lastLogin: new Date().toISOString(),
    permissions: user.permissions || defaultPermissions
  }
  sessionStorage.setItem('currentUser', JSON.stringify(currentUser.value))
  document.cookie = 'auth=true; path=/; SameSite=Lax'
  await loadDevices()
  go('dashboard')
  resetInactivityTimer()
}

function logout() {
  currentUser.value = null
  sessionStorage.removeItem('currentUser')
  document.cookie = 'auth=; Max-Age=0; path=/; SameSite=Lax'
  if (inactivityTimer) window.clearTimeout(inactivityTimer)
  go('home')
}

function toggleSensor(sensor) {
  deviceForm.sensors = deviceForm.sensors.includes(sensor)
    ? deviceForm.sensors.filter(item => item !== sensor)
    : [...deviceForm.sensors, sensor]
}

async function addDevice() {
  if (!deviceForm.name || !deviceForm.ip || !deviceForm.category || !deviceForm.sensors.length) {
    flash('Complete the required fields and choose a sensor.')
    return
  }
  const payload = { ...deviceForm, sensors: [...deviceForm.sensors] }
  try {
    const response = await fetch('/devices', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload)
    })
    if (!response.ok) throw new Error()
    await loadDevices()
    flash(`${payload.name} is now being monitored.`)
  } catch {
    devices.value.push({ ...payload, ping_status: null, http_status: null, bandwidth_usage: null })
    flash('Added in preview mode. Start RustPing to persist it.')
  }
  Object.assign(deviceForm, { name: '', ip: '', category: 'Network', sensors: ['Ping'], http_path: '' })
  showDeviceModal.value = false
}

async function deleteDevice(device) {
  const sourceIndex = devices.value.indexOf(device)
  if (sourceIndex < 0 || !window.confirm(`Stop monitoring ${device.name}?`)) return
  try { await fetch(`/devices/${sourceIndex}`, { method: 'DELETE' }) } catch { /* preview mode */ }
  devices.value.splice(sourceIndex, 1)
  flash(`${device.name} removed from monitoring.`)
}

function applyUserTemplate() {
  if (userForm.role === 'Admin') {
    userForm.permissions = { manage_devices: true, view_logs: true, manage_settings: true, manage_users: true }
  } else if (userForm.role === 'Read-Only') {
    userForm.permissions = { manage_devices: false, view_logs: false, manage_settings: false, manage_users: false }
  } else {
    userForm.permissions = { manage_devices: false, view_logs: true, manage_settings: false, manage_users: false }
  }
}

async function saveUser() {
  if (!userForm.username || !userForm.password) {
    flash('Username and password are required.')
    return
  }
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(userForm.password))
  const hash = [...new Uint8Array(digest)].map(value => value.toString(16).padStart(2, '0')).join('')
  
  appUsers.value.push({
    username: userForm.username,
    role: userForm.role,
    passwordHash: hash,
    permissions: { ...userForm.permissions }
  })
  localStorage.setItem('users', JSON.stringify(appUsers.value))
  
  Object.assign(userForm, { username: '', password: '', role: 'Operator', permissions: { manage_devices: false, view_logs: true, manage_settings: false, manage_users: false } })
  showUserModal.value = false
  flash('User created successfully.')
}

function deleteUser(index) {
  if (appUsers.value[index].username === 'admin' && appUsers.value.length === 1) {
    flash('Cannot delete the last admin user.')
    return
  }
  appUsers.value.splice(index, 1)
  localStorage.setItem('users', JSON.stringify(appUsers.value))
}

async function loadEmailConfig() {
  try {
    const response = await fetch('/api/email/config')
    if (response.ok) Object.assign(emailForm, await response.json())
  } catch { /* disconnected state */ }
}

async function saveEmailConfig(test = false) {
  const endpoint = test ? '/api/email/config/test' : '/api/email/config'
  const payload = test ? { test_email: emailForm.test_email || emailForm.to_email } : emailForm
  try {
    const response = await fetch(endpoint, {
      method: test ? 'POST' : 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
    if (!response.ok) throw new Error()
    flash(test ? 'Test notification sent.' : 'Alert configuration saved.')
  } catch {
    flash('Start the Rust service to update alert settings.')
  }
}

function formatStatus(device) {
  return device.ping_status === false ? 'Offline' : device.ping_status === true ? 'Operational' : 'Checking'
}

function exportLogs() {
  window.location.href = '/export_log?format=csv'
}

function handleHash() {
  let next = window.location.hash.replace('#/', '')
  if (!next || next === 'home') next = isAuthenticated.value ? 'dashboard' : 'login'
  route.value = appRoutes.includes(next) && !isAuthenticated.value ? 'login' : next
}

async function refreshData() {
  await loadDevices()
  if (route.value === 'logs') await loadLogs()
}

watch(route, async next => {
  if (!appRoutes.includes(next)) return
  await loadDevices()
  if (next === 'logs') await loadLogs()
  if (next === 'settings') await loadEmailConfig()
})

watch(userSettings, (val) => {
  localStorage.setItem('rustping-settings', JSON.stringify(val))
  if (refreshTimer) {
    window.clearInterval(refreshTimer)
    refreshTimer = window.setInterval(() => isApp.value && isAuthenticated.value && refreshData(), val.refreshRate)
  }
}, { deep: true })

function resetInactivityTimer() {
  if (inactivityTimer) window.clearTimeout(inactivityTimer)
  if (isAuthenticated.value) {
    inactivityTimer = window.setTimeout(() => {
      logout()
      flash('Session expired due to inactivity.')
    }, 2 * 60 * 1000)
  }
}

function handleActivity() {
  if (isAuthenticated.value) {
    resetInactivityTimer()
  }
}

onMounted(() => {
  window.addEventListener('hashchange', handleHash)
  handleHash()
  if (isAuthenticated.value) {
    loadDevices()
    resetInactivityTimer()
  }
  refreshTimer = window.setInterval(() => isApp.value && isAuthenticated.value && refreshData(), userSettings.refreshRate)
  
  window.addEventListener('mousemove', handleActivity)
  window.addEventListener('keydown', handleActivity)
  window.addEventListener('click', handleActivity)
})

onBeforeUnmount(() => {
  window.removeEventListener('hashchange', handleHash)
  window.clearInterval(refreshTimer)
  
  window.removeEventListener('mousemove', handleActivity)
  window.removeEventListener('keydown', handleActivity)
  window.removeEventListener('click', handleActivity)
  if (inactivityTimer) window.clearTimeout(inactivityTimer)
})
</script>

<template>
  <div :class="['site', `theme-${theme}`, `density-${userSettings.density.toLowerCase()}`]">
    <Transition name="toast"><div v-if="notice" class="toast"><Signal :size="15" />{{ notice }}</div></Transition>

    <template v-if="route === 'home'">
      <header class="site-nav shell">
        <button class="brand" aria-label="RustPing home" @click="go('home')"><img :src="brandLogo" alt="RustPing" /></button>
        <nav class="desktop-nav" aria-label="Primary navigation">
          <a href="#system">System</a><a href="#interface">Interface</a><a href="#features">Features</a><a href="#faq">Questions</a>
        </nav>
        <button class="signal-button compact" @click="go(isAuthenticated ? 'dashboard' : 'login')">{{ isAuthenticated ? 'Open console' : 'Deploy' }} <ArrowRight :size="14" /></button>
      </header>

      <main>
        <section class="hero shell">
          <div class="eyebrow"><span></span> Network observability / self-hosted</div>
          <div class="hero-grid">
            <div>
              <h1>Know your<br />network.<br /><em>Before it<br />knows you.</em></h1>
              <p>RustPing turns high-speed infrastructure telemetry into a calm, precise view of what is alive, what is slow, and what needs you now.</p>
              <div class="hero-actions">
                <button class="signal-button" @click="go(isAuthenticated ? 'dashboard' : 'login')">Start monitoring <ArrowRight :size="16" /></button>
                <a href="#system" class="text-link">See how it works <ChevronRight :size="14" /></a>
              </div>
              <div class="build-meta">
                <div><small>ENGINE</small><strong>Rust + Tokio</strong></div>
                <div><small>CHECKS</small><strong>Ping · HTTP · HTTPS</strong></div>
                <div><small>DEPLOYMENT</small><strong>Self-hosted</strong></div>
              </div>
            </div>
            <div class="hero-console">
              <div class="orbit orbit-one"></div><div class="orbit orbit-two"></div>
              <div class="console-card">
                <div class="console-bar"><span class="live-dot"></span><small>LIVE / NETWORK</small><span class="console-actions">•••</span></div>
                <div class="console-heading"><div><small>ACTIVE DEVICES</small><strong>12 devices online</strong></div><div class="health-ring">99<sup>%</sup></div></div>
                <div class="bars" aria-label="Network throughput chart"><span v-for="height in [22,34,29,48,42,61,53,68,59,72,64,81]" :key="height" :style="{height: `${height}%`}"></span></div>
                <div class="axis"><span>00:00</span><span>08:00</span><span>16:00</span><span>NOW</span></div>
              </div>
              <div class="mini-panel panel-top"><small>UPTIME</small><strong>99.99%</strong></div>
              <div class="mini-panel panel-bottom"><span class="live-dot"></span><strong>Zero incidents</strong></div>
            </div>
          </div>
        </section>

        <div class="trust-strip"><div class="shell"><span><Zap :size="13" /> Async-native core</span><span><ShieldCheck :size="13" /> Runs on your network</span><span><Gauge :size="13" /> 5-second refresh</span></div></div>

        <section id="system" class="section shell">
          <div class="section-kicker">01 / The system</div>
          <div class="section-head"><h2>Speed where it matters.<br /><span>Clarity everywhere else.</span></h2><p>From a single gateway to a multi-subnet estate, every signal arrives in one decisive operational view.</p></div>
          <div class="value-grid">
            <article><Zap :size="18" /><div><h3>Fast by design</h3><p>Async monitoring keeps every probe moving without unnecessary overhead.</p></div><div class="metric-line"><strong>00.2</strong><span></span></div></article>
            <article><Activity :size="18" /><div><h3>Live state, no noise</h3><p>At-a-glance status turns raw checks into a calm, scannable signal.</p></div><div class="micro-bars"><i v-for="n in [26,38,32,55,49,68,61,83]" :key="n" :style="{height:`${n}%`}"></i></div></article>
            <article><Network :size="18" /><div><h3>One view, every target</h3><p>Keep local devices, public endpoints, and HTTP services together.</p></div><div class="node-map"><i></i><i></i><i></i><i></i><span></span></div></article>
            <article><CloudCog :size="18" /><div><h3>Evidence on demand</h3><p>Filter and export event history for incident reviews and reporting.</p></div><div class="toggle-demo"><b>LIVE</b><span>24H</span><span>7D</span></div></article>
          </div>
        </section>

        <section id="interface" class="section interface-section shell">
          <div class="section-kicker">02 / One view</div>
          <div class="section-head"><h2>Your entire network.<br /><span>One decisive glance.</span></h2><p>A real operational interface—not a wall of charts. The most important states stay closest to your attention.</p></div>
          <div class="product-frame">
            <div class="mock-sidebar"><img :src="brandIcon" alt="" /><i class="active"></i><i></i><i></i><i></i><i></i></div>
            <div class="mock-main">
              <div class="mock-top"><strong>Network overview</strong><span>Last update · now</span></div>
              <div class="mock-stats"><span><small>DEVICES</small><b>12</b></span><span><small>ONLINE</small><b>11</b></span><span><small>UPTIME</small><b>99.9%</b></span><span><small>ALERTS</small><b class="lime">01</b></span></div>
              <div class="mock-table">
                <div class="mock-row head"><span>DEVICE</span><span>ADDRESS</span><span>SENSORS</span><span>STATE</span></div>
                <div class="mock-row" v-for="device in sampleDevices.slice(0,4)" :key="device.name"><span><i :class="{bad: device.ping_status === false}"></i>{{ device.name }}</span><span>{{ device.ip }}</span><span>{{ device.sensors.join(' · ') }}</span><span :class="{danger: device.ping_status === false}">{{ device.ping_status ? 'Operational' : 'Offline' }}</span></div>
              </div>
              <div class="mock-bottom"><div><small>THROUGHPUT</small><div class="soft-bars"><i v-for="n in [40,70,52,85,62,44,76,58]" :style="{height:`${n}%`}" :key="n"></i></div></div><div><small>SENSOR MIX</small><div class="donut"></div></div></div>
            </div>
          </div>
          <div class="interface-notes"><span><b>ICMP + HTTP</b>Multiple probe types in one table.</span><span><b>Live event stream</b>Status changes without refresh.</span><span><b>Exportable history</b>Operational evidence when needed.</span></div>
        </section>

        <section id="features" class="feature-section">
          <div class="shell">
            <div class="section-kicker dark-kicker">03 / Product surface</div>
            <div class="section-head dark-copy"><h2>Built in public.<br /><span>Driven by operations.</span></h2><p>Every release makes RustPing faster, easier to deploy, and more useful when something goes wrong.</p></div>
            <div class="feature-list"><article v-for="(feature, index) in features" :key="feature.label"><small>0{{ index + 1 }}</small><component :is="feature.icon" :size="17" /><strong>{{ feature.label }}</strong><span>{{ feature.copy }}</span><b>{{ feature.tag }}</b><Plus :size="15" /></article></div>
          </div>
        </section>

        <section class="section steps-section shell">
          <div><div class="section-kicker">04 / Deployment</div><h2>From zero to<br />signal<br />in three steps.</h2><ol><li><b>Build</b><span>Compile the optimized service.</span></li><li><b>Launch</b><span>Run it inside your network.</span></li><li><b>Observe</b><span>Open the console and act.</span></li></ol></div>
          <div class="terminal-card"><div class="terminal-top"><span></span><span></span><span></span><small>rustping / deploy</small></div><code><i>$</i> git clone https://github.com/karthik558/Rust-Ping<br /><i>$</i> npm install && npm run build<br /><i>$</i> cargo run</code><div class="terminal-status"><span class="live-dot"></span> NETWORK MONITOR READY <b>127.0.0.1:8000</b></div></div>
        </section>

        <section id="faq" class="section faq-section shell">
          <div class="section-kicker">05 / Common ground</div><h2>Common questions.</h2>
          <div class="faq-list"><article v-for="(item, index) in faqs" :key="item[0]" :class="{open: faqOpen === index}"><button @click="faqOpen = faqOpen === index ? -1 : index"><small>0{{ index + 1 }}</small><span>{{ item[0] }}</span><ChevronDown :size="15" /></button><p>{{ item[1] }}</p></article></div>
        </section>

        <section class="cta-section"><div class="shell"><div><small>READY TO GO LIVE?</small><h2>Put your network<br />on speaking terms.</h2></div><button class="dark-button" @click="go(isAuthenticated ? 'dashboard' : 'login')">Deploy RustPing <ArrowRight :size="15" /></button></div></section>
      </main>
      <footer class="site-footer shell"><img :src="brandLogo" alt="RustPing" /><div><a href="#system">System</a><a href="#interface">Interface</a><a href="#features">Features</a><a href="#faq">Questions</a></div><small>© 2026 RustPing. Open source under MIT.</small></footer>
    </template>

    <template v-else-if="route === 'login'">
      <main class="auth-page">
        <button class="auth-brand" @click="go('home')"><img :src="brandLogo" alt="RustPing" /></button>
        <section class="auth-copy"><div class="eyebrow"><span></span> Secure network console</div><h1>Signal is waiting.<br /><em>Step inside.</em></h1><p>One protected surface for the health, history, and control of your network.</p><div class="auth-metric"><span><Signal :size="18" /></span><div><small>SERVICE STATUS</small><strong>Monitoring engine ready</strong></div><i class="live-dot"></i></div></section>
        <section class="login-panel"><div><small>AUTHENTICATION / 01</small><h2>Welcome back.</h2><p>Use your RustPing operator credentials.</p></div><form @submit.prevent="login"><label>Username<input v-model="loginForm.username" autocomplete="username" placeholder="Operator ID" /></label><label>Password<input v-model="loginForm.password" type="password" autocomplete="current-password" placeholder="••••••••" /></label><p v-if="loginForm.error" class="form-error">{{ loginForm.error }}</p><button class="signal-button" type="submit">Enter console <ArrowRight :size="16" /></button></form></section>
      </main>
    </template>

    <template v-else>
      <div class="app-shell">
        <aside :class="['app-sidebar', {open: mobileMenu}]">
          <button class="side-brand" aria-label="RustPing console" @click="go('dashboard')"><img :src="brandIcon" alt="" /><span>RUST <b>PING</b></span></button>
          <nav>
            <button :class="{active: route === 'dashboard'}" @click="go('dashboard')"><LayoutDashboard :size="18" />Overview</button>
            <button :class="{active: route === 'devices'}" @click="go('devices')"><Server :size="18" />Devices <b>{{ devices.length }}</b></button>
            <button v-if="currentUser?.permissions?.view_logs" :class="{active: route === 'logs'}" @click="go('logs')"><TerminalSquare :size="18" />Event stream</button>
            <button v-if="currentUser?.permissions?.manage_settings" :class="{active: route === 'settings'}" @click="go('settings')"><Settings :size="18" />Settings</button>
            <button v-if="currentUser?.permissions?.manage_users" :class="{active: route === 'users'}" @click="go('users')"><Users :size="18" />Operators</button>
          </nav>
          <div class="side-bottom"><button @click="toggleTheme"><Sun v-if="theme === 'dark'" :size="17" /><Moon v-else :size="17" />{{ theme === 'dark' ? 'Light' : 'Dark' }} mode</button><button @click="logout"><LogOut :size="17" />Sign out</button><div class="operator"><span>{{ currentUser?.username?.slice(0,1).toUpperCase() }}</span><div><strong>{{ currentUser?.username }}</strong><small>{{ currentUser?.role }} operator</small></div></div></div>
        </aside>
        <div class="app-body">
          <header class="app-topbar"><button class="mobile-toggle" aria-label="Toggle navigation" @click="mobileMenu = !mobileMenu"><Menu :size="20" /></button><div class="app-breadcrumb"><span>RUSTPING</span><ChevronRight :size="13" /><b>{{ route }}</b></div><div class="top-actions"><span class="engine-status"><i></i> Engine online</span><button aria-label="Refresh data" @click="refreshData"><RefreshCw :class="{spin: loading}" :size="17" /></button><button aria-label="Notifications" @click="go('devices'); statusFilter = 'offline'"><Bell :size="17" /><i v-if="offlineCount"></i></button></div></header>
          <main class="app-content">
            <template v-if="route === 'dashboard'">
              <div class="page-title"><div><small>LIVE OPERATIONS</small><h1>Network overview</h1><p>Every monitored signal, ordered for action.</p></div><button v-if="currentUser?.permissions?.manage_devices" class="signal-button compact" @click="showDeviceModal = true"><Plus :size="15" /> Add device</button></div>
              <section class="metric-grid"><article><div><small>TOTAL DEVICES</small><strong>{{ devices.length }}</strong></div><span><Server :size="19" /></span><p><b>+{{ devices.length }}</b> active monitors</p></article><article><div><small>OPERATIONAL</small><strong>{{ onlineCount }}</strong></div><span><Signal :size="19" /></span><p><b>{{ health }}%</b> network health</p></article><article><div><small>INCIDENTS</small><strong>{{ offlineCount.toString().padStart(2,'0') }}</strong></div><span class="warn"><Bell :size="19" /></span><p :class="{danger: offlineCount}">{{ offlineCount ? 'Requires attention' : 'No active incidents' }}</p></article><article><div><small>UPTIME</small><strong>99.9<sup>%</sup></strong></div><span><CircleGauge :size="19" /></span><p><b>30D</b> rolling average</p></article></section>
              <section class="dashboard-grid">
                <article class="panel throughput-panel"><div class="panel-title"><div><small>NETWORK LOAD</small><h2>Throughput</h2></div><span>LAST 12 HOURS</span></div>
                  <div class="large-bars" :class="userSettings.graphStyle.toLowerCase()">
                    <template v-if="userSettings.graphStyle === 'Bar'">
                      <i v-for="(n,index) in throughputData" :key="index" :style="{height:`${n}%`}"></i>
                    </template>
                    <template v-else-if="userSettings.graphStyle === 'Line'">
                      <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg-graph">
                        <polyline :points="polylinePoints" fill="none" stroke="var(--lime)" stroke-width="1.5" />
                      </svg>
                    </template>
                    <template v-else-if="userSettings.graphStyle === 'Area'">
                      <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg-graph">
                        <polygon :points="polygonPoints" fill="var(--lime-muted)" />
                        <polyline :points="polylinePoints" fill="none" stroke="var(--lime)" stroke-width="1.5" />
                      </svg>
                    </template>
                  </div>
                  <div class="axis"><span>06:00</span><span>12:00</span><span>18:00</span><span>NOW</span></div></article>
                  
                <article class="panel health-panel"><div class="panel-title"><div><small>FLEET STATE</small><h2>Overall health</h2></div><span>LIVE</span></div><div class="health-donut" :style="{'--health': `${health * 3.6}deg`}"><div><strong>{{ health }}<sup>%</sup></strong><small>HEALTHY</small></div></div><div class="health-legend"><span><i></i>Online <b>{{ onlineCount }}</b></span><span><i></i>Offline <b>{{ offlineCount }}</b></span></div></article>
                
                <article class="panel throughput-panel"><div class="panel-title"><div><small>PERFORMANCE</small><h2>Global Latency</h2></div><span>24 HOURS</span></div>
                  <div class="large-bars" :class="userSettings.graphStyle.toLowerCase()">
                    <template v-if="userSettings.graphStyle === 'Bar'">
                      <i v-for="(n,index) in latencyData" :key="index" :style="{height:`${n * 2}%`}"></i>
                    </template>
                    <template v-else-if="userSettings.graphStyle === 'Line'">
                      <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg-graph">
                        <polyline :points="latencyPolyline" fill="none" stroke="var(--lime)" stroke-width="1.5" />
                      </svg>
                    </template>
                    <template v-else-if="userSettings.graphStyle === 'Area'">
                      <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg-graph">
                        <polygon :points="latencyPolygon" fill="var(--lime-muted)" />
                        <polyline :points="latencyPolyline" fill="none" stroke="var(--lime)" stroke-width="1.5" />
                      </svg>
                    </template>
                  </div>
                  <div class="axis"><span>0ms</span><span>15ms</span><span>25ms</span><span>NOW</span></div></article>
                
                <article class="panel traffic-panel"><div class="panel-title"><div><small>BANDWIDTH</small><h2>Network Traffic</h2></div><span>LIVE</span></div>
                  <div class="traffic-bars">
                    <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg-graph">
                      <polygon :points="egressPolygon" fill="var(--lime-muted)" />
                      <polyline :points="egressPolyline" fill="none" stroke="var(--lime)" stroke-width="1.5" />
                      <polygon :points="ingressPolygon" fill="rgba(167, 195, 72, 0.2)" />
                      <polyline :points="ingressPolyline" fill="none" stroke="var(--lime)" stroke-width="2.5" />
                    </svg>
                  </div>
                  <div class="health-legend"><span><i></i>Ingress <b>4.2GB</b></span><span><i style="background:var(--lime-muted)"></i>Egress <b>1.8GB</b></span></div>
                </article>

                <article class="panel uptime-panel">
                  <div class="panel-title"><div><small>RELIABILITY</small><h2>Uptime History</h2></div><span>90 DAYS</span></div>
                  <div class="uptime-heatmap">
                    <div v-for="(up, index) in uptimeHistory" :key="index" :class="['uptime-block', { down: !up }]"></div>
                  </div>
                  <div class="axis"><span>90 days ago</span><span>Today</span></div>
                </article>

                <article class="panel donut-panel">
                  <div class="panel-title"><div><small>CAPABILITIES</small><h2>Sensors Active</h2></div></div>
                  <div class="health-donut" :style="{'--health': `${(sensorStats.ping / sensorStats.total) * 360 || 0}deg`}">
                    <div><strong>{{ sensorStats.total }}</strong><small>PROBES</small></div>
                  </div>
                  <div class="health-legend"><span><i></i>ICMP Ping <b>{{ sensorStats.ping }}</b></span><span><i style="background:var(--danger)"></i>HTTP/S <b>{{ sensorStats.http }}</b></span></div>
                </article>

                <article class="panel category-panel">
                  <div class="panel-title"><div><small>INVENTORY</small><h2>Categories</h2></div></div>
                  <div class="category-list">
                    <div v-for="cat in categoryStats" :key="cat.name">
                      <div class="cat-label"><strong>{{ cat.name }}</strong><span>{{ cat.count }}</span></div>
                      <div class="cat-bar"><i :style="{ width: `${cat.percent}%` }"></i></div>
                    </div>
                  </div>
                </article>

                <article class="panel device-panel"><div class="panel-title"><div><small>PRIORITY VIEW</small><h2>Device status</h2></div><button @click="go('devices')">View all <ArrowRight :size="13" /></button></div><div class="device-list"><div v-for="device in devices.slice(0,5)" :key="device.name"><span class="device-icon"><Router :size="17" /></span><div><strong>{{ device.name }}</strong><small>{{ device.ip }} · {{ device.category }}</small></div><span :class="['status-pill', {offline: device.ping_status === false}]"><i></i>{{ formatStatus(device) }}</span></div></div></article>
                <article class="panel events-panel"><div class="panel-title"><div><small>ACTIVITY</small><h2>Recent events</h2></div><button @click="go('logs')">Open stream <ArrowRight :size="13" /></button></div><div class="event-list"><div><span><Check :size="14" /></span><p><b>Health check completed</b><small>All probes responded · just now</small></p></div><div v-if="offlineCount"><span class="error"><X :size="14" /></span><p><b>{{ offlineCount }} device{{ offlineCount > 1 ? 's' : '' }} unreachable</b><small>Escalation policy active · 2m ago</small></p></div><div><span><RefreshCw :size="14" /></span><p><b>Device registry synced</b><small>{{ devices.length }} records verified · 5m ago</small></p></div></div></article>
              </section>
            </template>

            <template v-else-if="route === 'devices'">
              <div class="page-title"><div><small>INVENTORY / {{ devices.length.toString().padStart(2,'0') }}</small><h1>Monitored devices</h1><p>Manage every target and the checks assigned to it.</p></div><button v-if="currentUser?.permissions?.manage_devices" class="signal-button compact" @click="showDeviceModal = true"><Plus :size="15" /> Add device</button></div>
              <div class="table-tools"><label><Search :size="16" /><input v-model="search" placeholder="Search name, IP, or category" /></label><div><button :class="{active: statusFilter === 'all'}" @click="statusFilter = 'all'">All</button><button :class="{active: statusFilter === 'online'}" @click="statusFilter = 'online'">Online</button><button :class="{active: statusFilter === 'offline'}" @click="statusFilter = 'offline'">Offline</button></div></div>
              <section class="data-table"><div class="table-row table-head"><span>Device</span><span>Address</span><span>Category</span><span>Sensors</span><span>Status</span><span></span></div><div v-for="device in filteredDevices" :key="device.ip" class="table-row"><span class="device-cell"><i :class="{offline: device.ping_status === false}"></i><b>{{ device.name }}</b></span><span class="mono">{{ device.ip }}</span><span>{{ device.category }}</span><span class="sensor-list"><b v-for="sensor in device.sensors" :key="sensor">{{ sensor }}</b></span><span><em :class="['status-pill', {offline: device.ping_status === false}]"><i></i>{{ formatStatus(device) }}</em></span><span><button v-if="currentUser?.permissions?.manage_devices" class="icon-button danger-button" :aria-label="`Delete ${device.name}`" @click="deleteDevice(device)"><Trash2 :size="15" /></button></span></div><div v-if="!filteredDevices.length" class="empty-state"><Search :size="24" /><strong>No devices found</strong><span>Try another search or filter.</span></div></section>
            </template>

            <template v-else-if="route === 'logs'">
              <div class="page-title"><div><small>DIAGNOSTICS</small><h1>Live event stream</h1><p>Raw evidence from every active network check.</p></div><button class="outline-button" @click="exportLogs"><Download :size="15" /> Export CSV</button></div>
              <div class="table-tools"><label><Search :size="16" /><input v-model="logSearch" placeholder="Search event stream..." /></label><div><button :class="{active: logStatusFilter === 'all'}" @click="logStatusFilter = 'all'">All</button><button :class="{active: logStatusFilter === 'success'}" @click="logStatusFilter = 'success'">Success</button><button :class="{active: logStatusFilter === 'fail'}" @click="logStatusFilter = 'fail'">Failures</button></div></div>
              <section class="terminal-log"><div class="log-head"><span class="live-dot"></span><b>STREAM ACTIVE</b><small>{{ filteredLogs.length }} entries</small></div><div class="log-line" v-for="(log,index) in filteredLogs" :key="index"><span>{{ userSettings.timeFormat === '12h' ? new Date(log.timestamp).toLocaleString('en-US',{hour:'numeric',minute:'numeric',second:'numeric',hour12:true}).split(' ')[0] : log.timestamp.split(' ')[1] }}</span><b :class="{down: log.down}">{{ log.down ? 'FAIL' : 'OK' }}</b><strong>{{ log.device }}</strong><em>PING {{ log.ping || 'N/A' }}</em><em>HTTP {{ log.http || 'N/A' }}</em><small>{{ log.bandwidth || '—' }}</small></div><div v-if="!filteredLogs.length" class="empty-state"><TerminalSquare :size="24" /><strong>No logs match filter</strong><span>Try adjusting your search criteria.</span></div></section>
            </template>

            <template v-else-if="route === 'settings'">
              <div class="page-title"><div><small>PREFERENCES</small><h1>Settings</h1><p>Configure interface options and alert routing.</p></div><button v-if="currentUser?.permissions?.manage_settings" class="signal-button compact" @click="saveEmailConfig(false)">Save configuration <Check :size="15" /></button></div>
              <section class="settings-grid">
                <!-- Appearance Settings -->
                <article class="panel settings-card full-width">
                  <div class="settings-title"><span><Layout :size="19" /></span><div><h2>Interface & Appearance</h2><p>Customize the console layout and data representation.</p></div></div>
                  <div class="settings-row">
                    <div class="setting-item">
                      <label>Graph Style</label>
                      <select v-model="userSettings.graphStyle">
                        <option value="Bar">Bar Chart</option>
                        <option value="Line">Line Chart</option>
                        <option value="Area">Area Chart</option>
                      </select>
                    </div>
                    <div class="setting-item">
                      <label>Data Density</label>
                      <select v-model="userSettings.density">
                        <option value="Comfortable">Comfortable</option>
                        <option value="Compact">Compact</option>
                      </select>
                    </div>
                    <div class="setting-item">
                      <label>Color Theme</label>
                      <button class="outline-button" @click="toggleTheme">{{ theme === 'dark' ? 'Dark Mode' : 'Light Mode' }}</button>
                    </div>
                  </div>
                </article>

                <!-- Behavior Settings -->
                <article class="panel settings-card full-width">
                  <div class="settings-title"><span><Sliders :size="19" /></span><div><h2>Monitoring Behavior</h2><p>Data polling and formatting preferences.</p></div></div>
                  <div class="settings-row">
                    <div class="setting-item">
                      <label>Poll Interval (ms)</label>
                      <select v-model="userSettings.refreshRate">
                        <option :value="5000">5 seconds (Default)</option>
                        <option :value="10000">10 seconds</option>
                        <option :value="30000">30 seconds</option>
                        <option :value="60000">1 minute</option>
                      </select>
                    </div>
                    <div class="setting-item">
                      <label>Time Format</label>
                      <select v-model="userSettings.timeFormat">
                        <option value="24h">24-hour clock</option>
                        <option value="12h">12-hour clock (AM/PM)</option>
                      </select>
                    </div>
                  </div>
                </article>
                
                <!-- Email Alerts -->
                <article class="panel settings-card">
                  <div class="settings-title"><span><CloudCog :size="19" /></span><div><h2>SMTP Gateway</h2><p>Credentials used to deliver alerts.</p></div></div>
                  <div class="field-grid">
                    <label>SMTP server<input v-model="emailForm.smtp_server" placeholder="smtp.example.com" /></label>
                    <label>Port<input v-model="emailForm.smtp_port" inputmode="numeric" placeholder="587" /></label>
                    <label>Username<input v-model="emailForm.smtp_username" autocomplete="username" placeholder="alerts@example.com" /></label>
                    <label>Password<input v-model="emailForm.smtp_password" type="password" autocomplete="new-password" placeholder="••••••••" /></label>
                  </div>
                </article>
                <article class="panel settings-card">
                  <div class="settings-title"><span><Bell :size="19" /></span><div><h2>Delivery Route</h2><p>Sender and recipient for incidents.</p></div></div>
                  <div class="field-grid one">
                    <label>From address<input v-model="emailForm.from_email" type="email" placeholder="rustping@example.com" /></label>
                    <label>Primary recipient<input v-model="emailForm.to_email" type="email" placeholder="ops@example.com" /></label>
                    <label>Test recipient<input v-model="emailForm.test_email" type="email" placeholder="you@example.com" /></label>
                    <button class="outline-button" @click="saveEmailConfig(true)">Send test alert <ArrowRight :size="14" /></button>
                  </div>
                </article>
              </section>
            </template>
            
            <template v-else-if="route === 'users'">
              <div class="page-title"><div><small>ACCESS CONTROL / {{ appUsers.length.toString().padStart(2,'0') }}</small><h1>Operators</h1><p>Manage authentication and access rights for your team.</p></div><button v-if="currentUser?.permissions?.manage_users" class="signal-button compact" @click="showUserModal = true"><Plus :size="15" /> Add operator</button></div>
              <section class="data-table">
                <div class="table-row table-head"><span>Operator</span><span>Role</span><span style="grid-column: span 2">Permissions</span><span></span></div>
                <div v-for="(user, index) in appUsers" :key="user.username" class="table-row">
                  <span class="device-cell"><i></i><b>{{ user.username }}</b></span>
                  <span>{{ user.role }}</span>
                  <span class="sensor-list" style="grid-column: span 2">
                    <b v-if="user.permissions?.manage_devices">Manage Devices</b>
                    <b v-if="user.permissions?.view_logs">View Logs</b>
                    <b v-if="user.permissions?.manage_settings">Modify Settings</b>
                    <b v-if="user.permissions?.manage_users">Manage Users</b>
                  </span>
                  <span><button v-if="currentUser?.permissions?.manage_users && user.username !== 'admin'" class="icon-button danger-button" @click="deleteUser(index)"><Trash2 :size="15" /></button></span>
                </div>
              </section>
            </template>

          </main>
        </div>
      </div>

      <div v-if="showDeviceModal" class="modal-backdrop" @click.self="showDeviceModal = false"><section class="modal-card" role="dialog" aria-modal="true" aria-labelledby="device-modal-title"><div class="modal-head"><div><small>NEW MONITOR</small><h2 id="device-modal-title">Add a device</h2></div><button aria-label="Close" @click="showDeviceModal = false"><X :size="19" /></button></div><div class="field-grid"><label>Device name<input v-model="deviceForm.name" placeholder="Core gateway" /></label><label>IP or hostname<input v-model="deviceForm.ip" placeholder="10.0.0.1" /></label><label>Category<input v-model="deviceForm.category" placeholder="Network" /></label><label>HTTP URL<input v-model="deviceForm.http_path" placeholder="https://status.example.com" /></label></div><div class="sensor-pick"><small>SENSORS</small><div><button v-for="sensor in ['Ping','Http','Https']" :key="sensor" :class="{active: deviceForm.sensors.includes(sensor)}" @click="toggleSensor(sensor)"><Check v-if="deviceForm.sensors.includes(sensor)" :size="13" />{{ sensor }}</button></div></div><button class="signal-button modal-action" @click="addDevice">Begin monitoring <ArrowRight :size="15" /></button></section></div>
      <div v-if="showUserModal" class="modal-backdrop" @click.self="showUserModal = false"><section class="modal-card" role="dialog" aria-modal="true" aria-labelledby="user-modal-title"><div class="modal-head"><div><small>AUTHENTICATION</small><h2 id="user-modal-title">Create operator</h2></div><button aria-label="Close" @click="showUserModal = false"><X :size="19" /></button></div><div class="field-grid"><label>Username<input v-model="userForm.username" placeholder="Operator ID" autocomplete="new-password" /></label><label>Password<input type="password" v-model="userForm.password" placeholder="••••••••" autocomplete="new-password" /></label></div><div class="field-grid" style="margin-top:20px"><label>Role Template<select v-model="userForm.role" @change="applyUserTemplate"><option value="Admin">Admin (Full Access)</option><option value="Operator">Operator (Standard)</option><option value="Read-Only">Read-Only</option></select></label></div><div class="sensor-pick"><small>ADVANCED PERMISSIONS</small><div style="flex-direction:column;align-items:flex-start;margin-top:15px"><label style="display:flex;align-items:center;gap:10px;margin-bottom:12px;font-size:12px;text-transform:none"><input type="checkbox" v-model="userForm.permissions.manage_devices" style="width:16px;height:16px;margin:0"> Manage network devices</label><label style="display:flex;align-items:center;gap:10px;margin-bottom:12px;font-size:12px;text-transform:none"><input type="checkbox" v-model="userForm.permissions.view_logs" style="width:16px;height:16px;margin:0"> View and export live event stream</label><label style="display:flex;align-items:center;gap:10px;margin-bottom:12px;font-size:12px;text-transform:none"><input type="checkbox" v-model="userForm.permissions.manage_settings" style="width:16px;height:16px;margin:0"> Modify interface and alert settings</label><label style="display:flex;align-items:center;gap:10px;margin-bottom:12px;font-size:12px;text-transform:none"><input type="checkbox" v-model="userForm.permissions.manage_users" style="width:16px;height:16px;margin:0"> Manage operators and access</label></div></div><button class="signal-button modal-action" @click="saveUser">Create operator <ArrowRight :size="15" /></button></section></div>
    </template>
  </div>
</template>
