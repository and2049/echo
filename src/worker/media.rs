use crate::events::AppEvent;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum MediaUpdate {
    Metadata {
        title: String,
        artist: String,
        album: String,
        duration_ms: u32,
        cover_url: Option<String>,
    },
    Playback(bool, u32),
}

pub fn spawn_media_thread(
    mut rx: mpsc::Receiver<MediaUpdate>,
    app_tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || {
        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        #[cfg(target_os = "windows")]
        let (hwnd, _dummy_window) = {
            let dummy_window = match windows::DummyWindow::new() {
                Ok(w) => w,
                Err(_) => return, // Gracefully fallback if HWND creation fails
            };
            let handle = Some(dummy_window.handle.0.cast());
            (handle, dummy_window)
        };

        let config = PlatformConfig {
            dbus_name: "echo_player",
            display_name: "Echo Player",
            hwnd,
        };

        let mut controls = match MediaControls::new(config) {
            Ok(c) => c,
            Err(_) => return, // Gracefully fallback if OS controls not supported
        };

        let app_tx_clone = app_tx.clone();
        let current_playing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let playing_clone = current_playing.clone();

        let _ = controls.attach(move |e| {
            let is_playing = playing_clone.load(std::sync::atomic::Ordering::Relaxed);
            match e {
                MediaControlEvent::Play => {
                    if !is_playing {
                        let _ = app_tx_clone.blocking_send(AppEvent::TogglePlayback(false));
                    }
                }
                MediaControlEvent::Pause => {
                    if is_playing {
                        let _ = app_tx_clone.blocking_send(AppEvent::TogglePlayback(true));
                    }
                }
                MediaControlEvent::Toggle => {
                    let _ = app_tx_clone.blocking_send(AppEvent::TogglePlayback(is_playing));
                }
                MediaControlEvent::Next => {
                    let _ = app_tx_clone.blocking_send(AppEvent::NextTrack { current_track_id: None });
                }
                MediaControlEvent::Previous => {
                    let _ = app_tx_clone.blocking_send(AppEvent::PreviousTrack { current_track_id: None });
                }
                _ => {}
            }
        });

        let mut last_title = String::new();
        loop {
            while let Ok(update) = rx.try_recv() {
                match update {
                    MediaUpdate::Metadata { title, artist, album, duration_ms, cover_url } => {
                        if title != last_title {
                            last_title = title.clone();
                            let duration = Some(Duration::from_millis(duration_ms as u64));
                            let mut meta = MediaMetadata {
                                title: Some(&title),
                                artist: Some(&artist),
                                album: Some(&album),
                                duration,
                                cover_url: cover_url.as_deref(),
                            };
                            
                            let _ = controls.set_metadata(meta);
                        }
                    }
                    MediaUpdate::Playback(is_playing, progress_ms) => {
                        current_playing.store(is_playing, std::sync::atomic::Ordering::Relaxed);
                        let progress = Some(MediaPosition(Duration::from_millis(progress_ms as u64)));
                        if is_playing {
                            let _ = controls.set_playback(MediaPlayback::Playing { progress });
                        } else {
                            let _ = controls.set_playback(MediaPlayback::Paused { progress });
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(100));

            #[cfg(target_os = "windows")]
            windows::pump_event_queue();
        }
    });
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub mod windows {
    use std::io::Error;
    use std::mem;

    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor,
        IsDialogMessageW, PeekMessageW, RegisterClassExW, TranslateMessage, GA_ROOT, MSG,
        PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_QUIT, WNDCLASSEXW,
    };

    pub struct DummyWindow {
        pub handle: HWND,
    }

    impl DummyWindow {
        pub fn new() -> Result<DummyWindow, String> {
            let class_name = w!("SimpleTray");

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

                // Ignore if it fails, it might already be registered
                let _ = RegisterClassExW(&raw const wnd_class);

                let handle = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    w!(""),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    instance,
                    None,
                )
                .map_err(|e| format!("Failed to create window: {e}"))?;

                if handle.0.is_null() {
                    Err(format!("Window creation failed"))
                } else {
                    Ok(DummyWindow { handle })
                }
            }
        }
        
        extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
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

    pub fn pump_event_queue() -> bool {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            let mut has_message = PeekMessageW(&raw mut msg, None, 0, 0, PM_REMOVE).as_bool();
            while msg.message != WM_QUIT && has_message {
                if !IsDialogMessageW(GetAncestor(msg.hwnd, GA_ROOT), &raw const msg).as_bool() {
                    let _ = TranslateMessage(&raw const msg);
                    let _ = DispatchMessageW(&raw const msg);
                }
                has_message = PeekMessageW(&raw mut msg, None, 0, 0, PM_REMOVE).as_bool();
            }
            msg.message == WM_QUIT
        }
    }
}
