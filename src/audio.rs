use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

/// streams raw s16le pcm out of ffmpeg. needed because symphonia (rodio's
/// decoder) has no opus codec, so most of the shelf (webm) can't be read directly.
struct FfmpegPcm {
    reader: BufReader<ChildStdout>,
    buf: Vec<u8>,
    pos: usize,
    child: Option<Child>,
}

impl FfmpegPcm {
    fn new(reader: ChildStdout, child: Child) -> Self {
        Self {
            reader: BufReader::with_capacity(32 * 1024, reader),
            buf: Vec::with_capacity(32 * 1024),
            pos: 0,
            child: Some(child),
        }
    }
}

impl Drop for FfmpegPcm {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Iterator for FfmpegPcm {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        while self.buf.len() - self.pos < 2 {
            if self.pos > 0 {
                self.buf.drain(..self.pos);
                self.pos = 0;
            }
            let mut chunk = [0u8; 8192];
            match self.reader.read(&mut chunk) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(_) => return None,
            }
        }
        let sample = i16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Some(sample as f32 / 32768.0)
    }
}

impl Source for FfmpegPcm {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> std::num::NonZero<u16> { std::num::NonZero::new(2).unwrap() }
    fn sample_rate(&self) -> std::num::NonZero<u32> { std::num::NonZero::new(44100).unwrap() }
    fn total_duration(&self) -> Option<Duration> { None }
}

fn ffmpeg_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("VR_FFMPEG") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(Path::new(".")).join("bin").join("ffmpeg.exe");
        if p.is_file() { return Some(p); }
    }
    let local = PathBuf::from("bin").join("ffmpeg.exe");
    if local.is_file() { return Some(local); }
    None
}

fn probe_duration(ff: &Path, file: &Path) -> Option<Duration> {
    let out = Command::new(ff)
        .arg("-hide_banner")
        .arg("-i")
        .arg(file)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find(|l| l.contains("Duration:"))?;
    let stamp = line.split("Duration:").nth(1)?.trim().split(',').next()?;
    let parts: Vec<&str> = stamp.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let s: f64 = parts[2].parse().ok()?;
    Some(Duration::from_secs_f64(h * 3600.0 + m * 60.0 + s))
}

pub struct Engine {
    _device: MixerDeviceSink,
    player: Player,
    path: Option<PathBuf>,
    seek_offset: Duration,
    ffmpeg: bool,
    pub paused: bool,
    pub error: Option<String>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        let device = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no audio device: {e}"))?;
        let player = Player::connect_new(&device.mixer());
        Ok(Self {
            _device: device,
            player,
            path: None,
            seek_offset: Duration::ZERO,
            ffmpeg: false,
            paused: true,
            error: None,
        })
    }

    pub fn play(&mut self, path: &Path, seek: Duration, speed: f32, volume: u8) -> Option<Duration> {
        self.player.stop();
        self.error = None;
        self.path = Some(path.to_path_buf());
        self.seek_offset = seek;
        self.paused = false;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let ffmpeg_kind = matches!(ext.as_str(), "webm" | "mp4" | "mkv" | "mov" | "avi");
        self.ffmpeg = ffmpeg_kind;

        let result: Result<(Box<dyn Source<Item = f32> + Send>, Option<Duration>), String> =
            if ffmpeg_kind {
                let ff = match ffmpeg_path() {
                    Some(p) => p,
                    None => {
                        self.error = Some("ffmpeg not found - put ffmpeg.exe in bin/ or set VR_FFMPEG".to_string());
                        return None;
                    }
                };
                let total = probe_duration(&ff, path);
                let mut cmd = Command::new(&ff);
                cmd.arg("-v").arg("error")
                    .arg("-ss").arg(seek.as_secs_f64().to_string())
                    .arg("-i").arg(path)
                    .arg("-f").arg("s16le")
                    .arg("-acodec").arg("pcm_s16le")
                    .arg("-ar").arg("44100")
                    .arg("-ac").arg("2")
                    .arg("pipe:1")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                match cmd.spawn() {
                    Ok(mut child) => {
                        let stdout = child.stdout.take()?;
                        let src = FfmpegPcm::new(stdout, child);
                        Ok((Box::new(src), total))
                    }
                    Err(e) => Err(format!("ffmpeg failed: {e}")),
                }
            } else {
                File::open(path)
                    .map_err(|e| format!("cannot open file: {e}"))
                    .and_then(|f| {
                        let dec = Decoder::new(f).map_err(|e| format!("cannot decode: {e}"))?;
                        let total = dec.total_duration();
                        Ok((Box::new(dec) as Box<dyn Source<Item = f32> + Send>, total))
                    })
            };

        match result {
            Ok((src, total)) => {
                self.player.set_volume(volume as f32 / 100.0);
                self.player.set_speed(speed);
                self.player.append(src);
                self.player.play();
                total
            }
            Err(e) => {
                self.error = Some(e);
                self.paused = true;
                None
            }
        }
    }

    pub fn set_volume(&self, v: u8) { self.player.set_volume(v as f32 / 100.0); }
    pub fn set_speed(&self, s: f32) { self.player.set_speed(s); }

    pub fn seek(&mut self, d: Duration, speed: f32, volume: u8) -> Option<Duration> {
        // ffmpeg streams aren't seekable through the sink, so restart the pipe at -ss.
        if self.ffmpeg {
            if let Some(p) = self.path.clone() {
                return self.play(&p, d, speed, volume);
            }
            return None;
        }
        let _ = self.player.try_seek(d);
        self.seek_offset = d;
        None
    }

    pub fn pause(&mut self) { self.player.pause(); self.paused = true; }
    pub fn resume(&mut self) { self.player.play(); self.paused = false; }
    pub fn is_playing(&self) -> bool { !self.paused && !self.player.empty() }
    pub fn ended(&self) -> bool { !self.paused && self.player.empty() }
    pub fn position(&self) -> Duration {
        self.seek_offset.saturating_add(self.player.get_pos())
    }
    pub fn source_path(&self) -> Option<&Path> { self.path.as_deref() }
}
