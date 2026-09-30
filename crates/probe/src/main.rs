//! Read-only probe for the Razer Huntsman V2 Analog control channel (MI_03).

use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use razer_core::actuation;
use razer_core::analog::{self, Layer};
use razer_core::control;
use razer_core::devices;
use razer_core::hid::{self, HidTransport, VID};
use razer_core::keymap;
use razer_core::packet::{self, Command};
use razer_core::transport;

const USAGE: &str = "\
usage: razer-probe <command>
  list                          HID interfaces of 1532:0266
  info                          firmware, serial, device mode
  get <cls> <id> <size> [hex..] raw getter (id must have bit 7 set)
  actuation <profile> [keys..]  per-key assignment and actuation thresholds
  dump <file>                   info + actuation of every key in every profile
  mode <0|3>                    device mode: 0 hardware, 3 driver (not persisted)
  actuate <profile> <key> <low> <high>  WRITE thresholds of one key (normal layer)
  stream [secs]                 raw input reports from MI_01 vendor collections
  lamps                         HID LampArray (MI_04, usage page 0x59): descriptor, attributes, lamps
  lampauto <0|1>                WRITE LampArray autonomous mode (1 = firmware effects)
  lampfill <r> <g> <b>          WRITE every LampArray lamp to one color
  set <cls> <id> <size> [hex..] WRITE raw command";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The probe explores one model; the app finds models through `razer_core::devices`.
const PID: u16 = 0x0266;

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
    let api = HidApi::new()?;
    if cmd == "stream" {
        return stream(&api, rest);
    }
    if cmd == "list" {
        return list(&api);
    }
    if cmd == "lamps" {
        return lamps(&api);
    }
    if cmd == "lampauto" {
        return lamp_auto(&api, rest);
    }
    if cmd == "lampfill" {
        return lamp_fill(&api, rest);
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
        "set" => raw_set(&dev, rest)?,
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

fn open(api: &HidApi) -> Result<HidTransport> {
    hid::open_control(api)?
        .map(|(t, _)| t)
        .ok_or_else(|| "control interface MI_03 not found".into())
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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn info(dev: &HidTransport) -> Result<String> {
    let fw = transport::exchange(dev, Command::new(0x00, 0x81), 2, &[])?;
    let serial = transport::exchange(dev, Command::new(0x00, 0x82), 0x16, &[])?;
    let mode = transport::exchange(dev, Command::new(0x00, 0x84), 2, &[])?;
    let active = control::active_profile(dev)?;
    let sn: String = serial.data().iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
    Ok(format!(
        "firmware {}.{}\nserial   {sn}\nmode     {}\nprofiles {:?} (active {})\n",
        fw.args[0],
        fw.args[1],
        hex(&mode.args[..2]),
        profiles(dev)?,
        active
    ))
}

/// Profile ids stored on the device (`05:81`: count, then ids).
fn profiles(dev: &HidTransport) -> Result<Vec<u8>> {
    let r = transport::exchange(dev, Command::new(0x05, 0x81), 80, &[])?;
    let n = (r.args[0] as usize).min(packet::ARGS_LEN - 1);
    Ok(r.args[1..=n].to_vec())
}

fn parse_hex(s: &str) -> Result<u8> {
    u8::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| format!("bad hex byte: {s}").into())
}

fn raw_get(dev: &HidTransport, a: &[String]) -> Result<String> {
    if a.len() < 3 {
        return Err(USAGE.into());
    }
    let cmd = Command::new(parse_hex(&a[0])?, parse_hex(&a[1])?);
    if !cmd.is_get() {
        return Err(format!("{cmd} is not a getter; writes are not allowed here").into());
    }
    let size = parse_hex(&a[2])?;
    let args = a[3..].iter().map(|s| parse_hex(s)).collect::<Result<Vec<_>>>()?;
    let r = transport::exchange(dev, cmd, size, &args)?;
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
        .map(|s| keymap::by_name(s).or_else(|| s.parse().ok()).ok_or_else(|| format!("unknown key: {s}").into()))
        .collect()
}

fn key_label(id: u8) -> String {
    format!("{:>3} {:<22}", id, keymap::name(id).unwrap_or("?"))
}


fn actuation(dev: &HidTransport, profile: u8, keys: &[u8]) -> Result<String> {
    let mut out = String::new();
    for layer in [Layer::Normal, Layer::Hypershift] {
        for &key in keys {
            let r = transport::exchange(
                dev,
                analog::GET_KEY_ASSIGNMENT,
                analog::KEY_ASSIGNMENT_SIZE,
                &[profile, key, layer as u8],
            )?;
            let a = analog::parse(r.data()).ok_or_else(|| format!("key {key}: short reply {}", hex(r.data())))?;
            let _ = writeln!(
                out,
                "{:?} {} low {:3} ({:.2} mm)  high {:3} ({:.2} mm)  fn {:02X} [{}]",
                layer,
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

fn dump(dev: &HidTransport, a: &[String]) -> Result<()> {
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

fn set_mode(dev: &HidTransport, a: &[String]) -> Result<String> {
    match a.first().map(String::as_str) {
        Some("0") => control::set_hardware_mode(dev),
        Some("3") => control::set_driver_mode(dev),
        _ => return Err(USAGE.into()),
    }?;
    let r = transport::exchange(dev, Command::new(0x00, 0x84), 2, &[])?;
    Ok(format!("mode {}\n", hex(&r.args[..2])))
}

/// Read-modify-write of one key's thresholds in the normal layer; the mapping is kept as is.
fn actuate(dev: &HidTransport, a: &[String]) -> Result<String> {
    let [profile, key, low, high] = a else {
        return Err(USAGE.into());
    };
    let profile: u8 = profile.parse().map_err(|_| "profile must be a number")?;
    let key = keys_arg(std::slice::from_ref(key))?[0];
    let low: u8 = low.parse().map_err(|_| "low must be 0..=255")?;
    let high: u8 = high.parse().map_err(|_| "high must be 0..=255")?;

    let read = |dev: &HidTransport| actuation::read_key(dev, profile, key);
    let before = read(dev)?;
    let after = analog::KeyAssignment { threshold_low: low, threshold_high: high, ..before.clone() };
    actuation::write_key(dev, &after)?;
    let now = read(dev)?;
    Ok(format!(
        "before {}\nafter  {}\n",
        hex(&analog::set_args(&before)),
        hex(&analog::set_args(&now))
    ))
}

/// Prints raw input reports from the vendor collections of MI_01 for a few seconds.
fn open_lamps(api: &HidApi) -> Result<HidDevice> {
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == 0x59)
        .ok_or("LampArray collection not found")?;
    Ok(info.open_device(api)?)
}

fn lamp_bytes(a: &[String]) -> Result<Vec<u8>> {
    Ok(a.iter().map(|s| s.parse::<u8>()).collect::<std::result::Result<_, _>>()?)
}

fn lamp_auto(api: &HidApi, a: &[String]) -> Result<()> {
    let &[on @ (0 | 1)] = lamp_bytes(a)?.as_slice() else {
        return Err(USAGE.into());
    };
    let spec = devices::by_pid(PID).ok_or("device description missing")?;
    Ok(hid::set_autonomous(api, spec, on == 1)?)
}

fn lamp_fill(api: &HidApi, a: &[String]) -> Result<()> {
    let bytes = lamp_bytes(a)?;
    let dev = open_lamps(api)?;
    let [last_lo, last_hi] = (lamp_count(&dev)? - 1).to_le_bytes();
    // LampRangeUpdateReport: flags (1 = update complete), first and last lamp id, RGB.
    let &[r, g, b] = bytes.as_slice() else {
        return Err(USAGE.into());
    };
    Ok(dev.send_feature_report(&[5, 1, 0, 0, last_lo, last_hi, r, g, b])?)
}

fn lamp_count(dev: &HidDevice) -> Result<u16> {
    let mut r = [0u8; 23];
    r[0] = 1;
    dev.get_feature_report(&mut r)?;
    Ok(u16::from_le_bytes([r[1], r[2]]))
}

fn lamps(api: &HidApi) -> Result<()> {
    let dev = open_lamps(api)?;
    let mut buf = [0u8; 4096];
    let n = dev.get_report_descriptor(&mut buf)?;
    println!("descriptor ({n} bytes)");
    for line in buf[..n].chunks(16) {
        println!("{}", hex(line));
    }
    let feature = |id: u8, len: usize| -> Result<Vec<u8>> {
        let mut r = vec![0u8; len + 1];
        r[0] = id;
        let n = dev.get_feature_report(&mut r).map_err(|e| format!("report {id}: {e}"))?;
        Ok(r[1..n].to_vec())
    };
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let a = feature(1, 22)?;
    let count = u16_at(&a, 0);
    println!(
        "lamps {count}, box {}x{}x{} um, kind {}, min update {} us",
        u32_at(&a, 2),
        u32_at(&a, 6),
        u32_at(&a, 10),
        u32_at(&a, 14),
        u32_at(&a, 18)
    );
    println!("autonomous {}", hex(&feature(6, 1)?));
    println!("vendor 07  {}", hex(&feature(7, 63)?));
    for id in 0..count {
        let [lo, hi] = id.to_le_bytes();
        dev.send_feature_report(&[2, lo, hi]).map_err(|e| format!("report 2: {e}"))?;
        let l = feature(3, 27)?;
        println!(
            "lamp {:3} pos {:6} {:6} {:6} latency {:5} purposes {:X} levels {} {} {} prog {} key {:02X}",
            u16_at(&l, 0),
            u32_at(&l, 2),
            u32_at(&l, 6),
            u32_at(&l, 10),
            u32_at(&l, 14),
            u32_at(&l, 18),
            l[22],
            l[23],
            l[24],
            l[25],
            l[26]
        );
    }
    Ok(())
}

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

fn raw_set(dev: &HidTransport, a: &[String]) -> Result<String> {
    if a.len() < 3 {
        return Err(USAGE.into());
    }
    let cmd = Command::new(parse_hex(&a[0])?, parse_hex(&a[1])?);
    let size = parse_hex(&a[2])?;
    let args = a[3..].iter().map(|s| parse_hex(s)).collect::<Result<Vec<_>>>()?;
    // Mode 0x01 reboots into the bootloader (re-enumerates as 1532:110E).
    if cmd == Command::new(0x00, 0x04) && !matches!(args.first(), Some(0x00 | 0x02 | 0x03)) {
        return Err("device mode other than 0, 2 or 3 is refused".into());
    }
    // Serial number, factory config reset and calibration live in config, which Synapse never writes.
    if cmd == Command::new(0x00, 0x02)
        || cmd == Command::new(0xFE, 0x28)
        || (cmd == Command::new(0x00, 0x0B) && args.starts_with(&[0x01, 0x01]))
    {
        return Err(format!("{cmd} rewrites the factory config and is refused").into());
    }
    let r = transport::exchange(dev, cmd, size, &args)?;
    Ok(format!("{cmd} ok, data: {}\n", hex(r.data())))
}
