use librespot_connect::{ConnectConfig, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::session::Session;

use crate::events::WorkerEvent;
use crate::worker::volume::{VOLUME_DB_RANGE, volume_to_mixer};
use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::config::{Bitrate, PlayerConfig, VolumeCtrl};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::mixer::Mixer;
use librespot_playback::player::Player;
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc as std_mpsc,
};
use tokio::sync::mpsc;

// Prefer a native stereo config, at `preferred_rate` if the device supports it, else at the device
// default rate. Taking the device default config verbatim can yield mono, which rodio downmixes by
// discarding the right channel rather than summing L+R.
pub(crate) fn preferred_output_config(
    device: &rodio::Device,
    preferred_rate: Option<u32>,
) -> Option<rodio::SupportedStreamConfig> {
    let default_config = device.default_output_config().ok()?;
    let is_stereo =
        |c: &rodio::cpal::SupportedStreamConfigRange| c.channels() == NUM_CHANNELS as u16;
    // Match the device's default sample format first. The supported-config list is not ordered by
    // quality, so taking the first stereo range can land on U8.
    let range = device
        .supported_output_configs()
        .ok()?
        .find(|c| is_stereo(c) && c.sample_format() == default_config.sample_format())
        .or_else(|| device.supported_output_configs().ok()?.find(is_stereo));
    Some(
        range
            .and_then(|c| {
                preferred_rate
                    .and_then(|rate| c.try_with_sample_rate(rodio::cpal::SampleRate(rate)))
                    .or_else(|| c.try_with_sample_rate(default_config.sample_rate()))
            })
            .unwrap_or(default_config),
    )
}

pub(crate) fn log_output_config(kind: &str, device: &str, stream: &rodio::OutputStream) {
    let config = stream.config();
    let _ = std::fs::write(
        format!("echo-debug-audio-{kind}.log"),
        format!(
            "device={device} channels={} sample_rate={} format={:?}",
            config.channel_count(),
            config.sample_rate(),
            config.sample_format()
        ),
    );
}

struct EchoRodioSink {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    errors: std_mpsc::Receiver<String>,
    error_tx: std_mpsc::Sender<String>,
    generation: Arc<AtomicU64>,
    output_available: Arc<AtomicBool>,
    playback_is_playing: Arc<AtomicBool>,
    worker_tx: mpsc::Sender<WorkerEvent>,
}

impl EchoRodioSink {
    fn new(
        worker_tx: mpsc::Sender<WorkerEvent>,
        output_available: Arc<AtomicBool>,
        playback_is_playing: Arc<AtomicBool>,
    ) -> Self {
        let (error_tx, errors) = std_mpsc::channel();
        Self {
            sink: None,
            stream: None,
            errors,
            error_tx,
            generation: Arc::new(AtomicU64::new(0)),
            output_available,
            playback_is_playing,
            worker_tx,
        }
    }

    fn open_output(&mut self) -> SinkResult<()> {
        while self.errors.try_recv().is_ok() {}

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let active_generation = self.generation.clone();
        let error_tx = self.error_tx.clone();
        let output_available = self.output_available.clone();
        let playback_is_playing = self.playback_is_playing.clone();
        let worker_tx = self.worker_tx.clone();
        let was_unavailable = !self.output_available.swap(true, Ordering::SeqCst);

        let on_error = move |error: rodio::cpal::StreamError| {
            if active_generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let message = error.to_string();
            output_available.store(false, Ordering::SeqCst);
            playback_is_playing.store(false, Ordering::SeqCst);
            let _ = error_tx.send(message.clone());
            let _ = worker_tx.try_send(WorkerEvent::AudioOutputUnavailable { message });
        };

        let Some(device) = rodio::cpal::default_host().default_output_device() else {
            return Err(self.output_error("no audio output device".to_string()));
        };
        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());

        let exact = preferred_output_config(&device, Some(SAMPLE_RATE)).and_then(|config| {
            rodio::OutputStreamBuilder::default()
                .with_device(device.clone())
                .with_supported_config(&config)
                .with_error_callback(on_error.clone())
                .open_stream()
                .ok()
        });
        let mut stream = match exact {
            Some(stream) => stream,
            None => rodio::OutputStreamBuilder::from_device(device)
                .map_err(|error| self.output_error(error.to_string()))?
                .with_error_callback(on_error)
                .open_stream_or_fallback()
                .map_err(|error| self.output_error(error.to_string()))?,
        };
        stream.log_on_drop(false);
        log_output_config("spotify", &device_name, &stream);

        if let Ok(message) = self.errors.try_recv() {
            return Err(self.output_error(message));
        }

        self.sink = Some(rodio::Sink::connect_new(stream.mixer()));
        self.stream = Some(stream);
        if was_unavailable {
            let _ = self.worker_tx.try_send(WorkerEvent::AudioOutputRecovered);
        }
        Ok(())
    }

    fn output_error(&mut self, message: String) -> SinkError {
        self.output_available.store(false, Ordering::SeqCst);
        self.playback_is_playing.store(false, Ordering::SeqCst);
        let _ = self
            .worker_tx
            .try_send(WorkerEvent::AudioOutputUnavailable {
                message: message.clone(),
            });
        SinkError::NotConnected(message)
    }

    fn take_stream_error(&mut self) -> SinkResult<()> {
        if let Ok(message) = self.errors.try_recv() {
            self.sink = None;
            self.stream = None;
            return Err(SinkError::NotConnected(message));
        }
        Ok(())
    }
}

impl Sink for EchoRodioSink {
    fn start(&mut self) -> SinkResult<()> {
        // A previous callback failure is exactly why start must rebuild the stream.
        let _ = self.take_stream_error();
        if self.sink.is_none() {
            self.open_output()?;
        }
        if let Some(sink) = self.sink.as_ref() {
            sink.play();
        }
        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        if let Some(sink) = self.sink.as_ref() {
            sink.pause();
        }
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        self.take_stream_error()?;
        let samples = packet
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let source = rodio::buffer::SamplesBuffer::new(
            NUM_CHANNELS as u16,
            SAMPLE_RATE,
            converter.f64_to_f32(samples),
        );
        self.sink
            .as_ref()
            .ok_or_else(|| SinkError::NotConnected("audio output is not open".to_string()))?
            .append(source);
        while self.sink.as_ref().is_some_and(|sink| sink.len() > 26) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.take_stream_error()?;
        }
        Ok(())
    }
}

pub async fn spawn_librespot_daemon(
    _access_token: String,
    device_name: String,
    tx: mpsc::Sender<WorkerEvent>,
    mixer_holder: Arc<parking_lot::Mutex<Option<Arc<dyn Mixer>>>>,
    output_available: Arc<AtomicBool>,
    playback_is_playing: Arc<AtomicBool>,
    bitrate: u32,
    normalisation: bool,
    normalisation_pregain: f64,
    volume: u32,
) {
    tokio::spawn(async move {
        loop {
            let tx = tx.clone();
            let output_available = output_available.clone();
            let playback_is_playing = playback_is_playing.clone();
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                // Find or create cache directory
                let cache_dir = crate::config::echo_config_root();
                std::fs::create_dir_all(&cache_dir)?;
                let cache = Cache::new(Some(cache_dir.clone()), None, None, None)?;

                let credentials = if let Some(creds) = cache.credentials() {
                    creds
                } else {
                    let _ = std::fs::write(
                        "echo-librespot-status.log",
                        "FALLBACK: OPENING BROWSER FOR HARDCODED OAUTH",
                    );

                    let client_builder = librespot_oauth::OAuthClientBuilder::new(
                        "d420a117a32841c2b3474932e49fb54b",
                        "http://127.0.0.1:8989/login",
                        vec![
                            "streaming",
                            "user-read-playback-state",
                            "user-modify-playback-state",
                            "app-remote-control",
                        ],
                    )
                    .open_in_browser();

                    let oauth_client = client_builder
                        .build()
                        .expect("Failed to build OAuth client");
                    let t = oauth_client
                        .get_access_token()
                        .expect("Failed to get access token");

                    // Clear the terminal because librespot-oauth hardcodes a `println!` that
                    // corrupts the TUI layout. Raw ANSI rather than crossterm so the core stays
                    // frontend-free; a windowed frontend's stdout ignores this harmlessly.
                    {
                        use std::io::Write;
                        let _ = write!(std::io::stdout(), "\x1b[2J\x1b[H");
                        let _ = std::io::stdout().flush();
                    }
                    let _ = tx.send(WorkerEvent::ForceRedraw).await;

                    let creds = Credentials::with_access_token(t.access_token);
                    cache.save_credentials(&creds);
                    let _ = std::fs::remove_file("echo-librespot-status.log");
                    creds
                };

                let session_config = SessionConfig::default();
                let session = Session::new(session_config, Some(cache.clone()));

                let player_config = PlayerConfig {
                    bitrate: match bitrate {
                        96 => Bitrate::Bitrate96,
                        160 => Bitrate::Bitrate160,
                        _ => Bitrate::Bitrate320,
                    },
                    normalisation,
                    // Normalisation attenuates by the track's ReplayGain and adds nothing back, so
                    // without a pregain playback sits several dB below the official client. The
                    // default `Dynamic` method's limiter absorbs the added gain instead of clipping.
                    normalisation_pregain_db: normalisation_pregain,
                    ..Default::default()
                };

                let mixer_fn = librespot_playback::mixer::find(None).unwrap();
                // Cubic matches the taper local playback uses, so the same percentage sounds the
                // same on both sources. The default logarithmic curve is far more aggressive.
                let mixer_config = librespot_playback::mixer::MixerConfig {
                    volume_ctrl: VolumeCtrl::Cubic(VOLUME_DB_RANGE),
                    ..Default::default()
                };
                let mixer = mixer_fn(mixer_config).unwrap();
                // SoftMixer opens at a hardcoded 0.5 attenuation, so seed it before any audio flows.
                mixer.set_volume(volume_to_mixer(volume));
                *mixer_holder.lock() = Some(mixer.clone());

                let player = Player::new(
                    player_config,
                    session.clone(),
                    mixer.get_soft_volume(),
                    move || {
                        let backend: Box<dyn Sink> = Box::new(EchoRodioSink::new(
                            tx.clone(),
                            output_available.clone(),
                            playback_is_playing.clone(),
                        ));
                        let shared_bands = std::sync::Arc::new(parking_lot::Mutex::new(
                            [0.0f32; crate::worker::visualization::BANDS],
                        ));
                        let enable_flag =
                            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                        let tx_clone = tx.clone();
                        let bands_clone = shared_bands.clone();
                        let flag_clone = enable_flag.clone();
                        let _ = tx_clone.blocking_send(WorkerEvent::AudioVisualizationReady(
                            bands_clone,
                            flag_clone,
                        ));
                        Box::new(crate::worker::visualization::VisualizationSink::new(
                            backend,
                            shared_bands,
                            enable_flag,
                        ))
                    },
                );

                // Spirc pushes `initial_volume` into the mixer as it starts, so it has to be the
                // real volume: librespot's default of half range would leave playback attenuated
                // until the user happened to touch the volume keys. `disable_volume` tells Spotify
                // clients not to offer a slider for this device, since volume is ours to handle.
                let connect_config = ConnectConfig {
                    name: device_name.clone(),
                    initial_volume: volume_to_mixer(volume),
                    disable_volume: true,
                    ..Default::default()
                };

                let (_spirc, spirc_task) =
                    Spirc::new(connect_config, session.clone(), credentials, player, mixer).await?;

                let _ = std::fs::write(
                    "echo-debug-fallback.log",
                    format!(
                        "Spirc Daemon initialized successfully, awaiting task... bitrate={bitrate} normalisation={normalisation}"
                    ),
                );
                spirc_task.await;
                let _ = std::fs::write("echo-debug-fallback.log", "Spirc Daemon task exited!");

                Ok(())
            }
            .await;

            if let Err(e) = result {
                let err_msg = format!("{:?}", e);
                let _ = std::fs::write(
                    "echo-librespot-fatal.log",
                    format!("Librespot Daemon crashed: {}", err_msg),
                );

                // If the error was caused by invalid/expired credentials, delete the cache and retry immediately.
                if err_msg.contains("BadCredentials") || err_msg.contains("PermissionDenied") {
                    let cache_dir = crate::config::echo_config_root();
                    let _ = std::fs::remove_file(cache_dir.join("credentials.json"));
                    continue; // Loop back and trigger the browser re-auth flow
                }

                break; // Unrecoverable error, break the loop
            } else {
                let _ = std::fs::write(
                    "echo-librespot-fatal.log",
                    "Librespot Daemon exited normally.",
                );
                break;
            }
        }
    });
}
