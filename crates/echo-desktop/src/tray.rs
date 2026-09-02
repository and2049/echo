//! The tray icon that stands in for the window while it is closed: created when the window
//! closes to the tray, dropped when the window reopens, so it is never up alongside a window.
//! Windows uses tray-icon, whose message-only window is pumped by gpui's own message loop on
//! the main thread. Linux registers a StatusNotifierItem over D-Bus via ksni, which fails
//! without a tray host (stock GNOME) so the caller can quit instead of stranding a headless
//! process. macOS has no tray: the Dock keeps the app and `on_reopen` brings the window back.

use tokio::sync::mpsc::UnboundedSender;

/// What the tray menu, the icon click and a second launch's knock ask of the running app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    Quit,
}

pub struct TrayLabels {
    pub show: String,
    pub quit: String,
}

pub struct Tray {
    _handle: platform::Handle,
}

impl Tray {
    /// Puts the icon up with a Show / Quit menu; dropping the result takes it down again.
    pub async fn create(
        tx: UnboundedSender<TrayEvent>,
        labels: TrayLabels,
    ) -> anyhow::Result<Self> {
        let handle = platform::create(tx, labels).await?;
        Ok(Self { _handle: handle })
    }
}

#[cfg(not(target_os = "macos"))]
fn icon_rgba() -> (u32, u32, Vec<u8>) {
    let image = image::load_from_memory(include_bytes!("../../../icons/32x32.png"))
        .expect("bundled tray icon decodes")
        .into_rgba8();
    (image.width(), image.height(), image.into_raw())
}

#[cfg(target_os = "linux")]
fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|px| [px[3], px[0], px[1], px[2]])
        .collect()
}

#[cfg(windows)]
mod platform {
    use super::{TrayEvent, TrayLabels};
    use tokio::sync::mpsc::UnboundedSender;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    pub type Handle = tray_icon::TrayIcon;

    pub async fn create(
        tx: UnboundedSender<TrayEvent>,
        labels: TrayLabels,
    ) -> anyhow::Result<Handle> {
        let show = MenuItem::new(labels.show, true, None);
        let quit = MenuItem::new(labels.quit, true, None);
        let menu = Menu::new();
        menu.append_items(&[&show, &quit])?;
        let (width, height, rgba) = super::icon_rgba();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("echo")
            .with_icon(Icon::from_rgba(rgba, width, height)?)
            .build()?;
        let click_tx = tx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                click_tx.send(TrayEvent::Show).ok();
            }
        }));
        let (show_id, quit_id) = (show.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let event = if event.id == show_id {
                TrayEvent::Show
            } else if event.id == quit_id {
                TrayEvent::Quit
            } else {
                return;
            };
            tx.send(event).ok();
        }));
        Ok(tray)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::time::Duration;

    use super::{TrayEvent, TrayLabels};
    use ksni::menu::StandardItem;
    use ksni::{Icon, MenuItem, TrayMethods};
    use tokio::sync::mpsc::UnboundedSender;

    pub struct Item {
        tx: UnboundedSender<TrayEvent>,
        labels: TrayLabels,
        icon: Icon,
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "echo".into()
        }

        fn title(&self) -> String {
            "echo".into()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            vec![self.icon.clone()]
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.tx.send(TrayEvent::Show).ok();
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let item = |label: &str, event: TrayEvent| {
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |item: &mut Self| {
                        item.tx.send(event).ok();
                    }),
                    ..Default::default()
                }
                .into()
            };
            vec![
                item(&self.labels.show, TrayEvent::Show),
                item(&self.labels.quit, TrayEvent::Quit),
            ]
        }
    }

    pub struct Handle(ksni::Handle<Item>);

    impl Drop for Handle {
        fn drop(&mut self) {
            drop(self.0.shutdown());
        }
    }

    pub async fn create(
        tx: UnboundedSender<TrayEvent>,
        labels: TrayLabels,
    ) -> anyhow::Result<Handle> {
        let (width, height, rgba) = super::icon_rgba();
        let item = Item {
            tx,
            labels,
            icon: Icon {
                width: width as i32,
                height: height as i32,
                data: super::argb(&rgba),
            },
        };
        let handle = tokio::time::timeout(Duration::from_secs(3), item.spawn()).await??;
        Ok(Handle(handle))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{TrayEvent, TrayLabels};
    use tokio::sync::mpsc::UnboundedSender;

    pub type Handle = ();

    pub async fn create(
        _tx: UnboundedSender<TrayEvent>,
        _labels: TrayLabels,
    ) -> anyhow::Result<Handle> {
        Err(anyhow::anyhow!("the Dock stands in for a tray on macOS"))
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    #[test]
    fn bundled_icon_is_32px_rgba() {
        let (width, height, rgba) = super::icon_rgba();
        assert_eq!((width, height), (32, 32));
        assert_eq!(rgba.len(), 32 * 32 * 4);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn argb_moves_alpha_first() {
        assert_eq!(
            super::argb(&[1, 2, 3, 4, 5, 6, 7, 8]),
            [4, 1, 2, 3, 8, 5, 6, 7]
        );
    }
}
