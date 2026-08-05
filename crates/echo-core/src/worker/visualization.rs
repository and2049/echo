use librespot_playback::audio_backend::{Sink, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use parking_lot::Mutex;
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub const BANDS: usize = 32;
pub const FFT_SIZE: usize = 8192;
pub const HOP_SIZE: usize = 256;

pub struct VisualizationSink {
    inner: Box<dyn Sink>,
    shared_bands: Arc<Mutex<[f32; BANDS]>>,
    enable_flag: Arc<std::sync::atomic::AtomicBool>,

    sample_buffer: Vec<f32>,
    hann_window: Vec<f32>,
    planner: FftPlanner<f32>,
}

impl VisualizationSink {
    pub fn new(
        inner: Box<dyn Sink>,
        shared_bands: Arc<Mutex<[f32; BANDS]>>,
        enable_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let mut hann_window = vec![0.0; FFT_SIZE];
        for i in 0..FFT_SIZE {
            hann_window[i] = 0.5
                * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos());
        }

        Self {
            inner,
            shared_bands,
            enable_flag,
            sample_buffer: Vec::with_capacity(FFT_SIZE),
            hann_window,
            planner: FftPlanner::new(),
        }
    }

    fn process_audio(&mut self, samples: &[f64]) {
        for chunk in samples.chunks_exact(2) {
            let mono = ((chunk[0] + chunk[1]) / 2.0) as f32;
            self.sample_buffer.push(mono);

            if self.sample_buffer.len() >= FFT_SIZE {
                self.compute_fft();
                self.sample_buffer.drain(0..HOP_SIZE);
            }
        }
    }

    fn compute_fft(&mut self) {
        let fft = self.planner.plan_fft_forward(FFT_SIZE);

        let mut buffer: Vec<Complex<f32>> = self
            .sample_buffer
            .iter()
            .zip(self.hann_window.iter())
            .map(|(s, w)| Complex { re: s * w, im: 0.0 })
            .collect();

        fft.process(&mut buffer);

        let mut bands = [0.0f32; BANDS];

        // At 44.1kHz and 8192 points, each bin is ~5.38 Hz
        let min_freq_idx = 4.0; // ~21 Hz
        let max_freq_idx = (FFT_SIZE / 2) as f32; // ~22 kHz

        for bucket in 0..BANDS {
            let p_start = bucket as f32 / BANDS as f32;
            let p_end = (bucket + 1) as f32 / BANDS as f32;

            let ratio = max_freq_idx / min_freq_idx;
            let start_idx = (min_freq_idx * ratio.powf(p_start)) as usize;
            let end_idx = (min_freq_idx * ratio.powf(p_end)) as usize;

            let start_idx = start_idx.clamp(1, max_freq_idx as usize - 1);
            let end_idx = end_idx.clamp(start_idx, max_freq_idx as usize - 1);

            let mut max_mag = 0.0f32;
            if start_idx == end_idx {
                max_mag = buffer[start_idx].norm();
            } else {
                for i in start_idx..=end_idx {
                    max_mag = max_mag.max(buffer[i].norm());
                }
            }
            bands[bucket] = max_mag;
        }

        let mut shared = self.shared_bands.lock();
        for i in 0..BANDS {
            let mag = bands[i];
            let db = if mag > 0.001 {
                20.0 * mag.log10()
            } else {
                -60.0
            };
            // Map -40dB .. 40dB to 0 .. 100
            let normalized = ((db + 40.0) * (100.0 / 80.0)).clamp(0.0, 100.0);

            // Smooth decay
            shared[i] = (shared[i] * 0.6) + (normalized * 0.4);
        }
    }
}

impl Sink for VisualizationSink {
    fn start(&mut self) -> SinkResult<()> {
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.inner.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let mut cloned_samples = None;
        if let AudioPacket::Samples(ref samples) = packet {
            cloned_samples = Some(samples.clone());
        }
        let res = self.inner.write(packet, converter);

        if self.enable_flag.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(samples) = cloned_samples {
                self.process_audio(&samples);
            }
        } else {
            // If disabled, slowly decay existing bars to 0 instead of freezing them high
            let mut shared = self.shared_bands.lock();
            for i in 0..BANDS {
                shared[i] *= 0.8;
            }
        }
        res
    }
}
