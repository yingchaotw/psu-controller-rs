//! # PSU Controller Main Entry
//! 
//! This module handles the Slint UI initialization, manages the serial port 
//! connection state, and binds UI events to SCPI communication logic.
//!
//! ## Main Features
//! - Serial port scanning and selection
//! - Real-time monitoring of voltage and current readings
//! - Waveform Generator (Voltage cycle testing)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod scpi; 

use slint::{ComponentHandle, Model, SharedString, VecModel, Color, Timer, TimerMode};
use std::time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
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
    let monitor_timer = Rc::new(RefCell::new(Timer::default()));

    // --- 3. 連線/斷線邏輯 ---
    let ui_handle = ui.as_weak();
    let sp_connect = shared_port.clone(); 
    let monitor_timer_ref = monitor_timer.clone(); 

    ui.on_toggle_connection(move || {
        let ui = ui_handle.unwrap();
        
        if ui.get_status_text() == "Connected" {
            monitor_timer_ref.borrow().stop();
            if let Some(ref mut p) = *sp_connect.borrow_mut() {
                // 使用 scpi::cmds:: 常數
                let _ = scpi::send_command(p, scpi::cmds::UNLOCK);
            }
            *sp_connect.borrow_mut() = None; 
            ui.set_status_text("Disconnected".into());
            ui.set_status_color(Color::from_rgb_u8(255, 0, 0).into());
            ui.set_is_looping(false); 
            ui.set_window_title("Rust PSU Controller".into());
        } else {
            let port_name = ui.get_selected_port();
            match serialport::new(port_name.as_str(), 9600).timeout(Duration::from_millis(500)).open() {
                Ok(mut p) => {
                    let _ = p.clear(ClearBuffer::Input);
                    // 獲取 IDN
                    if let Some(info) = scpi::send_command(&mut p, scpi::cmds::IDN) {
                        ui.set_window_title(format!("Rust PSU Controller - {}", info).into());
                    }

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

    // 傳送自定義指令
    let io = io_scpi.clone();
    ui.on_send_command(move |cmd_str| { io(cmd_str.as_str()); });

    // 設定電壓與電流
    let io = io_scpi.clone();
    ui.on_apply_voltage(move |v| { io(&format!("{} {}", scpi::cmds::SET_VOLT, v)); });
    let io = io_scpi.clone();
    ui.on_apply_current(move |c| { io(&format!("{} {}", scpi::cmds::SET_CURR, c)); });

    // 手動讀取
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

    // 重置
    let io = io_scpi.clone();
    ui.on_confirm_reset(move || { io(scpi::cmds::RESET); });

    // 微調邏輯 (不需要通訊，純 UI 狀態計算)
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

    ui.run()?;
    Ok(())
}

fn trigger_auto_poll(ui_weak: slint::Weak<AppWindow>, sp: Rc<RefCell<Option<Box<dyn SerialPort>>>>, timer: Rc<RefCell<Timer>>) {
    timer.borrow().start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let ui = ui_weak.unwrap();
        let mut port_ref = sp.borrow_mut();
        if let Some(ref mut p) = *port_ref {
            // 明確接收 Option<String> 解決推導問題
            let response: Option<String> = scpi::send_command(p, scpi::cmds::READ_ALL);
            if let Some(raw_res) = response {
                let clean_str = raw_res.replace("«", "").trim().to_string();
                let parts: Vec<&str> = clean_str.split(',').collect();
                if parts.len() >= 2 {
                    ui.set_voltage_reading(parts[0].trim().into());
                    ui.set_current_reading(parts[1].trim().into());
                }
            }
        }
    });
}