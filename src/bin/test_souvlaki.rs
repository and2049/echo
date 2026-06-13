use souvlaki::{MediaControls, PlatformConfig, MediaMetadata};

fn main() {
    println!("Testing souvlaki...");

    #[cfg(target_os = "windows")]
    let (hwnd, _dummy_window) = {
        let dummy_window = match windows::DummyWindow::new() {
            Ok(w) => w,
            Err(_) => return,
        };
        let handle = Some(dummy_window.handle.0.cast());
        (handle, dummy_window)
    };

    #[cfg(not(target_os = "windows"))]
    let hwnd = None;

    let config = PlatformConfig {
        dbus_name: "test_player",
        display_name: "Test Player",
        hwnd,
    };

    let mut controls = MediaControls::new(config).unwrap();

    let meta = MediaMetadata {
        title: Some("Test Title"),
        artist: Some("Test Artist"),
        album: Some("Test Album"),
        duration: None,
        cover_url: Some("https://i.scdn.co/image/ab67616d0000b273b5e4c278fb12b596fb638eb6"),
    };

    println!("Setting metadata with URL: {:?}", meta.cover_url);
    match controls.set_metadata(meta) {
        Ok(_) => println!("Success HTTP!"),
        Err(e) => println!("Error setting HTTP metadata: {:?}", e),
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub mod windows {
    use std::mem;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW,
        WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
    };
    use windows::core::w;

    pub struct DummyWindow {
        pub handle: HWND,
    }

    impl DummyWindow {
        pub fn new() -> Result<DummyWindow, String> {
            let class_name = w!("SimpleTrayTest");

            unsafe {
                let instance = GetModuleHandleW(None)
                    .map_err(|e| format!("Getting module handle failed: {e}"))?;

                let wnd_class = WNDCLASSEXW {
                    cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                    hInstance: instance.into(),
                    lpszClassName: class_name,
                    lpfnWndProc: Some(Self::wnd_proc),
                    ..Default::default()
                };

                let _ = RegisterClassExW(&raw const wnd_class);

                let handle = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    w!(""),
                    WINDOW_STYLE::default(),
                    0, 0, 0, 0, None, None, instance, None,
                ).map_err(|e| format!("Failed to create window: {e}"))?;

                if handle.0.is_null() {
                    Err("Window creation failed".to_string())
                } else {
                    Ok(DummyWindow { handle })
                }
            }
        }

        extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    impl Drop for DummyWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.handle);
            }
        }
    }
}
