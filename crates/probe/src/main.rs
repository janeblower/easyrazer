//! Read-only probe for the Razer Huntsman V2 Analog control channel (MI_03).

use std::fmt::Write as _;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use razer_core::analog::{self, Mode};
use razer_core::keymap;
use razer_core::packet::{self, Command, Response, Status};

const VID: u16 = 0x1532;
const PID: u16 = 0x0266;
const CONTROL_INTERFACE: i32 = 3;
const TID: u8 = 0x1F;

const USAGE: &str = "\
usage: razer-probe <command>
  list                          HID interfaces of 1532:0266
  info                          firmware, serial, device mode
  get <cls> <id> <size> [hex..] raw getter (id must have bit 7 set)
  actuation <profile> [keys..]  per-key assignment and actuation thresholds
  dump <file>                   info + actuation of every key in every profile
  mode <0|3>                    device mode: 0 hardware, 3 driver (not persisted)
  actuate <profile> <key> <low> <high>  WRITE thresholds of one key (normal layer)
  stream [secs]                 raw input reports from MI_01 vendor collections";

type Result<T> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first() else {
        return Err(USAGE.into());
    };
    let rest = &args[1..];
    let api = HidApi::new().map_err(|e| e.to_string())?;
    if cmd == "stream" {
        return stream(&api, rest);
    }
    if cmd == "list" {
        return list(&api);
    }
    warn_if_synapse_running();
    let dev = open(&api)?;
    let out = match cmd.as_str() {
        "info" => info(&dev)?,
        "get" => raw_get(&dev, rest)?,
        "actuation" => actuation(&dev, profile_arg(rest)?, &keys_arg(&rest[1..])?)?,
        "dump" => return dump(&dev, rest),
        "mode" => set_mode(&dev, rest)?,
        "actuate" => actuate(&dev, rest)?,
        _ => return Err(USAGE.into()),
    };
    print!("{out}");
    Ok(())
}

fn list(api: &HidApi) -> Result<()> {
    for d in api.device_list().filter(|d| d.vendor_id() == VID && d.product_id() == PID) {
        println!(
            "mi={} usage_page={:04X} usage={:04X} {}",
            d.interface_number(),
            d.usage_page(),
            d.usage(),
            d.path().to_string_lossy()
        );
    }
    Ok(())
}

fn open(api: &HidApi) -> Result<HidDevice> {
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == CONTROL_INTERFACE)
        .ok_or("control interface MI_03 not found")?;
    info.open_device(api).map_err(|e| e.to_string())
}

fn warn_if_synapse_running() {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq RazerAppEngine.exe", "/NH"])
        .output();
    if let Ok(o) = out {
        if String::from_utf8_lossy(&o.stdout).contains("RazerAppEngine") {
            eprintln!("warning: Synapse (RazerAppEngine) is running and may interleave commands on MI_03");
        }
    }
}

fn exchange(dev: &HidDevice, cmd: Command, size: u8, args: &[u8]) -> Result<Response> {
    let mut buf = [0u8; packet::LEN + 1];
    buf[1..].copy_from_slice(&packet::request(TID, cmd, size, args));
    dev.send_feature_report(&buf).map_err(|e| format!("{cmd} set: {e}"))?;
    for _ in 0..10 {
        sleep(Duration::from_millis(15));
        let mut rx = [0u8; packet::LEN + 1];
        let n = dev.get_feature_report(&mut rx).map_err(|e| format!("{cmd} get: {e}"))?;
        if n < rx.len() {
            return Err(format!("{cmd}: short report ({n} bytes)"));
        }
        let r = Response::parse(rx[1..].try_into().unwrap());
        match r.status {
            Status::Busy => continue,
            Status::Ok if r.cmd == cmd && r.tid == TID => {
                if !r.crc_ok {
                    eprintln!("warning: {cmd}: response crc mismatch");
                }
                return Ok(r);
            }
            Status::Ok => return Err(format!("{cmd}: reply is for {} tid {:02X}", r.cmd, r.tid)),
            s => return Err(format!("{cmd}: status {s:?}")),
        }
    }
    Err(format!("{cmd}: device stayed busy"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn info(dev: &HidDevice) -> Result<String> {
    let fw = exchange(dev, Command::new(0x00, 0x81), 2, &[])?;
    let serial = exchange(dev, Command::new(0x00, 0x82), 0x16, &[])?;
    let mode = exchange(dev, Command::new(0x00, 0x84), 2, &[])?;
    let active = exchange(dev, Command::new(0x05, 0x84), 1, &[])?;
    let sn: String = serial.data().iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
    Ok(format!(
        "firmware {}.{}\nserial   {sn}\nmode     {}\nprofiles {:?} (active {})\n",
        fw.args[0],
        fw.args[1],
        hex(&mode.args[..2]),
        profiles(dev)?,
        active.args[0]
    ))
}

/// Profile ids stored on the device (`05:81`: count, then ids).
fn profiles(dev: &HidDevice) -> Result<Vec<u8>> {
    let r = exchange(dev, Command::new(0x05, 0x81), 80, &[])?;
    let n = (r.args[0] as usize).min(packet::ARGS_LEN - 1);
    Ok(r.args[1..=n].to_vec())
}

fn parse_hex(s: &str) -> Result<u8> {
    u8::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| format!("bad hex byte: {s}"))
}

fn raw_get(dev: &HidDevice, a: &[String]) -> Result<String> {
    if a.len() < 3 {
        return Err(USAGE.into());
    }
    let cmd = Command::new(parse_hex(&a[0])?, parse_hex(&a[1])?);
    if !cmd.is_get() {
        return Err(format!("{cmd} is not a getter; writes are not allowed here"));
    }
    let size = parse_hex(&a[2])?;
    let args = a[3..].iter().map(|s| parse_hex(s)).collect::<Result<Vec<_>>>()?;
    let r = exchange(dev, cmd, size, &args)?;
    Ok(format!("{cmd} size={} data: {}\n", r.size, hex(r.data())))
}

fn profile_arg(a: &[String]) -> Result<u8> {
    a.first().ok_or(USAGE)?.parse().map_err(|_| "profile must be a number".into())
}

fn keys_arg(a: &[String]) -> Result<Vec<u8>> {
    if a.is_empty() {
        return Ok(keymap::KEYS.iter().map(|&(_, id)| id).collect());
    }
    a.iter()
        .map(|s| keymap::by_name(s).or_else(|| s.parse().ok()).ok_or(format!("unknown key: {s}")))
        .collect()
}

fn key_label(id: u8) -> String {
    format!("{:>3} {:<22}", id, keymap::name(id).unwrap_or("?"))
}


fn actuation(dev: &HidDevice, profile: u8, keys: &[u8]) -> Result<String> {
    let mut out = String::new();
    for mode in [Mode::Normal, Mode::Hypershift] {
        for &key in keys {
            let r = exchange(
                dev,
                analog::GET_KEY_ASSIGNMENT,
                analog::KEY_ASSIGNMENT_SIZE,
                &analog::get_args(profile, key, mode),
            )?;
            let a = analog::parse(r.data()).ok_or_else(|| format!("key {key}: short reply {}", hex(r.data())))?;
            let _ = writeln!(
                out,
                "{:?} {} low {:3} ({:.2} mm)  high {:3} ({:.2} mm)  fn {:02X} [{}]",
                mode,
                key_label(a.key),
                a.threshold_low,
                analog::threshold_to_mm(a.threshold_low),
                a.threshold_high,
                analog::threshold_to_mm(a.threshold_high),
                a.fn_id,
                hex(&a.fn_data)
            );
        }
    }
    Ok(out)
}

fn dump(dev: &HidDevice, a: &[String]) -> Result<()> {
    let file = a.first().ok_or(USAGE)?;
    let keys = keys_arg(&[])?;
    let mut out = info(dev)?;
    let mut ids = vec![0];
    ids.extend(profiles(dev)?);
    for p in ids {
        let _ = writeln!(out, "\n== profile {p}");
        out += &actuation(dev, p, &keys).unwrap_or_else(|e| format!("  {e}\n"));
    }
    std::fs::write(file, &out).map_err(|e| format!("{file}: {e}"))?;
    println!("wrote {file}");
    Ok(())
}

fn set_mode(dev: &HidDevice, a: &[String]) -> Result<String> {
    let m: u8 = match a.first().map(String::as_str) {
        Some("0") => 0x00,
        Some("3") => 0x03,
        _ => return Err(USAGE.into()),
    };
    exchange(dev, Command::new(0x00, 0x04), 2, &[m, 0])?;
    let r = exchange(dev, Command::new(0x00, 0x84), 2, &[])?;
    Ok(format!("mode {}\n", hex(&r.args[..2])))
}

/// Read-modify-write of one key's thresholds in the normal layer; the mapping is kept as is.
fn actuate(dev: &HidDevice, a: &[String]) -> Result<String> {
    let [profile, key, low, high] = a else {
        return Err(USAGE.into());
    };
    let profile: u8 = profile.parse().map_err(|_| "profile must be a number")?;
    let key = keys_arg(std::slice::from_ref(key))?[0];
    let low: u8 = low.parse().map_err(|_| "low must be 0..=255")?;
    let high: u8 = high.parse().map_err(|_| "high must be 0..=255")?;

    let read = |dev: &HidDevice| -> Result<analog::KeyAssignment> {
        let r = exchange(
            dev,
            analog::GET_KEY_ASSIGNMENT,
            analog::KEY_ASSIGNMENT_SIZE,
            &analog::get_args(profile, key, Mode::Normal),
        )?;
        analog::parse(r.data()).ok_or_else(|| format!("short reply {}", hex(r.data())))
    };
    let before = read(dev)?;
    let after = analog::KeyAssignment { threshold_low: low, threshold_high: high, ..before.clone() };
    exchange(dev, analog::SET_KEY_ASSIGNMENT, analog::KEY_ASSIGNMENT_SIZE, &analog::set_args(&after))?;
    let now = read(dev)?;
    Ok(format!(
        "before {}\nafter  {}\n",
        hex(&analog::set_args(&before)),
        hex(&analog::set_args(&now))
    ))
}

/// Prints raw input reports from the vendor collections of MI_01 for a few seconds.
fn stream(api: &HidApi, a: &[String]) -> Result<()> {
    let secs: u64 = a.first().map_or(Ok(10), |s| s.parse()).map_err(|_| "seconds must be a number")?;
    let devs: Vec<(String, HidDevice)> = api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == 1 && d.usage() == 0)
        .filter_map(|d| {
            let name = d.path().to_string_lossy();
            let col = name.split('&').find(|p| p.starts_with("Col")).unwrap_or("?").to_string();
            match d.open_device(api) {
                Ok(h) => Some((col, h)),
                Err(e) => {
                    eprintln!("{col}: {e}");
                    None
                }
            }
        })
        .collect();
    if devs.is_empty() {
        return Err("no MI_01 vendor collections opened".into());
    }
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    let start = std::time::Instant::now();
    let mut buf = [0u8; 64];
    while std::time::Instant::now() < end {
        for (col, d) in &devs {
            if let Ok(n) = d.read_timeout(&mut buf, 1) {
                if n > 0 {
                    println!("{:6} {col} {}", start.elapsed().as_millis(), hex(&buf[..n]));
                }
            }
        }
    }
    Ok(())
}
