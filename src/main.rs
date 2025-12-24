//! # PSU Controller Main Entry
//! 
//! This module handles the Slint UI initialization, manages the serial port 
//! connection state, and binds UI events to SCPI communication logic.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod scpi; 

use slint::{ComponentHandle, Model, SharedString, VecModel, Color, Timer, TimerMode};
use std::time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque; // 用來做 Ring Buffer
use serialport::{ClearBuffer, SerialPort};

slint::include_modules!();

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

    // --- 2. 共享資源 ---
    let shared_port: Rc<RefCell<Option<Box<dyn SerialPort>>>> = Rc::new(RefCell::new(None));
    let loop_timer = Rc::new(RefCell::new(Timer::default()));
    let loop_state = Rc::new(RefCell::new(false)); 
    let monitor_timer = Rc::new(RefCell::new(Timer::default())); // 在 main 裡叫 monitor_timer

    // --- 3. 連線/斷線邏輯 ---
    let ui_handle = ui.as_weak();
    let sp_connect = shared_port.clone(); 
    let monitor_timer_ref = monitor_timer.clone(); 

    ui.on_toggle_connection(move || {
        let ui = ui_handle.unwrap();
        
        if ui.get_status_text() == "Connected" {
            // --- 斷線邏輯 ---
            monitor_timer_ref.borrow().stop();
            
            // 解鎖面板
            if let Some(ref mut p) = *sp_connect.borrow_mut() {
                let _ = scpi::send_command(p, scpi::cmds::UNLOCK);
            }
            *sp_connect.borrow_mut() = None; 

            // 更新狀態列
            ui.set_status_text("Disconnected".into());
            ui.set_status_color(Color::from_rgb_u8(255, 0, 0).into());
            ui.set_window_title("Rust PSU Controller".into());
            
            // 重置功能開關
            ui.set_is_looping(false); 
            ui.set_is_output_on(false); // 按鈕變回灰色

            // 🟢 [新增] 重置讀值顯示
            ui.set_voltage_reading("---".into());
            ui.set_current_reading("---".into());
            ui.set_power_reading("0.00".into()); // 如果你有加功率計的話
            ui.set_psu_mode("".into());          // 清除 CC/CV 燈號
        } else {
            let port_name = ui.get_selected_port();
            match serialport::new(port_name.as_str(), 9600).timeout(Duration::from_millis(500)).open() {
                Ok(mut p) => {
                    let _ = p.clear(ClearBuffer::Input);
                    
                    // 1. 獲取 IDN
                    if let Some(info) = scpi::send_command(&mut p, scpi::cmds::IDN) {
                        ui.set_window_title(format!("Rust PSU Controller - {}", info).into());
                    }

                    // 2. 同步 Output 狀態 (上一回加的)
                    if let Some(outp_status) = scpi::send_command(&mut p, scpi::cmds::READ_OUTP) {
                        let clean = outp_status.trim().to_uppercase();
                        let is_on = clean == "1" || clean == "ON";
                        ui.set_is_output_on(is_on);
                    }

                    // 🟢 [新增] 3. 同步設定電壓 (Set Voltage)
                    if let Some(v_str) = scpi::send_command(&mut p, scpi::cmds::GET_SET_VOLT) {
                        // SCPI 可能回傳 "12.0000"，我們解析後轉回 "12.00" 保持介面整潔
                        let val: f64 = v_str.trim().parse().unwrap_or(0.0);
                        ui.set_target_voltage(format!("{:.2}", val).into());
                    }

                    // 🟢 [新增] 4. 同步設定電流 (Set Current Limit)
                    if let Some(c_str) = scpi::send_command(&mut p, scpi::cmds::GET_SET_CURR) {
                        // 轉為 3 位小數，例如 "1.500"
                        let val: f64 = c_str.trim().parse().unwrap_or(0.0);
                        ui.set_target_current(format!("{:.3}", val).into());
                    }

                    // 3. 同步設定電壓 (Set Voltage)
                    if let Some(v_str) = scpi::send_command(&mut p, scpi::cmds::GET_SET_VOLT) {
                        let val: f64 = v_str.trim().parse().unwrap_or(0.0);
                        // 更新輸入框 (給人看)
                        ui.set_target_voltage(format!("{:.2}", val).into());
                        // 🟢 [新增] 更新生效值 (給邏輯用)
                        ui.set_active_voltage_target(val as f32);
                    }

                    // 4. 同步設定電流 (Set Current Limit)
                    if let Some(c_str) = scpi::send_command(&mut p, scpi::cmds::GET_SET_CURR) {
                        let val: f64 = c_str.trim().parse().unwrap_or(0.0);
                        // 更新輸入框 (給人看)
                        ui.set_target_current(format!("{:.3}", val).into());
                        // 🟢 [新增] 更新生效值 (給邏輯用)
                        ui.set_active_current_limit(val as f32);
                    }

                    // 5. 設定連線狀態
                    *sp_connect.borrow_mut() = Some(p); 
                    ui.set_status_text("Connected".into());
                    ui.set_status_color(Color::from_rgb_u8(0, 128, 0).into()); 

                    if ui.get_enable_auto_refresh() {
                        trigger_auto_poll(ui.as_weak(), sp_connect.clone(), monitor_timer_ref.clone());
                    }
                },
                Err(e) => ui.set_status_text(format!("Err: {}", e).into()),
            }
        }
    });

    // --- 4. Auto Refresh 切換 ---
    let sp_refresh = shared_port.clone();
    let timer_refresh = monitor_timer.clone();
    let ui_refresh = ui.as_weak();
    ui.on_toggle_auto_refresh(move |enabled| {
        let ui = ui_refresh.unwrap();
        if ui.get_status_text() == "Connected" {
            if enabled {
                trigger_auto_poll(ui_refresh.clone(), sp_refresh.clone(), timer_refresh.clone());
            } else {
                timer_refresh.borrow().stop();
            }
        }
    });

    // --- 5. 通用 SCPI 通訊 Closure ---
    let sp_io = shared_port.clone();
    let io_scpi = move |cmd: &str| -> Option<String> {
        let mut port_ref = sp_io.borrow_mut();
        if let Some(ref mut p) = *port_ref {
            scpi::send_command(p, cmd)
        } else {
            None
        }
    };

    // --- 6. 綁定 UI Callbacks ---

    let io = io_scpi.clone();
    ui.on_send_command(move |cmd_str| { io(cmd_str.as_str()); });

    // 設定電壓 Apply
    let io = io_scpi.clone();
    let ui_handle_v = ui.as_weak(); // 需要 handle
    ui.on_apply_voltage(move |v| { 
        io(&format!("{} {}", scpi::cmds::SET_VOLT, v)); 
        // 🟢 [新增] 同步生效值
        let val: f32 = v.parse().unwrap_or(0.0);
        ui_handle_v.unwrap().set_active_voltage_target(val);
    });

    // 設定電流 Apply
    let io = io_scpi.clone();
    let ui_handle_c = ui.as_weak(); // 需要 handle
    ui.on_apply_current(move |c| { 
        io(&format!("{} {}", scpi::cmds::SET_CURR, c)); 
        // 🟢 [新增] 同步生效值
        let val: f32 = c.parse().unwrap_or(0.0);
        ui_handle_c.unwrap().set_active_current_limit(val);
    });

    let io = io_scpi.clone();
    let ui_h = ui.as_weak();
    ui.on_read_voltage(move || {
        if let Some(val) = io(scpi::cmds::READ_VOLT) { ui_h.unwrap().set_voltage_reading(val.into()); }
    });

    let io = io_scpi.clone();
    let ui_h = ui.as_weak();
    ui.on_read_current(move || {
        if let Some(val) = io(scpi::cmds::READ_CURR) { ui_h.unwrap().set_current_reading(val.into()); }
    });

    let io = io_scpi.clone();
    ui.on_confirm_reset(move || { io(scpi::cmds::RESET); });

    let ui_h = ui.as_weak();
    ui.on_adjust_voltage(move |step| {
        let u = ui_h.unwrap();
        let val: f64 = u.get_target_voltage().parse().unwrap_or(0.0);
        u.set_target_voltage(format!("{:.2}", (val + step as f64).max(0.0)).into());
    });
    
    let ui_h = ui.as_weak();
    ui.on_adjust_current(move |step| {
        let u = ui_h.unwrap();
        let val: f64 = u.get_target_current().parse().unwrap_or(0.0);
        u.set_target_current(format!("{:.3}", (val + step as f64).max(0.0)).into());
    });

    // 波形循環邏輯
    let ui_h = ui.as_weak();
    let sp_loop = shared_port.clone(); 
    let t_loop = loop_timer.clone();
    let s_loop = loop_state.clone();
    
    ui.on_toggle_loop(move |va, vb, interval| {
        let u = ui_h.unwrap();
        if u.get_is_looping() {
            t_loop.borrow().stop();
            u.set_is_looping(false);
        } else {
            u.set_is_looping(true);
            let sp = sp_loop.clone();
            let state = s_loop.clone();
            let v1 = va.to_string();
            let v2 = vb.to_string();

            t_loop.borrow().start(TimerMode::Repeated, Duration::from_millis(interval as u64), move || {
                let mut curr_state = state.borrow_mut();
                *curr_state = !*curr_state;
                let target_v = if *curr_state { &v1 } else { &v2 };
                if let Some(ref mut p) = *sp.borrow_mut() {
                    let _ = scpi::send_command(p, &format!("{} {}", scpi::cmds::SET_VOLT, target_v));
                }
            });
        }
    });

    // 🔴 [已刪除] 這裡原本有一段 "7. 圖表資料處理" 的重複程式碼，已移除。
    // 圖表更新已經整合進底部的 trigger_auto_poll 函式，並透過上方的 callbacks 呼叫。

    ui.run()?;
    Ok(())
}

// 🟢 [新增] 一個輔助函式，用來把數值陣列轉成 SVG Path 字串
// 參數: buffer (數據), width (圖寬), height (圖高)
fn generate_svg_path(buffer: &VecDeque<f32>, width: f32, height: f32) -> String {
    if buffer.is_empty() { return String::new(); }

    // 1. 找出最大值做 Auto-Scale (防止除以 0，且給一點頂部空間)
    // 技巧: 如果最大值很小(例如 0V)，強制設為 1.0，避免線條亂飛
    let max_val = buffer.iter().fold(0.0f32, |a, &b| a.max(b)).max(1.0) * 1.1; 
    
    let mut path_cmd = String::with_capacity(1024);
    use std::fmt::Write;

    for (i, &val) in buffer.iter().enumerate() {
        let x = (i as f32 / (buffer.len() - 1) as f32) * width;
        // Y 軸反轉 (Slint 0 在上面)
        let y = height - (val / max_val * height); 
        
        if i == 0 {
            let _ = write!(path_cmd, "M {:.1} {:.1} ", x, y);
        } else {
            let _ = write!(path_cmd, "L {:.1} {:.1} ", x, y);
        }
    }
    path_cmd
}

// 🟢 [修改] 主邏輯函式
fn trigger_auto_poll(ui_weak: slint::Weak<AppWindow>, sp: Rc<RefCell<Option<Box<dyn SerialPort>>>>, timer: Rc<RefCell<Timer>>) {
    // 1. 初始化歷史資料 Buffer
    const CHART_WIDTH: usize = 100; // 這是我們固定的採樣點數
    let mut history_v = VecDeque::with_capacity(CHART_WIDTH);
    let mut history_i = VecDeque::with_capacity(CHART_WIDTH);
    for _ in 0..CHART_WIDTH { 
        history_v.push_back(0.0f32); 
        history_i.push_back(0.0f32); 
    }

    // 2. 讀取時間並限制最小間隔 (避免過快導致塞車)
    let ui = ui_weak.unwrap(); 
    let raw_interval = ui.get_polling_interval().parse::<u64>().unwrap_or(100);
    // 強制設定最小 200ms (RS232 物理極限保護)
    let interval_ms = raw_interval.max(200); 

    if raw_interval < 200 {
        ui.set_polling_interval(format!("{}", interval_ms).into());
    }

    // 更新圖表時間標籤
    let total_seconds = (interval_ms as f64 * CHART_WIDTH as f64) / 1000.0;
    ui.set_chart_duration(format!("{:.1}s", total_seconds).into());

    // 🟢 [修改] 使用變數 interval_ms
    timer.borrow().start(TimerMode::Repeated, Duration::from_millis(interval_ms), move || {
        let ui = ui_weak.unwrap();
        let mut port_ref = sp.borrow_mut();
        
        // 暫存目前的數值
        let mut curr_v = 0.0f32;
        let mut curr_i = 0.0f32;
        let mut read_success = false;

        // --- A. SCPI 通訊 ---
        if let Some(ref mut p) = *port_ref {
            
            if let Some(raw_res) = scpi::send_command(p, scpi::cmds::READ_ALL) {
                let clean_str = raw_res.replace("«", "").trim().to_string();
                let parts: Vec<&str> = clean_str.split(',').collect();
                
                if parts.len() >= 2 {
                    let v_str = parts[0].trim();
                    let i_str = parts[1].trim();
                    
                    // 1. 更新文字 UI (只有讀成功才更新文字)
                    ui.set_voltage_reading(v_str.into());
                    ui.set_current_reading(i_str.into());
                    
                    // 2. 解析數值
                    curr_v = v_str.parse().unwrap_or(0.0);
                    curr_i = i_str.parse().unwrap_or(0.0);

                    // 3. 更新功率 UI
                    let power = curr_v * curr_i;
                    ui.set_power_reading(format!("{:.2}", power).into());

                    // 🟢 [修正] CC/CV 智能判斷邏輯
                    // 1. 取得生效的電流上限 (Active Limit)
                    let i_limit_active = ui.get_active_current_limit() as f32;
                    
                    // 2. 判斷是否 Output ON (沒開電就不顯示模式)
                    let output_on = ui.get_is_output_on();

                    // 3. 判斷 CC (電流接近上限 95% 且大於 10mA 避免雜訊)
                    let is_cc = (curr_i - i_limit_active).abs() < (i_limit_active * 0.05) && curr_i > 0.01;

                    let mode = if !output_on {
                        "" // 沒開電，燈號熄滅
                    } else if is_cc {
                        "CC" // 限流模式
                    } else {
                        "CV" // 定壓模式
                    };
                    ui.set_psu_mode(mode.into());

                    // 🟢 [重點修改 2] 標記讀取成功
                    read_success = true;
                }
            }
        }

        // --- B. 圖表更新邏輯 ---
        
        // 🟢 [重點修改 3] 如果讀取失敗 (塞車或超時)，使用「上一次的值」填補
        // 這樣圖表會變成「水平線」繼續往左跑，而不會掉到 0，也不會因為沒 push 導致不同步
        if !read_success {
             // 拿 Buffer 最後一筆資料，如果 Buffer 是空的就用 0.0
             curr_v = *history_v.back().unwrap_or(&0.0);
             curr_i = *history_i.back().unwrap_or(&0.0);
        }

        // 🟢 [重點修改 4] 無條件推進 Buffer (保證 V 和 I 永遠同步)
        // 不管 read_success 是 true 還是 false，這裡都要執行
        
        // 更新 V
        history_v.pop_front();
        history_v.push_back(curr_v);
        
        // 更新 I
        history_i.pop_front();
        history_i.push_back(curr_i);

        // 3. 生成 SVG (重複利用 generate_svg_path 函式)
        let chart_h = 120.0; // 對應 UI 高度
        let chart_w = 750.0; // 對應 UI 寬度

        let path_v_str = generate_svg_path(&history_v, chart_w, chart_h);
        let path_i_str = generate_svg_path(&history_i, chart_w, chart_h);

        // 4. 更新 UI
        ui.set_chart_data_v(path_v_str.into());
        ui.set_chart_data_i(path_i_str.into());
    });
}