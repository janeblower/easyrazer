use super::ipc::{Channel, owned};
use crate::autostart::wide;
use hidmaestro_client::{Controller, Writer};
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::null_mut;
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{DUPLICATE_CLOSE_SOURCE, DuplicateHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Memory::{FILE_MAP_READ, FILE_MAP_WRITE};
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub(super) struct Process {
    inner: Option<Endpoint>,
}

struct Endpoint {
    handle: OwnedHandle,
    channel: Channel,
}

// Retain IPC until exit without waiting under the device or keyboard-engine locks.
static STOPPING: Mutex<Vec<Endpoint>> = Mutex::new(Vec::new());
static LAUNCHING: Mutex<()> = Mutex::new(());

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(endpoint) = self.inner.take() {
            if unsafe { SetEvent(endpoint.channel.stop.as_raw_handle()) } == 0 {
                eprintln!("gamepad stop: {}", io::Error::last_os_error());
            }
            STOPPING.lock().unwrap_or_else(|e| e.into_inner()).push(endpoint);
        }
    }
}

impl Process {
    pub(super) fn handle(&self) -> HANDLE {
        self.inner.as_ref().unwrap().handle.as_raw_handle()
    }

    fn channel(&self) -> &Channel {
        &self.inner.as_ref().unwrap().channel
    }

    #[cfg(test)]
    fn close(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.channel().stop.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        match unsafe { WaitForSingleObject(self.handle(), 120_000) } {
            WAIT_OBJECT_0 => self.inner.as_ref().unwrap().result(),
            WAIT_TIMEOUT => Err(io::Error::new(io::ErrorKind::TimedOut, "gamepad helper cleanup")),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Endpoint {
    fn result(&self) -> io::Result<()> {
        self.channel.result()?;
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if code != 0 {
            return Err(io::Error::other(format!("helper exited with code {code}")));
        }
        Ok(())
    }
}

pub(super) fn cleanup() -> io::Result<bool> {
    let mut stopping = STOPPING.lock().unwrap_or_else(|e| e.into_inner());
    let mut error = None;
    let mut i = 0;
    while i < stopping.len() {
        match unsafe { WaitForSingleObject(stopping[i].handle.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => {
                if let Err(e) = stopping.swap_remove(i).result() { error.get_or_insert(e); }
            }
            WAIT_TIMEOUT => i += 1,
            _ => {
                error.get_or_insert_with(io::Error::last_os_error);
                i += 1;
            }
        }
    }
    if let Some(e) = error { Err(e) } else { Ok(!stopping.is_empty()) }
}

pub(super) fn launch() -> std::sync::mpsc::Receiver<Result<(Writer, Process), String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let _launch = LAUNCHING.try_lock().map_err(|_| "A gamepad helper is already starting. Apply again after startup finishes.".to_string())?;
            if !super::drivers::installed()? {
                return Err("Install HIDMaestro v2.0.0 from the Gamepad tab first.".into());
            }
            (|| launch_inner(&std::env::current_exe()?))().map_err(|e| format!("gamepad helper: {e}"))
        })();
        let _ = tx.send(result);
        crate::wake::wake();
    });
    rx
}

fn launch_inner(exe: &std::path::Path) -> io::Result<(Writer, Process)> {
    if cleanup()? {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "the previous gamepad helper is still stopping; apply again after cleanup"));
    }
    let process = spawn(exe)?;
    process.channel().wait_ready(process.handle())?;
    let raw = process.channel().result()?;
    if unsafe { SetEvent(process.channel().attached.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let [a, b, c] = raw.map(|h| owned(h as HANDLE));
    // The helper duplicated these handles into this process and stopped its writer.
    let writer = Writer::from_handles([a?, b?, c?])?;
    Ok((writer, process))
}

fn spawn(exe: &std::path::Path) -> io::Result<Process> {
    let channel = Channel::create()?;
    let args = format!("--gamepad-helper {} {}", std::process::id(), channel.name);
    let handle = elevate(exe, &args)?;
    Ok(Process { inner: Some(Endpoint { handle, channel }) })
}

fn elevate(exe: &std::path::Path, args: &str) -> io::Result<OwnedHandle> {
    use std::os::windows::ffi::OsStrExt;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let verb = wide("runas");
    let args = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: SW_HIDE,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    owned(info.hProcess)
}

pub(super) fn install() -> Result<(), String> {
    driver_action("--gamepad-install")
}

pub(super) fn uninstall() -> Result<(), String> {
    let _launch = LAUNCHING.try_lock().map_err(|_| "A gamepad helper is starting. Wait for startup to finish.".to_string())?;
    if cleanup().map_err(|e| e.to_string())? {
        return Err("The gamepad is still stopping. Retry after cleanup.".into());
    }
    driver_action("--gamepad-uninstall")
}

fn driver_action(action: &str) -> Result<(), String> {
    let run = || -> io::Result<()> {
        let channel = Channel::create()?;
        let args = format!("{action} {} {}", std::process::id(), channel.name);
        let process = elevate(&std::env::current_exe()?, &args)?;
        if unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        Endpoint { handle: process, channel }.result()?;
        Ok(())
    };
    run().map_err(|e| e.to_string())
}

pub fn driver_helper(args: &[String], uninstall: bool) -> Result<(), String> {
    let (_, name) = endpoint(args).map_err(|e| e.to_string())?;
    let channel = Channel::open(name).map_err(|e| e.to_string())?;
    let result = if uninstall { super::drivers::remove() } else { super::drivers::deploy() };
    channel.reply([0; 3], result.as_ref().err().map(String::as_str));
    result
}

fn endpoint(args: &[String]) -> io::Result<(u32, &str)> {
    let [pid, name] = args else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "expected parent PID and control channel"));
    };
    let pid = pid
        .parse::<u32>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid parent PID"))?;
    let prefix = format!("Local\\EasyRazerGamepad-{pid}-");
    if !name
        .strip_prefix(&prefix)
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid control channel"));
    }
    Ok((pid, name))
}

fn duplicate_writer(writer: &Writer, parent: HANDLE) -> io::Result<[usize; 3]> {
    let mut remote = [0usize; 3];
    for (i, source) in writer.handles().into_iter().enumerate() {
        let access = if i == 0 { FILE_MAP_READ | FILE_MAP_WRITE } else { EVENT_MODIFY_STATE };
        let mut target = null_mut();
        if unsafe { DuplicateHandle(GetCurrentProcess(), source.as_raw_handle(), parent, &mut target, access, 0, 0) } == 0 {
            let error = io::Error::last_os_error();
            revoke(parent, &remote[..i]);
            return Err(error);
        }
        remote[i] = target as usize;
    }
    Ok(remote)
}

fn revoke(parent: HANDLE, handles: &[usize]) {
    for &handle in handles {
        let mut local = null_mut();
        // No parent reader has been signaled yet, so only this process owns these values.
        if unsafe { DuplicateHandle(parent, handle as HANDLE, GetCurrentProcess(), &mut local, 0, 0, DUPLICATE_CLOSE_SOURCE) } != 0 {
            drop(owned(local));
        }
    }
}

pub(super) fn helper(args: &[String]) -> io::Result<()> {
    let (pid, name) = endpoint(args)?;
    let channel = Channel::open(name)?;
    let result = (|| {
        let parent = owned(unsafe { OpenProcess(PROCESS_DUP_HANDLE | PROCESS_SYNCHRONIZE, 0, pid) })?;
        if unsafe { WaitForSingleObject(parent.as_raw_handle(), 0) } != WAIT_TIMEOUT {
            return Err(io::Error::other("parent exited before setup"));
        }
        let mut controller = Controller::create()?;
        let session = (|| {
            // The parent drops its writer before Stop; a crashed parent closes all
            // handles before its process signals. Both precede controller cleanup.
            let writer = controller.take_writer()?;
            let handles = duplicate_writer(&writer, parent.as_raw_handle())?;
            drop(writer);
            channel.reply(handles, None);
            if let Err(e) = channel.signal_ready() {
                revoke(parent.as_raw_handle(), &handles);
                return Err(e);
            }
            let attach = [channel.attached.as_raw_handle(), channel.stop.as_raw_handle(), parent.as_raw_handle()];
            let rc = unsafe { WaitForMultipleObjects(3, attach.as_ptr(), 0, INFINITE) };
            if rc != WAIT_OBJECT_0 {
                revoke(parent.as_raw_handle(), &handles);
                return if rc == WAIT_OBJECT_0 + 1 || rc == WAIT_OBJECT_0 + 2 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                };
            }
            let waits = [channel.stop.as_raw_handle(), parent.as_raw_handle()];
            let rc = unsafe { WaitForMultipleObjects(2, waits.as_ptr(), 0, INFINITE) };
            if rc != WAIT_OBJECT_0 && rc != WAIT_OBJECT_0 + 1 {
                let error = io::Error::last_os_error();
                // Keep the delegated writer contract even if the control wait fails.
                unsafe {
                    WaitForSingleObject(parent.as_raw_handle(), INFINITE);
                }
                return Err(error);
            }
            Ok(())
        })();
        let cleanup = controller.close();
        session.and(cleanup)
    })();
    channel.reply([0; 3], result.as_ref().err().map(|e| e.to_string()).as_deref());
    let _ = channel.signal_ready();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_start_returns_without_requesting_elevation() {
        let _launch = LAUNCHING.lock().unwrap();
        let pending = launch();
        let result = pending.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(matches!(result, Err(e) if e.contains("already starting")));
    }

    #[test]
    fn slow_cleanup_does_not_block_the_caller() {
        if let Some(name) = std::env::var_os("EASYRAZER_SLOW_CLEANUP") {
            let channel = Channel::open(&name.to_string_lossy()).unwrap();
            channel.signal_ready().unwrap();
            assert_eq!(unsafe { WaitForSingleObject(channel.stop.as_raw_handle(), 5000) }, WAIT_OBJECT_0);
            assert!(unsafe { windows_sys::Win32::System::Memory::OpenFileMappingW(FILE_MAP_READ, 0, wide(&format!("{}-Input", channel.name)).as_ptr()) }.is_null(), "writer must close before Stop");
            std::thread::sleep(std::time::Duration::from_millis(350));
            channel.reply([0; 3], None);
            return;
        }
        let channel = Channel::create().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "gamepad::process::tests::slow_cleanup_does_not_block_the_caller", "--nocapture"])
            .env("EASYRAZER_SLOW_CLEANUP", &channel.name)
            .stdout(std::process::Stdio::null())
            .spawn().unwrap();
        let handle = owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, child.id()) }).unwrap();
        channel.wait_ready(handle.as_raw_handle()).unwrap();
        let mapping = owned(unsafe {
            windows_sys::Win32::System::Memory::CreateFileMappingW(
                windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE, null_mut(),
                windows_sys::Win32::System::Memory::PAGE_READWRITE, 0, 362,
                wide(&format!("{}-Input", channel.name)).as_ptr(),
            )
        }).unwrap();
        let input = owned(unsafe { CreateEventW(null_mut(), 0, 0, null_mut()) }).unwrap();
        let companion = owned(unsafe { CreateEventW(null_mut(), 0, 0, null_mut()) }).unwrap();
        let writer = Writer::from_handles([mapping, input, companion]).unwrap();
        let mut runtime = super::super::Runtime::new();
        runtime.config.enabled = true;
        runtime.session = Some(super::super::Session {
            writer, previous: hidmaestro_client::State::default(),
            process: Process { inner: Some(Endpoint { handle, channel }) },
        });
        let start = std::time::Instant::now();
        runtime.configure(1, super::super::Config::default(), false);
        let elapsed = start.elapsed();
        assert!(!runtime.input().running);
        assert!(cleanup().unwrap());
        assert_eq!(launch_inner(std::path::Path::new("must-not-launch.exe")).err().unwrap().kind(), io::ErrorKind::WouldBlock);
        assert!(child.wait().unwrap().success());
        assert!(!cleanup().unwrap());
        assert!(elapsed < std::time::Duration::from_millis(100), "cleanup blocked for {elapsed:?}");
    }

    fn app_exe() -> std::path::PathBuf {
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("easyrazer.exe")
    }

    #[test]
    #[ignore = "explicit hardware test cancels startup before importing handles; build easyrazer first"]
    fn cancelled_attachment_revokes_remote_handles() {
        let process = spawn(&app_exe()).unwrap();
        process.channel().wait_ready(process.handle()).unwrap();
        process.close().unwrap();
        let (writer, process) = launch_inner(&app_exe()).unwrap();
        drop(writer);
        process.close().unwrap();
    }

    #[test]
    #[ignore = "explicit hardware test launches the elevated helper; build easyrazer first"]
    fn helper_lifecycle_and_direct_xinput() {
        if std::env::var_os("EASYRAZER_PROBE_CHILD").is_some() {
            let mut active = None;
            for slot in 0..4 {
                let mut state = windows_sys::Win32::UI::Input::XboxController::XINPUT_STATE::default();
                if unsafe { windows_sys::Win32::UI::Input::XboxController::XInputGetState(slot, &mut state) } == 0 && state.Gamepad.wButtons == 4096 {
                    active = Some(slot);
                }
            }
            let slot = active.expect("direct A press");
            // Exceed the driver's 500-read stale-section recovery threshold.
            for _ in 0..1100 {
                let mut state = windows_sys::Win32::UI::Input::XboxController::XINPUT_STATE::default();
                assert_eq!(unsafe { windows_sys::Win32::UI::Input::XboxController::XInputGetState(slot, &mut state) }, 0);
                assert_eq!(state.Gamepad.wButtons, 4096);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            println!("buttons=4096");
            return;
        }
        for _ in 0..2 {
            let (mut writer, process) = launch_inner(&app_exe()).unwrap();
            writer
                .submit(&hidmaestro_client::State {
                    buttons: hidmaestro_client::Buttons::A,
                    ..Default::default()
                })
                .unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "gamepad::process::tests::helper_lifecycle_and_direct_xinput",
                    "--ignored",
                    "--nocapture",
                ])
                .env("EASYRAZER_PROBE_CHILD", "1")
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("buttons=4096"));
            writer.submit(&hidmaestro_client::State::default()).unwrap();
            drop(writer);
            process.close().unwrap();
        }
    }

    #[test]
    #[ignore = "explicit hardware test kills its own parent subprocess; build easyrazer first"]
    fn parent_crash_removes_controller() {
        use std::io::BufRead;
        if std::env::var_os("EASYRAZER_PARENT_CHILD").is_some() {
            let (_writer, process) = launch_inner(&app_exe()).unwrap();
            println!("HELPER_READY={}", unsafe { GetProcessId(process.handle()) });
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "gamepad::process::tests::parent_crash_removes_controller",
                "--ignored",
                "--nocapture",
            ])
            .env("EASYRAZER_PARENT_CHILD", "1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let lines = std::io::BufReader::new(child.stdout.take().unwrap()).lines();
        let pid = lines
            .map_while(Result::ok)
            .find_map(|l| l.strip_prefix("HELPER_READY=").and_then(|p| p.parse().ok()))
            .expect("child helper startup");
        let helper = owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) }).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(unsafe { WaitForSingleObject(helper.as_raw_handle(), 120_000) }, WAIT_OBJECT_0);
        let (writer, process) = launch_inner(&app_exe()).unwrap();
        drop(writer);
        process.close().unwrap();
    }

    #[test]
    fn malformed_endpoint_is_rejected_before_opening_handles() {
        let name = format!("Local\\EasyRazerGamepad-1234-{}", "ab".repeat(32));
        for args in [
            vec![],
            vec!["1234".into()],
            vec!["0".into(), name.clone()],
            vec!["4294967296".into(), name.clone()],
            vec!["1234".into(), "Global\\other".into()],
            vec!["1234".into(), format!("Local\\EasyRazerGamepad-1234-{}", "gg".repeat(32))],
        ] {
            assert_eq!(endpoint(&args).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        }
        assert_eq!(endpoint(&["1234".into(), name.clone()]).unwrap(), (1234, name.as_str()));
    }

    #[test]
    fn duplicated_writer_handles_support_direct_submission() {
        use windows_sys::Win32::System::Memory::*;
        let mapping = owned(unsafe {
            CreateFileMappingW(
                windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                362,
                std::ptr::null(),
            )
        })
        .unwrap();
        let input = owned(unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) }).unwrap();
        let companion = owned(unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) }).unwrap();
        let events = [input.try_clone().unwrap(), companion.try_clone().unwrap()];
        let writer = Writer::from_handles([mapping, input, companion]).unwrap();
        let handles = duplicate_writer(&writer, unsafe { GetCurrentProcess() }).unwrap();
        drop(writer);
        let handles = handles.map(|h| owned(h as HANDLE).unwrap());
        let mut writer = Writer::from_handles(handles).unwrap();
        writer.submit(&hidmaestro_client::State::default()).unwrap();
        for event in events {
            assert_eq!(unsafe { WaitForSingleObject(event.as_raw_handle(), 0) }, WAIT_OBJECT_0);
        }
    }
}
