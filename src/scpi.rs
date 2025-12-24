//! # SCPI Communication Driver
//! 
//! This module defines the SCPI command set and low-level serial communication 
//! functions required to interact with a Programmable Power Supply (PSU).

use std::io::{Read, Write};
use std::time::Duration;
use serialport::SerialPort;

// ==========================================
// SCPI 指令清單 (集中管理，一眼就能看到指令)
// ==========================================
#[allow(dead_code)] // 加入這一行，允許這個模組內有未使用的代碼
pub mod cmds {
    pub const IDN: &str        = "*IDN?";
    pub const RESET: &str      = "*RST";
    pub const UNLOCK: &str     = "SYST:COMM:RLST LOC";
    pub const SET_VOLT: &str   = "VOLT";
    pub const SET_CURR: &str   = "CURR";
    pub const READ_ALL: &str   = "MEAS:ALL?";
    pub const READ_VOLT: &str  = "MEAS:VOLT?";
    pub const READ_CURR: &str  = "MEAS:CURR?";
    pub const READ_OUTP: &str  = "OUTPut?";
    pub const OUTP_ON: &str    = "OUTP ON";
    pub const OUTP_OFF: &str   = "OUTP OFF";
    pub const GET_SET_VOLT: &str = "SOUR:VOLT:LEV:IMM:AMPL?";
    pub const GET_SET_CURR: &str = "SOUR:CURR:LEV:IMM:AMPL?";
}

/// 讀取序列埠回應
pub fn read_serial_response(port: &mut Box<dyn SerialPort>) -> Option<String> {
    let mut received_bytes: Vec<u8> = Vec::new();
    let mut byte_buf = [0u8; 1];
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_millis(500);

    loop {
        if start_time.elapsed() > timeout {
            if received_bytes.is_empty() { return None; }
            break;
        }

        match port.read(&mut byte_buf) {
            Ok(1) => {
                let b = byte_buf[0];
                received_bytes.push(b);
                if b == b'\n' { break; } 
            },
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                eprintln!("Read Error: {}", e);
                break;
            }
        }
    }
    
    if received_bytes.is_empty() { return None; }
    Some(String::from_utf8_lossy(&received_bytes).trim().to_string())
}

/// 傳送指令並(選擇性)讀取回傳
pub fn send_command(port: &mut Box<dyn SerialPort>, cmd: &str) -> Option<String> {
    let full_cmd = format!("{}\r\n", cmd);
    if let Err(e) = port.write_all(full_cmd.as_bytes()) {
        eprintln!("Write Error: {}", e);
        return None;
    }
    
    if cmd.contains('?') {
        read_serial_response(port)
    } else {
        None
    }
}