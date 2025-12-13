use slint::{ComponentHandle, Model, SharedString, VecModel, Color, Timer, TimerMode};
use std::time::Duration;
use std::io::{Read, Write};
use std::rc::Rc;
use std::cell::RefCell;
use serialport::{SerialPort, ClearBuffer};

slint::include_modules!();

// ==========================================
// SCPI 指令定義區
// ==========================================
const CMD_SET_VOLT: &str   = "VOLT";                    // 設定電壓
const CMD_SET_CURR: &str   = "CURR";                    // 設定電流
const CMD_READ_VOLT: &str  = "MEAS:VOLT?";              // 讀取電壓
const CMD_READ_CURR: &str  = "MEAS:CURR?";              // 讀取電流
const CMD_OUTP_ON: &str    = "OUTP ON";                 // 開啟輸出
const CMD_OUTP_OFF: &str   = "OUTP OFF";                // 關閉輸出
const CMD_UNLOCK: &str     = "SYST:COMM:RLST LOC";      // 面板解鎖 (有些機器用 SYSTem:LOCal )
const CMD_RESET: &str      = "*RST";                    // 機器重置
const CMD_INFO: &str       = "*IDN?";                   // 機器資訊
// ==========================================

// 獨立的讀取函式：負責從 Port 讀取直到收到換行符號或超時
fn read_serial_response(port: &mut Box<dyn SerialPort>) -> Option<String> {
    let mut received_bytes: Vec<u8> = Vec::new();
    let mut byte_buf = [0u8; 1];
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_millis(500); // 設定讀取超時

    loop {
        if start_time.elapsed() > timeout {
            // 超時回傳目前收到的，或直接回傳 Timeout
            if received_bytes.is_empty() { return None; }
            break;
        }

        match port.read(&mut byte_buf) {
            Ok(1) => {
                let b = byte_buf[0];
                received_bytes.push(b);
                if b == b'\n' { break; } // 收到換行就停止
            },
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                eprintln!("Read Error: {}", e);
                break;
            }
        }
    }
    
    if received_bytes.is_empty() {
        return None;
    }
    
    // 轉字串並修剪空白
    Some(String::from_utf8_lossy(&received_bytes).trim().to_string())
}

fn main() -> Result<(), anyhow::Error> {
    let ui = AppWindow::new()?;

    // --- 1. Port 列表初始化 ---
    let ports = serialport::available_ports().unwrap_or_default();
    let mut port_names: Vec<SharedString> = vec![];
    if ports.is_empty() { port_names.push("No Ports Found".into()); } 
    else { for p in ports { port_names.push(p.port_name.into()); } }
    
    let ports_model = Rc::new(VecModel::from(port_names));
    ui.set_available_ports(ports_model.clone().into());
    if let Some(first_port) = ports_model.row_data(0) { ui.set_selected_port(first_port); }

    // --- 2. 建立共享 Port ---
    let shared_port: Rc<RefCell<Option<Box<dyn SerialPort>>>> = Rc::new(RefCell::new(None));

    // --- 3. 建立 Timer 物件與 Loop 狀態 (給波形產生器用) ---
    let loop_timer = Rc::new(RefCell::new(Timer::default()));
    let loop_state = Rc::new(RefCell::new(false)); // true=VoltA, false=VoltB

    // --- 4. 連線/斷線邏輯 ---
    let ui_handle = ui.as_weak();
    let sp_connect = shared_port.clone(); 

    ui.on_toggle_connection(move || {
        let ui = ui_handle.unwrap();
        
        if ui.get_status_text() == "Connected" {
            // === 斷線邏輯 ===
            if let Some(ref mut p) = *sp_connect.borrow_mut() {
                let _ = p.write(format!("{}\r\n", CMD_UNLOCK).as_bytes()); // 解鎖面板
                std::thread::sleep(Duration::from_millis(50));
            }
            *sp_connect.borrow_mut() = None; 
            ui.set_status_text("Disconnected".into());
            ui.set_status_color(Color::from_rgb_u8(255, 0, 0).into());
            ui.set_device_info("Device Info: ---".into()); // 清空資訊
            ui.set_is_looping(false); // 斷線時強制停止 Loop
        } else {
            // === 連線邏輯 ===
            let port_name = ui.get_selected_port();
            match serialport::new(port_name.as_str(), 9600).timeout(Duration::from_millis(500)).open() {
                Ok(mut p) => {
                    // 連線成功後，詢問機器資訊 (*IDN?)
                    let _ = p.clear(ClearBuffer::Input);
                    if let Ok(_) = p.write(format!("{}\r\n", CMD_INFO).as_bytes()) {
                         if let Some(info) = read_serial_response(&mut p) {
                             ui.set_device_info(format!("Device: {}", info).into());
                         }
                    }

                    *sp_connect.borrow_mut() = Some(p); 
                    ui.set_status_text("Connected".into());
                    ui.set_status_color(Color::from_rgb_u8(0, 128, 0).into()); 
                },
                Err(e) => ui.set_status_text(format!("Err: {}", e).into()),
            }
        }
    });

    // =======================================================
    //  通用 SCPI 通訊 Closure (Helper)
    // =======================================================
    let sp_io = shared_port.clone();
    let io_scpi = move |cmd: &str| -> Option<String> {
        let mut port_ref = sp_io.borrow_mut();
        if let Some(ref mut p) = *port_ref {
            // 1. 清空 Input Buffer (避免讀到舊資料)
            let _ = p.clear(ClearBuffer::Input);

            // 2. 發送
            let full_cmd = format!("{}\r\n", cmd);
            if let Err(e) = p.write(full_cmd.as_bytes()) {
                eprintln!("Write Error: {}", e);
                return None;
            }
            println!("TX: {}", cmd);

            // 3. 讀取 (如果有問號)
            if cmd.contains("?") {
                let res = read_serial_response(p);
                if let Some(ref s) = res {
                    println!("RX: {}", s);
                }
                return res;
            }
        }
        None
    };

    // =======================================================
    //  綁定 UI Callbacks
    // =======================================================

    // 1. [一般指令] ON / OFF / UNLOCK
    let io = io_scpi.clone();
    ui.on_send_command(move |action| {
        let cmd = match action.as_str() {
            "ON" => CMD_OUTP_ON,
            "OFF" => CMD_OUTP_OFF,
            "UNLOCK" => CMD_UNLOCK,
            _ => return,
        };
        io(cmd);
    });

    // 2. 讀取電壓
    let io = io_scpi.clone();
    let ui_handle = ui.as_weak();
    ui.on_read_voltage(move || {
        let ui = ui_handle.unwrap();
        if let Some(val) = io(CMD_READ_VOLT) {
            ui.set_voltage_reading(val.into());
        }
    });

    // 3. 讀取電流
    let io = io_scpi.clone();
    let ui_handle = ui.as_weak();
    ui.on_read_current(move || {
        let ui = ui_handle.unwrap();
        if let Some(val) = io(CMD_READ_CURR) {
            ui.set_current_reading(val.into());
        }
    });

    // 4. 設定電壓
    let io = io_scpi.clone();
    ui.on_apply_voltage(move |val_str| {
        io(&format!("{} {}", CMD_SET_VOLT, val_str));
    });

    // 5. 設定電流
    let io = io_scpi.clone();
    ui.on_apply_current(move |val_str| {
        io(&format!("{} {}", CMD_SET_CURR, val_str));
    });

    // 6. Reset
    let io = io_scpi.clone();
    ui.on_confirm_reset(move || {
        io(CMD_RESET);
    });

    // 7. 微調 (電壓)
    let ui_handle = ui.as_weak();
    ui.on_adjust_voltage(move |step| {
        let ui = ui_handle.unwrap();
        let current_val: f64 = ui.get_target_voltage().parse().unwrap_or(0.0);
        let new_val = (current_val + step as f64).max(0.0);
        ui.set_target_voltage(format!("{:.2}", new_val).into());
    });

    // 8. 微調 (電流)
    let ui_handle = ui.as_weak();
    ui.on_adjust_current(move |step| {
        let ui = ui_handle.unwrap();
        let current_val: f64 = ui.get_target_current().parse().unwrap_or(0.0);
        let new_val = (current_val + step as f64).max(0.0);
        ui.set_target_current(format!("{:.3}", new_val).into());
    });

    // 9. [新增] 波形循環邏輯 (Timer)
    let ui_handle = ui.as_weak();
    let sp_loop = shared_port.clone(); 
    let timer_ref = loop_timer.clone();
    
    ui.on_toggle_loop(move |volt_a, volt_b, interval_ms| {
        let ui = ui_handle.unwrap();
        
        // 檢查是否正在跑
        if ui.get_is_looping() {
            // A. 停止
            timer_ref.borrow().stop();
            ui.set_is_looping(false);
            println!("Loop Stopped");
        } else {
            // B. 啟動
            ui.set_is_looping(true);
            println!("Loop Start: {}V <-> {}V, Every {}ms", volt_a, volt_b, interval_ms);
            
            let sp_timer = sp_loop.clone();
            let state_ref = loop_state.clone();
            let va = volt_a.to_string();
            let vb = volt_b.to_string();

            timer_ref.borrow().start(
                TimerMode::Repeated, 
                Duration::from_millis(interval_ms as u64), 
                move || {
                    let mut state = state_ref.borrow_mut();
                    *state = !*state; // 翻轉狀態
                    
                    let target_volt = if *state { &va } else { &vb };
                    let command = format!("{} {}\r\n", CMD_SET_VOLT, target_volt);

                    // 直接寫入 Port (不透過 io helper 以免 borrow conflict)
                    if let Some(ref mut p) = *sp_timer.borrow_mut() {
                        if let Err(e) = p.write(command.as_bytes()) {
                            eprintln!("Timer Write Error: {}", e);
                        } else {
                            println!("Auto Set: {} V", target_volt);
                        }
                    }
                }
            );
        }
    });

    ui.run()?;
    Ok(())
}