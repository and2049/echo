use super::*;

impl EchoApp {
    /// Ask GitHub whether a newer release exists.
    ///
    /// The second exception to the "every setting is a `:` command" rule, alongside
    /// [`Self::set_audio_quality`]: `commands::execute` is synchronous and returns an
    /// `Option<AppEvent>`, which cannot express an in-flight network call.
    pub(crate) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        self.start_update_check(false, cx);
    }

    pub(crate) fn maybe_auto_update(&mut self, cx: &mut Context<Self>) {
        use echo_core::update::{self, AutoStep, AutoUpdateInputs};
        let now = std::time::Instant::now();
        match update::auto_step(AutoUpdateInputs {
            dev_build: update::is_dev_build(),
            enabled: self.state.ui.library_config.auto_update,
            startup_elapsed: now.duration_since(self.started_at)
                >= update::AUTO_CHECK_STARTUP_DELAY,
            due: update::auto_check_due(self.last_auto_check, now, update::AUTO_CHECK_INTERVAL),
            busy: matches!(
                self.update_state,
                UpdateState::Checking | UpdateState::Downloading(_)
            ),
            available: matches!(self.update_state, UpdateState::Available(_)),
            pending: self.auto_update_pending,
            installed: self.auto_update_installed,
            ready: matches!(self.update_state, UpdateState::Ready(_)),
        }) {
            AutoStep::Check => {
                self.last_auto_check = Some(now);
                self.start_update_check(true, cx);
            }
            AutoStep::Install => self.install_update(cx),
            AutoStep::Skip | AutoStep::Notify => {}
        }
    }

    fn finish_update_operation(&mut self) {
        if self.auto_update_pending {
            match &self.update_state {
                UpdateState::Failed(error) | UpdateState::Blocked(error) => {
                    use std::io::Write;
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(echo_core::config::debug_log_path("echo-debug-update.log"))
                    {
                        let _ = writeln!(file, "{}", error.replace(['\r', '\n'], " "));
                    }
                }
                UpdateState::Ready(_) => self.auto_update_installed = true,
                _ => {}
            }
            if !matches!(self.update_state, UpdateState::Available(_)) {
                self.auto_update_pending = false;
            }
        }
        if matches!(self.update_state, UpdateState::Ready(_)) {
            self.update_banner_dismissed = false;
        }
    }

    pub(crate) fn restart_updated(&mut self, cx: &mut Context<Self>) {
        if self.restarting {
            return;
        }
        self.restarting = true;
        let executable = self.launch_executable.clone();
        let shell = cx.global_mut::<Shell>();
        let task = shell.instance_task.take();
        let tray_tx = shell.tray_tx.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tokio.spawn(async move {
            if let Some(task) = task {
                task.abort();
                let _ = task.await;
            }
            let result = executable
                .ok_or_else(|| std::io::Error::other("Executable path unavailable"))
                .and_then(|exe| {
                    std::process::Command::new(exe)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .map(|_| ())
                });
            let listener = if result.is_err() {
                match single_instance::claim(&echo_core::config::echo_config_root()) {
                    single_instance::Instance::Primary(Some(listener)) => {
                        Some(listener.serve(tray_tx))
                    }
                    _ => None,
                }
            } else {
                None
            };
            let _ = tx.send((result, listener));
        });
        cx.spawn(async move |this, cx| {
            if let Ok((result, listener)) = rx.await {
                let _ = this.update(cx, |app, cx| {
                    if result.is_ok() {
                        cx.quit();
                    } else {
                        cx.global_mut::<Shell>().instance_task = listener;
                        app.restarting = false;
                        app.state.ui.status_message = Some(
                            views::tr(&app.state, "desktop.settings.updates.restart_failed")
                                .to_string(),
                        );
                        app.state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + Duration::from_secs(5));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn start_update_check(&mut self, automatic: bool, cx: &mut Context<Self>) {
        if matches!(
            self.update_state,
            UpdateState::Checking | UpdateState::Downloading(_)
        ) {
            return;
        }
        self.auto_update_pending = automatic;
        self.update_state = UpdateState::Checking;
        cx.notify();

        // reqwest needs the tokio reactor, so the request runs on the runtime; the result comes
        // back over a oneshot, which is reactor-free and therefore safe to await on GPUI's
        // foreground executor (the same reasoning as the worker-event bridge above).
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tokio.spawn(async move {
            let _ = tx.send(echo_core::update::check().await);
        });

        cx.spawn(async move |this, cx| {
            let outcome = rx.await;
            let _ = this.update(cx, |app: &mut EchoApp, cx| {
                app.update_state = match outcome {
                    Ok(Ok(echo_core::update::Check::UpToDate)) => UpdateState::UpToDate,
                    Ok(Ok(echo_core::update::Check::Available(release))) => {
                        UpdateState::Available(release)
                    }
                    Ok(Err(error)) => UpdateState::from_error(error),
                    Err(_) => UpdateState::Failed(
                        views::tr(&app.state, "desktop.settings.updates.failed").to_string(),
                    ),
                };
                app.finish_update_operation();
                if app.auto_update_pending {
                    app.maybe_auto_update(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Download the release found by [`Self::check_for_updates`] and swap it in.
    pub(crate) fn install_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(release) = self.update_state.clone() else {
            return;
        };
        // Resolved before the download so a read-only install fails immediately.
        let plan = match echo_core::update::plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.update_state = UpdateState::from_error(error);
                self.finish_update_operation();
                cx.notify();
                return;
            }
        };

        self.update_state = UpdateState::Downloading(0);
        cx.notify();

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        self.tokio.spawn(async move {
            let staged = echo_core::update::download(plan, &release, move |percent| {
                let _ = progress_tx.send(percent);
            })
            .await;
            // The swap is blocking filesystem work, but it is a handful of renames — keeping it
            // on the runtime rather than the UI thread is enough.
            let _ = done_tx.send(staged.and_then(echo_core::update::apply));
        });

        cx.spawn(async move |this, cx| {
            // The progress sender is dropped when the download future completes, which ends
            // this loop and lets the outcome through.
            while let Some(percent) = progress_rx.recv().await {
                let applied = this.update(cx, |app: &mut EchoApp, cx| {
                    app.update_state = UpdateState::Downloading(percent);
                    cx.notify();
                });
                if applied.is_err() {
                    return; // entity dropped — app is shutting down
                }
            }
            let outcome = done_rx.await;
            let _ = this.update(cx, |app: &mut EchoApp, cx| {
                app.update_state = match outcome {
                    Ok(Ok(version)) => UpdateState::Ready(version),
                    Ok(Err(error)) => UpdateState::from_error(error),
                    Err(_) => UpdateState::Failed(
                        views::tr(&app.state, "desktop.settings.updates.failed").to_string(),
                    ),
                };
                app.finish_update_operation();
                cx.notify();
            });
        })
        .detach();
    }
}
