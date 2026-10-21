use crate::sys::{self, Channel, Remote};
use hidmaestro_client::{Controller, Writer};
use std::io;
use std::os::windows::io::{AsHandle, BorrowedHandle, OwnedHandle};
use std::sync::Mutex;
use windows_sys::Win32::System::Memory::{FILE_MAP_READ, FILE_MAP_WRITE};
use windows_sys::Win32::System::Threading::*;

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
            if let Err(e) = endpoint.channel.stop.set() {
                eprintln!("gamepad stop: {e}");
            }
            STOPPING.lock().unwrap_or_else(|e| e.into_inner()).push(endpoint);
        }
    }
}

impl Process {
    pub(super) fn handle(&self) -> BorrowedHandle<'_> {
        self.inner.as_ref().unwrap().handle.as_handle()
    }

    fn channel(&self) -> &Channel {
        &self.inner.as_ref().unwrap().channel
    }

    #[cfg(test)]
    fn close(&self) -> io::Result<()> {
        self.channel().stop.set()?;
        if sys::wait(self.handle(), 120_000)? {
            self.inner.as_ref().unwrap().result()
        } else {
            Err(io::Error::new(io::ErrorKind::TimedOut, "gamepad helper cleanup"))
        }
    }
}

impl Endpoint {
    fn result(&self) -> io::Result<()> {
        self.channel.result()?;
        let code = sys::exit_code(self.handle.as_handle())?;
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
        match sys::wait(stopping[i].handle.as_handle(), 0) {
            Ok(true) => {
                if let Err(e) = stopping.swap_remove(i).result() { error.get_or_insert(e); }
            }
            Ok(false) => i += 1,
            Err(e) => {
                error.get_or_insert(e);
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

// The helper's handle transfer is a cross-process contract no type can check.
#[allow(unsafe_code)]
fn launch_inner(exe: &std::path::Path) -> io::Result<(Writer, Process)> {
    if cleanup()? {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "the previous gamepad helper is still stopping; apply again after cleanup"));
    }
    let process = spawn(exe)?;
    process.channel().wait_ready(process.handle())?;
    // SAFETY: the channel's peer is the helper spawned above, whose `helper` duplicated these
    // handles into this process, stopped its writer and revokes them only until Attached.
    let [a, b, c] = unsafe { process.channel().attach() }?;
    let writer = Writer::from_handles([a?, b?, c?])?;
    Ok((writer, process))
}

fn spawn(exe: &std::path::Path) -> io::Result<Process> {
    let channel = Channel::create()?;
    let args = format!("--gamepad-helper {} {}", std::process::id(), channel.name);
    let handle = sys::run_elevated(exe, &args)?;
    Ok(Process { inner: Some(Endpoint { handle, channel }) })
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
        let process = sys::run_elevated(&std::env::current_exe()?, &args)?;
        if !sys::wait(process.as_handle(), INFINITE)? {
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

fn duplicate_writer<'p>(writer: &Writer, parent: BorrowedHandle<'p>) -> io::Result<[Remote<'p>; 3]> {
    let mut remote = Vec::with_capacity(3);
    for (i, source) in writer.handles().into_iter().enumerate() {
        let access = if i == 0 { FILE_MAP_READ | FILE_MAP_WRITE } else { EVENT_MODIFY_STATE };
        match sys::duplicate_into(source, parent, access) {
            Ok(target) => remote.push(target),
            Err(error) => {
                revoke(&remote);
                return Err(error);
            }
        }
    }
    Ok([remote[0], remote[1], remote[2]])
}

fn revoke(handles: &[Remote<'_>]) {
    for handle in handles {
        // No parent reader has been signaled yet, so only this process owns these values.
        handle.close();
    }
}

pub(super) fn helper(args: &[String]) -> io::Result<()> {
    let (pid, name) = endpoint(args)?;
    let channel = Channel::open(name)?;
    let result = (|| {
        let parent = sys::open_process(pid, PROCESS_DUP_HANDLE | PROCESS_SYNCHRONIZE)?;
        let parent = parent.as_handle();
        if !matches!(sys::wait(parent, 0), Ok(false)) {
            return Err(io::Error::other("parent exited before setup"));
        }
        let mut controller = Controller::create()?;
        let session = (|| {
            // The parent drops its writer before Stop; a crashed parent closes all
            // handles before its process signals. Both precede controller cleanup.
            let writer = controller.take_writer()?;
            let handles = duplicate_writer(&writer, parent)?;
            drop(writer);
            channel.reply(handles.map(|h| h.value()), None);
            if let Err(e) = channel.signal_ready() {
                revoke(&handles);
                return Err(e);
            }
            match sys::wait_any(&[channel.attached.handle(), channel.stop.handle(), parent], INFINITE) {
                Ok(Some(0)) => {}
                attach => {
                    revoke(&handles);
                    return attach.map(drop);
                }
            }
            if let Err(error) = sys::wait_any(&[channel.stop.handle(), parent], INFINITE) {
                // Keep the delegated writer contract even if the control wait fails.
                let _ = sys::wait(parent, INFINITE);
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
            assert!(channel.stop.wait(5000).unwrap());
            assert!(!sys::mapping_exists(&format!("{}-Input", channel.name)), "writer must close before Stop");
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
        let handle = sys::open_process(child.id(), PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION).unwrap();
        channel.wait_ready(handle.as_handle()).unwrap();
        let mapping = sys::create_mapping(Some(&format!("{}-Input", channel.name)), 362).unwrap();
        let (input, companion) = (sys::event().unwrap(), sys::event().unwrap());
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
                if sys::xinput_buttons(slot) == Some(4096) {
                    active = Some(slot);
                }
            }
            let slot = active.expect("direct A press");
            // Exceed the driver's 500-read stale-section recovery threshold.
            for _ in 0..1100 {
                assert_eq!(sys::xinput_buttons(slot), Some(4096));
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
            println!("HELPER_READY={}", sys::process_id(process.handle()));
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
        let helper = sys::open_process(pid, PROCESS_SYNCHRONIZE).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(sys::wait(helper.as_handle(), 120_000).unwrap());
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
    #[allow(unsafe_code)]
    fn duplicated_writer_handles_support_direct_submission() {
        let mapping = sys::create_mapping(None, 362).unwrap();
        let (input, companion) = (sys::event().unwrap(), sys::event().unwrap());
        let events = [input.try_clone().unwrap(), companion.try_clone().unwrap()];
        let writer = Writer::from_handles([mapping, input, companion]).unwrap();
        // This process plays the helper's part and duplicates the handles to itself.
        let channel = Channel::create().unwrap();
        let peer = Channel::open(&channel.name).unwrap();
        peer.reply(duplicate_writer(&writer, sys::current_process()).unwrap().map(|h| h.value()), None);
        drop(writer);
        peer.signal_ready().unwrap();
        let own = sys::open_process(std::process::id(), PROCESS_SYNCHRONIZE).unwrap();
        channel.wait_ready(own.as_handle()).unwrap();
        // SAFETY: the peer above duplicated each handle into this process and never revokes it.
        let [a, b, c] = unsafe { channel.attach() }.unwrap();
        let mut writer = Writer::from_handles([a.unwrap(), b.unwrap(), c.unwrap()]).unwrap();
        writer.submit(&hidmaestro_client::State::default()).unwrap();
        for event in events {
            assert!(sys::wait(event.as_handle(), 0).unwrap());
        }
    }
}
