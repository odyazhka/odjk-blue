#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use eframe::egui;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, LazyLock};
use std::thread;
use std::time::{Duration, Instant};
use std::fs::OpenOptions;
use std::path::PathBuf;
use serde_json;

// ─── App dirs ─────────────────────────────────────────────────────────────────

static APP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push(".local/share/odjk-blue");
    p
});

static PAIRED_FILE: LazyLock<PathBuf> = LazyLock::new(|| APP_DIR.join("paired.json"));
static LOG_FILE:    LazyLock<PathBuf> = LazyLock::new(|| APP_DIR.join("odjk-blue.log"));

// ─── Logger ───────────────────────────────────────────────────────────────────

fn log(msg: &str) {
    eprintln!("{}", msg);
    let _ = std::fs::create_dir_all(&*APP_DIR);
    if let Ok(mut f) = OpenOptions::new()
        .create(true).append(true)
        .open(&*LOG_FILE)
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

// ─── Lang ─────────────────────────────────────────────────────────────────────

struct Lang {
    // Auth screen
    title:              &'static str,
    sudo_prompt:        &'static str,
    sudo_placeholder:   &'static str,
    unlock:             &'static str,
    wrong_sudo:         &'static str,
    // Toolbar
    scanning:           &'static str,
    refresh:            &'static str,
    visibility:         &'static str,
    // Device list / empty state
    no_devices:         &'static str,
    search_progress:    &'static str, // "Поиск устройств… {secs} / 9 сек" — fmt in code
    // Device states
    state_connected:    &'static str,
    state_paired:       &'static str,
    state_available:    &'static str,
    // Device popup
    connected_label:    &'static str,
    paired_label:       &'static str,
    not_paired_label:   &'static str,
    btn_disconnect:     &'static str,
    btn_unpair:         &'static str,
    btn_connect:        &'static str,
    btn_pair:           &'static str,
    close:              &'static str,
    // Pair session window
    win_pairing:        &'static str,
    win_connecting:     &'static str,
    spinner_pairing:    &'static str,
    spinner_connecting: &'static str,
    done_paired:        &'static str,
    done_connected:     &'static str,
    error_label:        &'static str,
    btn_connect_now:    &'static str,
    // Pair session log messages
    log_removing_old:   &'static str,
    log_starting_bctl:  &'static str,
    log_bctl_failed:    &'static str,
    log_pairing:        &'static str, // "Сопрягаем {addr}…" — fmt in code
    log_accept_device:  &'static str,
    log_confirm_key:    &'static str,
    log_trusting:       &'static str,
    log_pause:          &'static str,
    log_connecting:     &'static str, // "Подключаемся к {addr}…" — fmt in code
    log_done_ok:        &'static str,
    log_done_fail:      &'static str, // "❌ Не удалось подключиться: {reason}" — fmt in code
    log_pair_fail:      &'static str, // "❌ Сопряжение не удалось: {reason}" — fmt in code
    log_timeout:        &'static str,
    // System diagnostics
    err_no_bluetoothctl:    &'static str,
    err_no_bluetoothd:      &'static str,
    err_no_dbus:            &'static str,
    err_install_hint:       &'static str, // "Установите: {pkgs}" — fmt in code
    err_bluetoothd_unknown: &'static str,
    btn_dismiss:            &'static str,
}

const RU: Lang = Lang {
    title:              "Bluetooth менеджер",
    sudo_prompt:        "Введите пароль sudo для управления Bluetooth:",
    sudo_placeholder:   "пароль sudo…",
    unlock:             "Разблокировать",
    wrong_sudo:         "Неверный пароль sudo.",
    scanning:           "Сканирование…",
    refresh:            "↻  Обновить",
    visibility:         "Видимость",
    no_devices:         "Устройства не найдены. Нажмите «Обновить».",
    search_progress:    "Поиск устройств… {secs} / 9 сек",
    state_connected:    "подключено",
    state_paired:       "сопряжено",
    state_available:    "не сопряжено",
    connected_label:    "ПОДКЛЮЧЕНО",
    paired_label:       "СОПРЯЖЕНО",
    not_paired_label:   "НЕ СОПРЯЖЕНО",
    btn_disconnect:     "×  Отключить",
    btn_unpair:         "×  Разорвать сопряжение",
    btn_connect:        "⏵  Подключить",
    btn_pair:           "⚡  Сопрягать",
    close:              "Закрыть",
    win_pairing:        "Сопряжение",
    win_connecting:     "Подключение",
    spinner_pairing:    "Подождите…",
    spinner_connecting: "Подключаемся…",
    done_paired:        "Сопряжено!",
    done_connected:     "Подключено!",
    error_label:        "❌ Ошибка",
    btn_connect_now:    "Подключить",
    log_removing_old:   "Снимаем старое сопряжение...",
    log_starting_bctl:  "Запускаем bluetoothctl...",
    log_bctl_failed:    "❌ Не удалось запустить bluetoothctl",
    log_pairing:        "Сопрягаем {}...",
    log_accept_device:  "Примите запрос на устройстве если появится.",
    log_confirm_key:    "Подтверждаем passkey...",
    log_trusting:       "Добавляем в доверенные...",
    log_pause:          "Небольшая пауза перед подключением...",
    log_connecting:     "Подключаемся к {}...",
    log_done_ok:        "✔ Подключено!",
    log_done_fail:      "❌ Не удалось подключиться: {}",
    log_pair_fail:      "❌ Сопряжение не удалось: {}",
    log_timeout:        "таймаут",
    err_no_bluetoothctl:    "bluetoothctl не найден в системе.",
    err_no_bluetoothd:      "bluetoothd не установлен.",
    err_no_dbus:            "D-Bus не найден (dbus-daemon отсутствует).",
    err_install_hint:       "Установите: {}",
    err_bluetoothd_unknown: "bluetoothd не отвечает (неизвестное состояние sv).",
    btn_dismiss:            "Понятно",
};

const EN: Lang = Lang {
    title:              "Bluetooth Manager",
    sudo_prompt:        "Enter your sudo password to manage Bluetooth:",
    sudo_placeholder:   "sudo password…",
    unlock:             "Unlock",
    wrong_sudo:         "Wrong sudo password.",
    scanning:           "Scanning…",
    refresh:            "↻  Refresh",
    visibility:         "Visibility",
    no_devices:         "No devices found. Press «Refresh».",
    search_progress:    "Searching… {secs} / 9 sec",
    state_connected:    "connected",
    state_paired:       "paired",
    state_available:    "not paired",
    connected_label:    "CONNECTED",
    paired_label:       "PAIRED",
    not_paired_label:   "NOT PAIRED",
    btn_disconnect:     "×  Disconnect",
    btn_unpair:         "×  Unpair",
    btn_connect:        "⏵  Connect",
    btn_pair:           "⚡  Pair",
    close:              "Close",
    win_pairing:        "Pairing",
    win_connecting:     "Connecting",
    spinner_pairing:    "Please wait…",
    spinner_connecting: "Connecting…",
    done_paired:        "Paired!",
    done_connected:     "Connected!",
    error_label:        "❌ Error",
    btn_connect_now:    "Connect",
    log_removing_old:   "Removing old pairing...",
    log_starting_bctl:  "Starting bluetoothctl...",
    log_bctl_failed:    "❌ Failed to start bluetoothctl",
    log_pairing:        "Pairing {}...",
    log_accept_device:  "Accept the request on the device if prompted.",
    log_confirm_key:    "Confirming passkey...",
    log_trusting:       "Adding to trusted devices...",
    log_pause:          "Short pause before connecting...",
    log_connecting:     "Connecting to {}...",
    log_done_ok:        "✔ Connected!",
    log_done_fail:      "❌ Failed to connect: {}",
    log_pair_fail:      "❌ Pairing failed: {}",
    log_timeout:        "timeout",
    err_no_bluetoothctl:    "bluetoothctl not found in the system.",
    err_no_bluetoothd:      "bluetoothd is not installed.",
    err_no_dbus:            "D-Bus not found (dbus-daemon missing).",
    err_install_hint:       "Install: {}",
    err_bluetoothd_unknown: "bluetoothd is not responding (unknown sv state).",
    btn_dismiss:            "Dismiss",
};

fn detect_lang() -> &'static Lang {
    for var in &["LANG", "LANGUAGE", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            if val.to_lowercase().starts_with("ru") {
                return &RU;
            }
        }
    }
    &EN
}

// ─── Persistent paired set ───────────────────────────────────────────────────

/// Загружаем сохранённые устройства. Формат: JSON-массив MAC-строк.
fn load_paired() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(&*PAIRED_FILE).unwrap_or_default();
    let macs: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
    macs.into_iter()
        .filter(|mac| mac.len() == 17 && mac.chars().filter(|&c| c == ':').count() == 5)
        .map(|mac| { let name = mac.clone(); (mac, name) })
        .collect()
}

fn save_paired(devices: &[(String, String)]) {
    let _ = std::fs::create_dir_all(&*APP_DIR);
    let macs: Vec<&String> = devices.iter().map(|(mac, _)| mac).collect();
    if let Ok(json) = serde_json::to_string(&macs) {
        let _ = std::fs::write(&*PAIRED_FILE, json);
    }
}

// ─── sudo helpers ─────────────────────────────────────────────────────────────

fn sudo_output(pw: &str, args: &[&str]) -> String {
    let mut child = match Command::new("sudo")
        .arg("-S").args(args)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => { log(&format!("[sudo] spawn error: {e}")); return String::new(); }
    };
    if let Some(mut s) = child.stdin.take() {
        let _ = s.write_all(format!("{}\n", pw).as_bytes());
    }
    child.wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn sudo_run(pw: &str, args: &[&str]) -> bool {
    let mut child = match Command::new("sudo")
        .arg("-S").args(args)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(mut s) = child.stdin.take() {
        let _ = s.write_all(format!("{}\n", pw).as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

// ─── bluetoothd / runit helpers ──────────────────────────────────────────────

const BLUETOOTHD_SVC_DIR: &str = "/var/service/bluetoothd";
const BLUETOOTHD_ETC_DIR: &str = "/etc/sv/bluetoothd";

/// Проверить: симлинк существует И сервис запущен.
/// Возвращает:
///   Ok(true)  — запущен
///   Ok(false) — явно выключен (нет симлинка / есть down-файл / sv показывает "down:")
///   Err(SysError::BluetoothUnknownState) — симлинк есть, down-файла нет, но sv не ответил ни run: ни down:
fn bluetoothd_is_active(pw: &str) -> Result<bool, SysError> {
    if !std::path::Path::new(BLUETOOTHD_SVC_DIR).exists() {
        log("[bluetoothd] symlink absent");
        return Ok(false);
    }
    let down_file = format!("{}/down", BLUETOOTHD_SVC_DIR);
    if std::path::Path::new(&down_file).exists() {
        log("[bluetoothd] down file present, inactive");
        return Ok(false);
    }
    let status = sudo_output(pw, &["sv", "status", "bluetoothd"]);
    // Сервис остановлен вручную (`sv bluetoothd down`) — статус "down:", не ошибка
    if status.contains("down:") {
        log(&format!("[bluetoothd] manually stopped (sv down), treating as inactive: {}", status.trim()));
        return Ok(false);
    }
    if status.contains("run:") {
        log(&format!("[bluetoothd] active ({})", status.trim()));
        return Ok(true);
    }
    // Симлинк есть, down-файла нет, но sv вернул что-то непонятное
    log(&format!("[bluetoothd] unknown sv state: '{}'", status.trim()));
    Err(SysError::BluetoothUnknownState)
}

/// Включить bluetoothd: удалить файл down + sv up.
fn bluetoothd_enable(pw: &str) {
    log("[bluetoothd] enabling...");
    let down_file = format!("{}/down", BLUETOOTHD_SVC_DIR);
    if std::path::Path::new(&down_file).exists() {
        log("[bluetoothd] removing down file");
        sudo_run(pw, &["rm", "-f", &down_file]);
        thread::sleep(Duration::from_millis(200));
    }
    if !std::path::Path::new(BLUETOOTHD_SVC_DIR).exists() {
        log("[bluetoothd] creating symlink");
        sudo_run(pw, &["ln", "-sf", BLUETOOTHD_ETC_DIR, BLUETOOTHD_SVC_DIR]);
        thread::sleep(Duration::from_millis(500));
    }
    sudo_run(pw, &["sv", "up", "bluetoothd"]);
    // Ждём пока сервис поднимется
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(400));
        let s = sudo_output(pw, &["sv", "status", "bluetoothd"]);
        if s.contains("run:") {
            log(&format!("[bluetoothd] up: {}", s.trim()));
            break;
        }
    }
}

/// Выключить bluetoothd: создать файл down + sv down.
fn bluetoothd_disable(pw: &str) {
    log("[bluetoothd] disabling...");
    let down_file = format!("{}/down", BLUETOOTHD_SVC_DIR);
    sudo_run(pw, &["sh", "-c", &format!("touch {}", down_file)]);
    thread::sleep(Duration::from_millis(200));
    sudo_run(pw, &["sv", "down", "bluetoothd"]);
    log("[bluetoothd] disabled, down file created");
}

// ─── System dependency check ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum SysError {
    NoBluetoothctl,
    NoBluetoothd,
    NoDbus,
    BluetoothUnknownState,
}

/// Проверяет наличие dbus-daemon, bluetoothd и bluetoothctl в системе.
/// Возвращает список найденных проблем.
fn check_system_deps() -> Vec<SysError> {
    let mut errors = Vec::new();

    // Проверяем dbus-daemon
    let has_dbus = Command::new("which")
        .arg("dbus-daemon")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        || std::path::Path::new("/run/dbus/system_bus_socket").exists()
        || std::path::Path::new("/var/run/dbus/system_bus_socket").exists();
    if !has_dbus {
        log("[syscheck] dbus-daemon not found");
        errors.push(SysError::NoDbus);
    }

    // Проверяем bluetoothd (бинарник)
    let has_bluetoothd = Command::new("which")
        .arg("bluetoothd")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        || std::path::Path::new("/usr/lib/bluetooth/bluetoothd").exists()
        || std::path::Path::new("/usr/libexec/bluetooth/bluetoothd").exists();
    if !has_bluetoothd {
        log("[syscheck] bluetoothd binary not found");
        errors.push(SysError::NoBluetoothd);
    }

    // Проверяем bluetoothctl
    let has_bluetoothctl = Command::new("which")
        .arg("bluetoothctl")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_bluetoothctl {
        log("[syscheck] bluetoothctl not found");
        errors.push(SysError::NoBluetoothctl);
    }

    log(&format!("[syscheck] done, errors={:?}", errors));
    errors
}

/// Пакеты для установки по типу ошибки (для Void Linux / xbps).
fn install_hint(err: &SysError) -> &'static str {
    match err {
        SysError::NoDbus        => "dbus",
        SysError::NoBluetoothd  => "bluez",
        SysError::NoBluetoothctl => "bluez",
        SysError::BluetoothUnknownState => "",
    }
}


//
// Единственный способ надёжно работать с bluetoothctl без TTY —
// запустить его через sudo -S, дать sudo время сожрать пароль,
// читать stdout/stderr в отдельных тредах ДО отправки команд,
// затем отправлять команды с небольшими паузами.

struct BctlSession {
    child:  std::process::Child,
    stdin:  std::process::ChildStdin,
    output: Arc<Mutex<Vec<String>>>,
    _t_out: thread::JoinHandle<()>,
    _t_err: thread::JoinHandle<()>,
}

impl BctlSession {
    /// Запустить bluetoothctl через sudo -S.
    fn start(pw: &str) -> Option<Self> {
        // Записываем askpass-скрипт во временный файл.
        // sudo -A вызывает его чтобы получить пароль — в stdin bluetoothctl ничего не попадает.
        let askpass = format!("/tmp/odjk-askpass-{}.sh", std::process::id());
        let script  = format!("#!/bin/sh\necho '{}'\n", pw.replace('\'', "'\\''"));
        if std::fs::write(&askpass, &script).is_err() {
            log("[bctl] failed to write askpass script");
            return None;
        }
        let _ = Command::new("chmod").args(&["700", &askpass]).status();

        let mut child = match Command::new("sudo")
            .args(&["-A", "bluetoothctl"])
            .env("SUDO_ASKPASS", &askpass)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => { log(&format!("[bctl] spawn error: {e}")); let _ = std::fs::remove_file(&askpass); return None; }
        };

        let output: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // Читаем stdout и stderr СРАЗУ — до отправки пароля,
        // иначе буфер stderr заблокирует sudo.
        let out2 = Arc::clone(&output);
        let t_out = {
            let stdout = child.stdout.take().unwrap();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().flatten() {
                    let clean = strip_ansi(&line).replace('\r', "");
                    let clean = clean.trim().to_string();
                    if clean.is_empty() { continue; }
                    out2.lock().unwrap().push(clean);
                }
            })
        };

        let err2 = Arc::clone(&output);
        let t_err = {
            let stderr = child.stderr.take().unwrap();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().flatten() {
                    let clean = strip_ansi(&line).replace('\r', "");
                    let clean = clean.trim().to_string();
                    if clean.is_empty() { continue; }
                    err2.lock().unwrap().push(clean);
                }
            })
        };

        let stdin = child.stdin.take().unwrap();

        // Удаляем askpass-скрипт сразу после того как sudo его прочитал
        thread::sleep(Duration::from_millis(300));
        let _ = std::fs::remove_file(&askpass);
        // Даём sudo время exec'нуть bluetoothctl
        thread::sleep(Duration::from_millis(500));

        Some(BctlSession { child, stdin, output, _t_out: t_out, _t_err: t_err })
    }

    fn send(&mut self, cmd: &str) {
        let _ = self.stdin.write_all(format!("{}\n", cmd).as_bytes());
        let _ = self.stdin.flush();
    }

    /// Подождать пока в выводе появится один из паттернов (или истечёт таймаут).
    fn wait_for(&self, patterns: &[&str], timeout_secs: u64) -> Option<String> {
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let mut seen = 0usize;
        loop {
            thread::sleep(Duration::from_millis(200));
            let lines = self.output.lock().unwrap();
            for line in &lines[seen..] {
                for p in patterns {
                    if line.to_lowercase().contains(&p.to_lowercase()) {
                        return Some(line.clone());
                    }
                }
            }
            seen = lines.len();
            if Instant::now() >= deadline { return None; }
        }
    }

    fn snapshot(&self) -> Vec<String> {
        self.output.lock().unwrap().clone()
    }

    fn quit(mut self) {
        let _ = self.stdin.write_all(b"quit\n");
        let _ = self.stdin.flush();
        thread::sleep(Duration::from_millis(300));
        let _ = self.child.wait();
    }
}

// ─── bt state ────────────────────────────────────────────────────────────────

/// Проверить включён ли адаптер через bluetoothctl show.
fn bt_get_state(pw: &str) -> (bool, bool) {
    let out = sudo_output(pw, &["bluetoothctl", "show"]);
    let powered      = out.contains("Powered: yes");
    let discoverable = out.contains("Discoverable: yes");
    log(&format!("[state] powered={} discoverable={}", powered, discoverable));
    (powered, discoverable)
}

// ─── discoverable ────────────────────────────────────────────────────────────

fn bt_set_discoverable(pw: &str, on: bool) {
    log(&format!("[bt_disc] on={}", on));
    let val = if on { "on" } else { "off" };
    sudo_run(pw, &["bluetoothctl", "discoverable", val]);
}

// ─── disconnect / unpair ─────────────────────────────────────────────────────

fn bt_disconnect(pw: &str, addr: &str) {
    log(&format!("[bt_disconnect] {}", addr));
    sudo_run(pw, &["bluetoothctl", "disconnect", addr]);
}

fn bt_unpair(pw: &str, addr: &str) {
    log(&format!("[bt_unpair] {}", addr));
    sudo_run(pw, &["bluetoothctl", "remove", addr]);
    // Также чистим файловую систему (bluetoothd иногда не удаляет)
    let addr_up = addr.to_uppercase();
    sudo_run(pw, &["sh", "-c", &format!("rm -rf /var/lib/bluetooth/*/{}", addr_up)]);
    log("[bt_unpair] done");
}

// ─── Device state ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum DevState {
    Connected,
    Paired,
    Available,
}

#[derive(Clone, Debug)]
struct BtDevice {
    address: String,
    name:    String,
    state:   DevState,
}



// ─── Scan ────────────────────────────────────────────────────────────────────

/// Получить список сопряжённых и подключённых через bluetoothctl.
fn get_known_addrs(pw: &str) -> (Vec<String>, Vec<String>) {
    fn extract_macs(text: &str) -> Vec<String> {
        let mut macs = Vec::new();
        for line in text.lines() {
            let upper = line.to_uppercase();
            for w in upper.split_whitespace() {
                if w.len() == 17 && w.chars().filter(|&c| c == ':').count() == 5 {
                    if !macs.contains(&w.to_string()) { macs.push(w.to_string()); }
                }
            }
        }
        macs
    }

    // bluetoothctl devices Connected
    let con_out = sudo_output(pw, &["bluetoothctl", "devices", "Connected"]);
    let connected = extract_macs(&con_out);

    let paired_out = sudo_output(pw, &["bluetoothctl", "devices", "Paired"]);
    let mut paired = extract_macs(&paired_out);

    // Filesystem fallback
    let ls_out = sudo_output(pw, &["sh", "-c",
        "find /var/lib/bluetooth -mindepth 2 -maxdepth 2 -type d 2>/dev/null"
    ]);
    for mac in extract_macs(&ls_out) {
        if !paired.contains(&mac) { paired.push(mac); }
    }

    for mac in &connected {
        if !paired.contains(mac) { paired.push(mac.clone()); }
    }

    (connected, paired)
}

fn bt_scan(pw: &str, known_paired: &[(String, String)]) -> Vec<BtDevice> {
    log("[bt_scan] starting scan via bluetoothctl...");
    let (connected, mut paired) = get_known_addrs(pw);
    for (mac, _) in known_paired {
        let up = mac.to_uppercase();
        if !paired.contains(&up) { paired.push(up); }
    }

    // Запускаем сессию bluetoothctl для скана
    let mut sess = match BctlSession::start(pw) {
        Some(s) => s,
        None => {
            log("[bt_scan] failed to start bluetoothctl");
            return vec![];
        }
    };

    sess.send("scan on");
    thread::sleep(Duration::from_secs(9));
    sess.send("scan off");
    thread::sleep(Duration::from_secs(2));
    let raw = sess.snapshot();
    sess.quit();

    let mut devs = parse_scan_bctl(&raw, &connected, &paired);

    // Заполняем пустые имена для устройств добавленных из CHG (без [NEW])
    for dev in devs.iter_mut() {
        if dev.name.is_empty() {
            let info = sudo_output(pw, &["bluetoothctl", "info", &dev.address]);
            dev.name = info.lines()
                .find(|l| l.trim_start().starts_with("Name:"))
                .map(|l| l.trim_start().trim_start_matches("Name:").trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| dev.address.clone());
        }
    }

    devs.sort_by(|a, b| {
        let rank = |s: &DevState| match s {
            DevState::Connected => 0,
            DevState::Paired    => 1,
            DevState::Available => 2,
        };
        rank(&a.state).cmp(&rank(&b.state))
    });

    devs
}

/// Парсим вывод bluetoothctl scan.
/// Строки приходят с префиксом "[bluetoothctl]> " и без ведущей "[":
///   [bluetoothctl]> NEW] Device AA:BB:CC:DD:EE:FF Name
///   [bluetoothctl]> CHG] Device AA:BB:CC:DD:EE:FF RSSI: 0xffffffaf (-81)
fn parse_scan_bctl(lines: &[String], connected: &[String], paired: &[String]) -> Vec<BtDevice> {
    let mut devs: Vec<BtDevice> = Vec::new();

    fn is_mac(s: &str) -> bool {
        s.len() == 17 && s.chars().filter(|&c| c == ':').count() == 5
            && s.chars().all(|c| c == ':' || c.is_ascii_hexdigit())
    }

    // Убираем префикс "[bluetoothctl]> " и восстанавливаем "[" если нужно
    let clean_lines: Vec<String> = lines.iter().map(|l| {
        let s = l.replace('\r', "");
        let s = s.trim().to_string();
        let s = if let Some(pos) = s.find("> ") { s[pos + 2..].to_string() } else { s };
        let s = s.trim().to_string();
        if s.starts_with("NEW]") || s.starts_with("CHG]") || s.starts_with("DEL]") {
            format!("[{}", s)
        } else {
            s
        }
    }).collect();

    for line in &clean_lines {
        let line = line.trim();

        // [NEW] Device MAC Name
        if line.contains("[NEW] Device") {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let Some(idx) = words.iter().position(|w| is_mac(w)) {
                let addr = words[idx].to_uppercase();
                let name = if idx + 1 < words.len() {
                    words[idx + 1..].join(" ")
                } else {
                    addr.clone()
                };
                let name = if name.chars().filter(|&c| c == '-').count() == 5 {
                    addr.clone()
                } else if name.is_empty() {
                    addr.clone()
                } else {
                    name
                };

                let is_conn   = connected.iter().any(|c| c.eq_ignore_ascii_case(&addr));
                let is_paired = paired.iter().any(|c| c.eq_ignore_ascii_case(&addr));
                let state = if is_conn { DevState::Connected }
                            else if is_paired { DevState::Paired }
                            else { DevState::Available };

                if let Some(ex) = devs.iter_mut().find(|d| d.address == addr) {
                    if ex.name == ex.address && name != addr { ex.name = name; }
                    if is_conn || (is_paired && ex.state == DevState::Available) { ex.state = state; }
                } else {
                    devs.push(BtDevice { address: addr, name, state });
                }
            }
        }

        // [CHG] Device MAC RSSI — игнорируем
    }

    devs
}

// ─── ANSI strip ───────────────────────────────────────────────────────────────

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&nc) = chars.peek() { chars.next(); if nc == 'm' { break; } }
        } else {
            out.push(c);
        }
    }
    out
}

// ─── Pair / Connect / Reconnect ──────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum PairStatus { Running, Done(bool) }

/// Цель сессии сопряжения/подключения.
#[derive(Clone, Debug, PartialEq)]
enum PairGoal {
    /// Устройство незнакомо: pair → trust → connect.
    PairFresh,
    /// Устройство уже сопряжено: только connect.
    Reconnect,
    /// Принудительное перепаривание: remove → pair → trust → connect.
    RepairForce,
}

struct PairSession {
    address: String,
    name:    String,
    goal:    PairGoal,
    log:     Arc<Mutex<Vec<String>>>,
    status:  Arc<Mutex<PairStatus>>,
    #[allow(dead_code)]
    scan:    Arc<Mutex<ScanState>>,
}

fn start_pair_session(pw: &str, addr: &str, name: &str, goal: PairGoal, scan_arc: Arc<Mutex<ScanState>>, lang: &'static Lang) -> PairSession {
    let session_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let status:      Arc<Mutex<PairStatus>>  = Arc::new(Mutex::new(PairStatus::Running));

    let slog2   = Arc::clone(&session_log);
    let status2 = Arc::clone(&status);
    let pw2     = pw.to_string();
    let addr2   = addr.to_string();
    let name2   = name.to_string();
    let scan2   = Arc::clone(&scan_arc);
    let goal2   = goal.clone();
    let lang2   = lang;

    thread::spawn(move || {
        macro_rules! push {
            ($msg:expr) => {{
                let m: String = $msg.to_string();
                log(&format!("[pair_session] {}", m));
                slog2.lock().unwrap().push(m);
            }};
        }

        let success = match goal2 {
            PairGoal::RepairForce => {
                push!(lang2.log_removing_old);
                bt_unpair(&pw2, &addr2);
                thread::sleep(Duration::from_millis(500));
                do_pair_and_connect_bctl(&pw2, &addr2, false, &slog2, lang2)
            }
            PairGoal::PairFresh => {
                do_pair_and_connect_bctl(&pw2, &addr2, false, &slog2, lang2)
            }
            PairGoal::Reconnect => {
                do_pair_and_connect_bctl(&pw2, &addr2, true, &slog2, lang2)
            }
        };

        // Проверяем реальное состояние
        let con_out = sudo_output(&pw2, &["bluetoothctl", "devices", "Connected"]);
        let is_now_connected = con_out.to_uppercase().contains(&addr2.to_uppercase());
        log(&format!("[pair_session] post-done connected={}", is_now_connected));

        *status2.lock().unwrap() = PairStatus::Done(success || is_now_connected);

        // Обновляем список устройств
        {
            let mut s = scan2.lock().unwrap();
            if success || is_now_connected {
                let up = addr2.to_uppercase();
                if !s.known_paired.iter().any(|(m, _)| m.eq_ignore_ascii_case(&up)) {
                    s.known_paired.push((up, name2.clone()));
                    save_paired(&s.known_paired);
                }
            }
            for dev in s.devices.iter_mut() {
                if dev.address.eq_ignore_ascii_case(&addr2) {
                    if is_now_connected { dev.state = DevState::Connected; }
                    else if success     { dev.state = DevState::Paired; }
                    if dev.name == dev.address && !name2.is_empty() && name2 != addr2 {
                        dev.name = name2.clone();
                    }
                }
            }
        }
    });

    PairSession { address: addr.to_string(), name: name.to_string(), goal, log: session_log, status, scan: scan_arc }
}

/// Единый этап: pair → trust → connect в одном сеансе bluetoothctl.
/// `only_connect` — если устройство уже сопряжено, пропускаем pair/trust.
fn do_pair_and_connect_bctl(pw: &str, addr: &str, only_connect: bool, slog: &Arc<Mutex<Vec<String>>>, l: &'static Lang) -> bool {
    macro_rules! push {
        ($msg:expr) => {{
            let m: String = $msg.to_string();
            log(&format!("[bctl] {}", m));
            slog.lock().unwrap().push(m);
        }};
    }
    macro_rules! push_cmd {
        ($cmd:expr) => {{
            let m = format!("> {}", $cmd);
            slog.lock().unwrap().push(m);
        }};
    }
    macro_rules! push_out {
        ($line:expr) => {{
            let m: String = $line.to_string();
            slog.lock().unwrap().push(m);
        }};
    }

    push!(l.log_starting_bctl);
    let mut sess = match BctlSession::start(pw) {
        Some(s) => s,
        None => { push!(l.log_bctl_failed); return false; }
    };

    // Запускаем агента и скан, но НЕ ждём — pair отправим сразу,
    // bluetoothd уже видит устройство из предыдущего скана UI.
    push_cmd!("agent on");
    sess.send("agent on");
    push_cmd!("default-agent");
    sess.send("default-agent");
    push_cmd!("scan on");
    sess.send("scan on");
    // Короткая пауза чтобы агент зарегистрировался
    thread::sleep(Duration::from_millis(800));

    let mut pair_ok = only_connect; // если только connect — считаем pair уже сделан

    if !only_connect {
        push!(format!("{}", l.log_pairing.replace("{}", addr)));
        push!(l.log_accept_device);

        let pair_cmd = format!("pair {}", addr);
        push_cmd!(&pair_cmd);
        sess.send(&pair_cmd);

        // iPhone показывает код и ждёт подтверждения с нашей стороны тоже.
        // bluetoothctl пишет "[agent] Confirm passkey XXXXXX (yes/no):" — надо ответить yes.
        if sess.wait_for(&["confirm passkey"], 30).is_some() {
            push!(l.log_confirm_key);
            sess.send("yes");
        }

        // Теперь ждём финального результата
        let result = sess.wait_for(
            &["pairing successful", "failed to pair", "not available",
              "authentication failed", "already paired", "paired: yes",
              "request confirmed", "connected"],
            30,
        );
        if let Some(ref line) = result {
            push_out!(line);
        }

        // Останавливаемся только при явном отказе
        let snap = sess.snapshot().join("\n").to_lowercase();
        let explicitly_failed = result.as_ref().map(|r| {
            let r = r.to_lowercase();
            r.contains("failed to pair") || r.contains("authentication failed") || r.contains("not available")
        }).unwrap_or(false);

        if explicitly_failed && !snap.contains("pairing successful") && !snap.contains("already paired") {
            let reason = result.unwrap_or_else(|| l.log_timeout.into());
            push!(format!("{}", l.log_pair_fail.replace("{}", &reason)));
            sess.quit();
            return false;
        }

        pair_ok = true;

        // Trust — всегда после pair, независимо от формулировки ответа
        push!(l.log_trusting);
        let trust_cmd = format!("trust {}", addr);
        push_cmd!(&trust_cmd);
        sess.send(&trust_cmd);
        let trust_result = sess.wait_for(&["trusted: yes", "trust succeeded", "changing trusted succeeded"], 10);
        if let Some(ref line) = trust_result {
            push_out!(line);
        }

        // Пауза: iPhone нужно время зарегистрировать пару перед connect
        push!(l.log_pause);
        thread::sleep(Duration::from_secs(2));
    }

    // Подключение
    push!(format!("{}", l.log_connecting.replace("{}", addr)));
    let conn_cmd = format!("connect {}", addr);
    push_cmd!(&conn_cmd);
    sess.send(&conn_cmd);

    let conn_result = sess.wait_for(
        &["connection successful", "failed to connect", "not available",
          "already connected", "connected: yes", "error"],
        25,
    );

    if let Some(ref line) = conn_result {
        push_out!(line);
    }

    // Также проверяем весь снапшот — iPhone мог подключиться ещё во время trust
    let conn_snap = sess.snapshot().join("\n").to_lowercase();
    let connected = conn_result.as_ref().map(|r| {
        let r = r.to_lowercase();
        r.contains("connection successful") || r.contains("already connected") || r.contains("connected: yes")
    }).unwrap_or(false)
    || conn_snap.contains("connection successful")
    || conn_snap.contains("connected: yes");

    if connected {
        push!(l.log_done_ok);
    } else {
        let reason = conn_result.unwrap_or_else(|| l.log_timeout.into());
        push!(format!("{}", l.log_done_fail.replace("{}", &reason)));
    }

    push_cmd!("quit");
    sess.quit();
    pair_ok && connected
}

// ─── App state ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct ScanState {
    devices:      Vec<BtDevice>,
    scanning:     bool,
    /// Сопряжённые устройства (MAC, Имя)
    known_paired: Vec<(String, String)>,
}

enum Screen { SudoAuth, Main }

struct BtApp {
    lang:            &'static Lang,
    screen:          Screen,
    sudo_password:   String,
    sudo_error:      Option<String>,
    auth_pending:    bool,
    bt_power:        bool,
    bt_disc:         bool,
    scan:            Arc<Mutex<ScanState>>,
    last_scan:       Option<Instant>,
    scan_started:    Option<Instant>,
    pair_session:    Option<PairSession>,
    selected_device: Option<String>,
    /// Ошибки отсутствия системных зависимостей (обнаруживаются при старте)
    sys_errors:      Vec<SysError>,
    /// Непонятное состояние bluetoothd (показывается как предупреждение в toolbar)
    dbus_error:      Option<String>,
    /// Канал для передачи dbus_error из фонового треда
    dbus_error_arc:  Arc<Mutex<Option<String>>>,
}

impl BtApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        log("=== odjk-blue started ===");
        let known_paired = load_paired();
        let sys_errors = check_system_deps();
        Self {
            lang:          detect_lang(),
            screen:        Screen::SudoAuth,
            sudo_password: String::new(),
            sudo_error:    None,
            auth_pending:  false,
            bt_power:      false,
            bt_disc:       false,
            scan:          Arc::new(Mutex::new(ScanState { known_paired, ..Default::default() })),
            last_scan:     None,
            scan_started:  None,
            pair_session:    None,
            selected_device: None,
            sys_errors,
            dbus_error:    None,
            dbus_error_arc: Arc::new(Mutex::new(None)),
        }
    }

    fn trigger_scan(&mut self) {
        // Не сканируем если BT выключен пользователем
        if !self.bt_power {
            log("[trigger_scan] skipped — bt_power is off");
            return;
        }
        {
            let mut s = self.scan.lock().unwrap();
            if s.scanning { return; }
            s.scanning = true;
            // Don't clear devices here — show old list while scanning
        }
        self.scan_started = Some(Instant::now());
        log("[trigger_scan] starting scan thread");
        let arc = Arc::clone(&self.scan);
        let pw  = self.sudo_password.clone();
        let known_paired = arc.lock().unwrap().known_paired.clone();
        let dbus_err_arc = Arc::clone(&self.dbus_error_arc);
        thread::spawn(move || {
            match bluetoothd_is_active(&pw) {
                Ok(true) => { /* всё хорошо, продолжаем */ }
                Ok(false) => {
                    log("[trigger_scan] bluetoothd not active, skipping scan");
                    arc.lock().unwrap().scanning = false;
                    return;
                }
                Err(SysError::BluetoothUnknownState) => {
                    log("[trigger_scan] bluetoothd unknown state, skipping scan");
                    *dbus_err_arc.lock().unwrap() = Some("BluetoothUnknownState".to_string());
                    arc.lock().unwrap().scanning = false;
                    return;
                }
                Err(_) => {
                    arc.lock().unwrap().scanning = false;
                    return;
                }
            }
            let devices = bt_scan(&pw, &known_paired);
            let mut s = arc.lock().unwrap();
            s.devices  = devices;
            s.scanning = false;
        });
    }

    /// Spawn a background task that disconnects — updates state optimistically.
    fn do_disconnect(&mut self, addr: String) {
        let pw  = self.sudo_password.clone();
        let arc = Arc::clone(&self.scan);
        // Оптимистично меняем статус на Paired сразу
        {
            let mut s = arc.lock().unwrap();
            for dev in s.devices.iter_mut() {
                if dev.address.eq_ignore_ascii_case(&addr) {
                    dev.state = DevState::Paired;
                }
            }
        }
        thread::spawn(move || {
            bt_disconnect(&pw, &addr);
        });
    }

    /// Spawn a background task that unpairs (and disconnects) then refreshes.
    fn do_unpair(&mut self, addr: String) {
        let pw  = self.sudo_password.clone();
        let arc = Arc::clone(&self.scan);
        // Оптимистично убираем из известных спаренных и из списка устройств
        {
            let mut s = arc.lock().unwrap();
            let addr_up = addr.to_uppercase();
            s.known_paired.retain(|(m, _)| !m.eq_ignore_ascii_case(&addr_up));
            save_paired(&s.known_paired);
            s.devices.retain(|d| !d.address.eq_ignore_ascii_case(&addr_up));
        }
        thread::spawn(move || {
            bt_disconnect(&pw, &addr);
            thread::sleep(Duration::from_millis(400));
            bt_unpair(&pw, &addr);
            // Не запускаем скан — список уже обновлён оптимистично
            // Просто убедимся что scanning не застрял
            arc.lock().unwrap().scanning = false;
        });
    }

}

impl eframe::App for BtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(300));
        let l = self.lang;

        // ── Опрашиваем фоновый тред на предмет dbus_error ────────────────────
        {
            let mut guard = self.dbus_error_arc.lock().unwrap();
            if let Some(ref e) = guard.take() {
                self.dbus_error = Some(e.clone());
            }
        }

        // ── Модальные окна системных ошибок (показываются поверх всего) ──────
        if !self.sys_errors.is_empty() {
            egui::Window::new("⚠  System")
                .collapsible(false).resizable(false)
                .default_width(360.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    for err in &self.sys_errors {
                        let msg = match err {
                            SysError::NoBluetoothctl    => l.err_no_bluetoothctl,
                            SysError::NoBluetoothd      => l.err_no_bluetoothd,
                            SysError::NoDbus            => l.err_no_dbus,
                            SysError::BluetoothUnknownState => l.err_bluetoothd_unknown,
                        };
                        ui.colored_label(egui::Color32::from_rgb(220, 90, 60), msg);
                        let pkg = install_hint(err);
                        if !pkg.is_empty() {
                            ui.label(
                                egui::RichText::new(l.err_install_hint.replace("{}", pkg))
                                    .monospace().color(egui::Color32::from_rgb(180, 180, 100))
                            );
                        }
                        ui.add_space(4.0);
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);
                    if ui.button(l.btn_dismiss).clicked() {
                        self.sys_errors.clear();
                    }
                });
        }

        // ── Sudo auth screen ──────────────────────────────────────────────────
        if matches!(self.screen, Screen::SudoAuth) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(80.0);
                ui.vertical_centered(|ui| {
                    ui.heading(egui::RichText::new(l.title).size(22.0));
                    ui.add_space(24.0);
                    ui.label(l.sudo_prompt);
                    ui.add_space(8.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.sudo_password)
                            .password(true).hint_text(l.sudo_placeholder).desired_width(260.0),
                    );
                    resp.request_focus();
                    if let Some(ref err) = self.sudo_error {
                        ui.add_space(6.0);
                        ui.colored_label(egui::Color32::from_rgb(220, 60, 60), err.as_str());
                    }
                    ui.add_space(14.0);
                    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button(l.unlock).clicked() || enter) && !self.auth_pending {
                        self.auth_pending = true;
                        if sudo_run(&self.sudo_password, &["true"]) {
                            log("[auth] ok");
                            // bt_power = bluetoothd запущен
                            match bluetoothd_is_active(&self.sudo_password) {
                                Ok(active) => {
                                    self.bt_power = active;
                                    self.dbus_error = None;
                                }
                                Err(SysError::BluetoothUnknownState) => {
                                    self.bt_power = false;
                                    self.dbus_error = Some("BluetoothUnknownState".to_string());
                                }
                                Err(_) => {
                                    self.bt_power = false;
                                }
                            }
                            self.bt_disc  = if self.bt_power {
                                let (_, disc) = bt_get_state(&self.sudo_password);
                                disc
                            } else { false };
                            self.screen     = Screen::Main;
                            self.sudo_error = None;
                            self.auth_pending = false;
                            if self.bt_power {
                                self.last_scan = Some(Instant::now());
                                self.trigger_scan();
                            }
                        } else {
                            log("[auth] failed");
                            self.auth_pending = false;
                            self.sudo_error   = Some(l.wrong_sudo.into());
                            self.sudo_password.clear();
                        }
                    }
                });
            });
            return;
        }

        // ── Toolbar ───────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar")
            .frame(egui::Frame::default().inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 4.0, bottom: 0.0 }).fill(egui::Color32::from_rgb(30, 30, 35)))
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.heading(egui::RichText::new("Bluetooth").size(18.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    let scanning = self.scan.lock().unwrap().scanning;
                    if scanning {
                        ui.spinner();
                        ui.label(egui::RichText::new(l.scanning).color(egui::Color32::GRAY));
                    } else if ui.button(l.refresh).clicked() {
                        self.last_scan = Some(Instant::now());
                        self.trigger_scan();
                    }
                });
            });
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                // Чекбокс Bluetooth — надпись всегда обычного цвета
                let mut bt_on = self.bt_power;
                if ui.checkbox(&mut bt_on, "bluetoothd").changed() {
                    self.bt_power = bt_on;
                    let pw = self.sudo_password.clone();
                    if self.bt_power {
                        thread::spawn(move || { bluetoothd_enable(&pw); });
                        self.last_scan = Some(Instant::now());
                        self.trigger_scan();
                    } else {
                        self.bt_disc = false;
                        let pw2 = self.sudo_password.clone();
                        thread::spawn(move || { bluetoothd_disable(&pw2); });
                        self.scan.lock().unwrap().devices.clear();
                    }
                }

                ui.add_space(12.0);

                // Чекбокс Видимость — только когда bluetoothd запущен
                if self.bt_power {
                    let mut disc_on = self.bt_disc;
                    if ui.checkbox(&mut disc_on, l.visibility).changed() {
                        self.bt_disc = disc_on;
                        let pw = self.sudo_password.clone();
                        thread::spawn(move || { bt_set_discoverable(&pw, disc_on); });
                    }
                    ui.add_space(12.0);
                }
            });

            // Полоса предупреждения о неизвестном состоянии bluetoothd
            if self.dbus_error.is_some() {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 160, 40),
                        egui::RichText::new(l.err_bluetoothd_unknown).size(11.5),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        if ui.small_button("✕").clicked() {
                            self.dbus_error = None;
                        }
                    });
                });
                ui.add_space(2.0);
            }

            ui.add_space(4.0);

        }); // end toolbar

        // ── Central panel ─────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {

        // ── Pair session window ───────────────────────────────────────────
            let mut close_pair = false;
            let mut connect_after_pair: Option<(String, String)> = None;
            let mut unpair_after_pair:  Option<String>            = None;
            if let Some(ref session) = self.pair_session {
                let plog   = session.log.lock().unwrap().clone();
                let status = session.status.lock().unwrap().clone();
                let mut open = true;

                egui::Window::new(format!("{}: {}", if session.goal == PairGoal::Reconnect { l.win_connecting } else { l.win_pairing }, session.name))
                    .collapsible(false).resizable(false)
                    .default_width(360.0)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .open(&mut open)
                    .show(ctx, |ui| {
                        match &status {
                            PairStatus::Running => {
                                ui.label(egui::RichText::new(&session.address)
                                    .monospace().size(11.0).color(egui::Color32::GRAY));
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    let msg = if session.goal == PairGoal::Reconnect {
                                        l.spinner_connecting
                                    } else {
                                        l.spinner_pairing
                                    };
                                    ui.label(egui::RichText::new(msg)
                                        .color(egui::Color32::from_rgb(220,200,60)));
                                });
                                ui.add_space(6.0);
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        for line in &plog {
                                            let color = if line.starts_with("✔") {
                                                egui::Color32::from_rgb(80,210,80)
                                            } else if line.starts_with("❌") || line.to_lowercase().contains("fail") {
                                                egui::Color32::from_rgb(220,100,100)
                                            } else {
                                                egui::Color32::GRAY
                                            };
                                            ui.label(egui::RichText::new(line).size(11.0).monospace().color(color));
                                        }
                                    });
                            }
                            PairStatus::Done(true) => {
                                let addr_done = session.address.clone();
                                let name_done = session.name.clone();
                                ui.vertical_centered(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(egui::RichText::new("✔")
                                        .size(56.0).color(egui::Color32::from_rgb(80,210,80)));
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new(
                                        if session.goal == PairGoal::Reconnect { l.done_connected } else { l.done_paired }
                                    ).size(22.0).strong().color(egui::Color32::from_rgb(80,210,80)));
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(&name_done)
                                        .size(15.0).color(egui::Color32::WHITE));
                                    ui.add_space(20.0);
                                    ui.separator();
                                    ui.add_space(12.0);

                                    if session.goal != PairGoal::Reconnect {
                                        let btn_conn = egui::Button::new(
                                            egui::RichText::new(l.btn_connect_now).size(14.0).color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(50, 120, 220))
                                        .min_size(egui::vec2(220.0, 38.0));
                                        if ui.add(btn_conn).clicked() {
                                            close_pair = true;
                                            connect_after_pair = Some((addr_done.clone(), name_done.clone()));
                                        }
                                        ui.add_space(8.0);
                                    }

                                    let btn_un = egui::Button::new(
                                        egui::RichText::new(l.btn_unpair).size(13.0).color(egui::Color32::WHITE)
                                    )
                                    .fill(egui::Color32::from_rgb(160, 50, 50))
                                    .min_size(egui::vec2(220.0, 34.0));
                                    if ui.add(btn_un).clicked() {
                                        close_pair = true;
                                        unpair_after_pair = Some(addr_done.clone());
                                    }

                                    ui.add_space(8.0);
                                    if ui.button(l.close).clicked() { close_pair = true; }
                                    ui.add_space(16.0);
                                });
                            }
                            PairStatus::Done(false) => {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(12.0);
                                    ui.label(egui::RichText::new(l.error_label)
                                        .size(18.0).color(egui::Color32::from_rgb(220,60,60)));
                                    ui.add_space(4.0);
                                });
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(180.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        for line in &plog {
                                            let color = if line.starts_with("✔") {
                                                egui::Color32::from_rgb(80,210,80)
                                            } else if line.starts_with("❌") || line.to_lowercase().contains("fail") {
                                                egui::Color32::from_rgb(220,100,100)
                                            } else {
                                                egui::Color32::GRAY
                                            };
                                            ui.label(egui::RichText::new(line).size(11.0).monospace().color(color));
                                        }
                                    });
                                ui.add_space(8.0);
                                ui.vertical_centered(|ui| {
                                    if ui.button(l.close).clicked() { close_pair = true; }
                                });
                            }
                        }
                    });

                if !open { close_pair = true; }
            }
            if close_pair {
                self.pair_session    = None;
                self.selected_device = None;
            }
            if let Some((addr, name)) = connect_after_pair {
                log(&format!("[ui] connect after pair for {}", addr));
                self.pair_session = Some(start_pair_session(
                    &self.sudo_password, &addr, &name, PairGoal::Reconnect, Arc::clone(&self.scan), l,
                ));
            }
            if let Some(addr) = unpair_after_pair {
                self.do_unpair(addr);
            }

            // ── Device list ───────────────────────────────────────────────────
            let (devices, scanning) = {
                let s = self.scan.lock().unwrap();
                (s.devices.clone(), s.scanning)
            };
            let ctx2 = ctx.clone();

            if devices.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    if scanning {
                        let elapsed = self.scan_started
                            .map(|t| t.elapsed().as_secs_f32())
                            .unwrap_or(0.0);
                        let progress = (elapsed / 9.0).min(1.0);
                        ui.label(egui::RichText::new(
                            format!("{}", l.search_progress.replace("{secs}", &format!("{:.0}", elapsed.min(9.0))))
                        ).color(egui::Color32::GRAY));
                        ui.add_space(8.0);
                        ui.add(egui::ProgressBar::new(progress).desired_width(260.0).animate(false));
                    } else {
                        ui.label(egui::RichText::new(l.no_devices).color(egui::Color32::GRAY));
                    }
                });
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                let mut action: Option<(String, String, &str)> = None;

                for dev in &devices {
                    let (state_text, state_color) = match dev.state {
                        DevState::Connected => (l.state_connected,  egui::Color32::from_rgb(80, 210, 80)),
                        DevState::Paired    => (l.state_paired,     egui::Color32::from_rgb(100, 160, 255)),
                        DevState::Available => (l.state_available,  egui::Color32::from_rgb(140, 140, 140)),
                    };

                    let row_top = ui.cursor().top();
                    let horiz_resp = ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let has_name = !dev.name.eq_ignore_ascii_case(&dev.address);
                        let display_name = if has_name { dev.name.as_str() } else { dev.address.as_str() };
                        let display_mac  = if has_name { dev.address.as_str() } else { " " };
                        let name_rt = egui::RichText::new(display_name).size(13.0).strong();
                        ui.label(name_rt);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(state_text).color(state_color).size(11.0));
                            ui.add_space(14.0);
                            ui.label(egui::RichText::new(display_mac)
                                .color(egui::Color32::from_rgb(120, 120, 120)).size(10.0).monospace());
                        });
                    });
                    // Точный rect только этой строки
                    let row_rect = {
                        let mut r = horiz_resp.response.rect;
                        r.set_top(row_top);
                        r
                    };

                    // Click on row → select device
                    let row_id = egui::Id::new(("dev_row", &dev.address));
                    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
                    if row_resp.clicked() && self.pair_session.is_none() {
                        if self.selected_device.as_deref() == Some(dev.address.as_str()) {
                            self.selected_device = None;
                        } else {
                            self.selected_device = Some(dev.address.clone());
                        }
                    }

                    // Popup for this device
                    if self.selected_device.as_deref() == Some(dev.address.as_str()) {
                        egui::Window::new(format!("{}", dev.name))
                            .collapsible(false).resizable(false)
                            .default_width(280.0)
                            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                            .show(&ctx2, |ui| {
                                ui.label(egui::RichText::new(&dev.address)
                                    .monospace().size(10.0).color(egui::Color32::GRAY));
                                ui.add_space(6.0);

                                match dev.state {
                                    DevState::Connected => {
                                        // ── Заголовок секции ──────────────────
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("✔")
                                                .color(egui::Color32::from_rgb(80, 210, 80)).size(14.0));
                                            ui.label(egui::RichText::new(l.connected_label)
                                                .color(egui::Color32::from_rgb(80, 210, 80))
                                                .size(13.0).strong());
                                        });
                                        ui.add_space(10.0);
                                        ui.separator();
                                        ui.add_space(8.0);

                                        ui.add_space(6.0);
                                        let btn_disconnect = egui::Button::new(
                                            egui::RichText::new(l.btn_disconnect)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(190, 50, 50))
                                        .min_size(egui::vec2(240.0, 32.0));
                                        if ui.add(btn_disconnect).clicked() {
                                            action = Some((dev.address.clone(), dev.name.clone(), "disconnect"));
                                        }

                                        ui.add_space(6.0);

                                        let btn_unpair = egui::Button::new(
                                            egui::RichText::new(l.btn_unpair)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(190, 50, 50))
                                        .min_size(egui::vec2(240.0, 32.0));
                                        if ui.add(btn_unpair).clicked() {
                                            action = Some((dev.address.clone(), dev.name.clone(), "unpair"));
                                        }
                                    }

                                    DevState::Paired => {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("✔")
                                                .color(egui::Color32::from_rgb(100, 160, 255)).size(14.0));
                                            ui.label(egui::RichText::new(l.paired_label)
                                                .color(egui::Color32::from_rgb(100, 160, 255))
                                                .size(13.0).strong());
                                        });
                                        ui.add_space(10.0);
                                        ui.separator();
                                        ui.add_space(8.0);

                                        let btn_connect = egui::Button::new(
                                            egui::RichText::new(l.btn_connect)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(50, 120, 220))
                                        .min_size(egui::vec2(240.0, 32.0));
                                        if ui.add(btn_connect).clicked() {
                                            action = Some((dev.address.clone(), dev.name.clone(), "connect"));
                                        }

                                        ui.add_space(6.0);

                                        let btn_unpair = egui::Button::new(
                                            egui::RichText::new(l.btn_unpair)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(190, 50, 50))
                                        .min_size(egui::vec2(240.0, 32.0));
                                        if ui.add(btn_unpair).clicked() {
                                            action = Some((dev.address.clone(), dev.name.clone(), "unpair"));
                                        }

                                    }

                                    DevState::Available => {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("○")
                                                .color(egui::Color32::GRAY).size(14.0));
                                            ui.label(egui::RichText::new(l.not_paired_label)
                                                .color(egui::Color32::GRAY)
                                                .size(13.0).strong());
                                        });
                                        ui.add_space(10.0);
                                        ui.separator();
                                        ui.add_space(8.0);

                                        let btn_pair = egui::Button::new(
                                            egui::RichText::new(l.btn_pair)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        )
                                        .fill(egui::Color32::from_rgb(50, 120, 220))
                                        .min_size(egui::vec2(240.0, 32.0));
                                        if ui.add(btn_pair).clicked() {
                                            action = Some((dev.address.clone(), dev.name.clone(), "pair"));
                                        }
                                    }
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(4.0);
                                if ui.button(l.close).clicked() {
                                    self.selected_device = None;
                                }
                            });
                    }

                    ui.add_space(4.0);
                    ui.separator();
                }

                if let Some((addr, name, act)) = action {
                    self.selected_device = None;
                    match act {
                        "disconnect" => self.do_disconnect(addr),
                        "unpair"     => self.do_unpair(addr),
                        "connect"    => {
                            log(&format!("[ui] reconnect session for {}", addr));
                            self.pair_session = Some(start_pair_session(
                                &self.sudo_password, &addr, &name, PairGoal::Reconnect, Arc::clone(&self.scan), l,
                            ));
                        }
                        "repair" => {
                            log(&format!("[ui] repair session for {}", addr));
                            self.pair_session = Some(start_pair_session(
                                &self.sudo_password, &addr, &name, PairGoal::RepairForce, Arc::clone(&self.scan), l,
                            ));
                        }
                        "pair" => {
                            log(&format!("[ui] pair session for {}", addr));
                            self.pair_session = Some(start_pair_session(
                                &self.sudo_password, &addr, &name, PairGoal::PairFresh, Arc::clone(&self.scan), l,
                            ));
                        }
                        _ => {}
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Bluetooth",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Bluetooth")
                .with_inner_size([460.0, 580.0])
                .with_min_inner_size([360.0, 400.0]),
            ..Default::default()
        },
        Box::new(|cc| Box::new(BtApp::new(cc))),
    )
}
