use super::ipc::{Channel, owned};
use crate::autostart::wide;
use hidmaestro_client::{Controller, Writer};
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{DUPLICATE_CLOSE_SOURCE, DuplicateHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Memory::{FILE_MAP_READ, FILE_MAP_WRITE};
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub(super) struct Process {
    pub(super) handle: OwnedHandle,
    channel: Channel,
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("gamepad cleanup: {e}");
        }
    }
}

impl Process {
    fn close(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.channel.stop.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 120_000) } {
            WAIT_OBJECT_0 => {
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
            WAIT_TIMEOUT => Err(io::Error::new(io::ErrorKind::TimedOut, "gamepad helper cleanup")),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

pub(super) fn launch() -> Result<(Writer, Process), String> {
    (|| launch_inner(&std::env::current_exe()?))().map_err(|e| format!("gamepad helper: {e}"))
}

fn launch_inner(exe: &std::path::Path) -> io::Result<(Writer, Process)> {
    let process = spawn(exe)?;
    process.channel.wait_ready(process.handle.as_raw_handle())?;
    let raw = process.channel.result()?;
    if unsafe { SetEvent(process.channel.attached.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let [a, b, c] = raw.map(|h| owned(h as HANDLE));
    // The helper duplicated these handles into this process and stopped its writer.
    let writer = unsafe { Writer::from_handles([a?, b?, c?]) }?;
    Ok((writer, process))
}

fn spawn(exe: &std::path::Path) -> io::Result<Process> {
    let channel = Channel::create()?;
    use std::os::windows::ffi::OsStrExt;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let verb = wide("runas");
    let args = wide(&format!("--gamepad-helper {} {}", std::process::id(), channel.name));
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
    Ok(Process {
        handle: owned(info.hProcess)?,
        channel,
    })
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
            let writer = unsafe { controller.take_writer() }?;
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

    fn app_exe() -> std::path::PathBuf {
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("easyrazer.exe")
    }

    #[test]
    #[ignore = "explicit hardware test cancels startup before importing handles; build easyrazer first"]
    fn cancelled_attachment_revokes_remote_handles() {
        let process = spawn(&app_exe()).unwrap();
        process.channel.wait_ready(process.handle.as_raw_handle()).unwrap();
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
            println!("HELPER_READY={}", unsafe { GetProcessId(process.handle.as_raw_handle()) });
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
        // This anonymous mapping is private to the test and has only one writer.
        let writer = unsafe { Writer::from_handles([mapping, input, companion]) }.unwrap();
        let handles = duplicate_writer(&writer, unsafe { GetCurrentProcess() }).unwrap();
        drop(writer);
        let handles = handles.map(|h| owned(h as HANDLE).unwrap());
        // Ownership was transferred above and the original writer has stopped.
        let mut writer = unsafe { Writer::from_handles(handles) }.unwrap();
        writer.submit(&hidmaestro_client::State::default()).unwrap();
        for event in events {
            assert_eq!(unsafe { WaitForSingleObject(event.as_raw_handle(), 0) }, WAIT_OBJECT_0);
        }
    }
}
