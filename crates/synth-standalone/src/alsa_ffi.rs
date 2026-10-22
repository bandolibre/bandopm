//! Dynamic runtime bindings to `libasound.so.2` (ALSA PCM & ALSA Sequencer)
//! and `libpulse-simple.so.0` (PulseAudio / PipeWire Simple API) on Linux via `libloading`.
//!
//! Loading `libasound.so.2` via `libloading` allows `synth-standalone` to compile cleanly
//! on any Linux machine even when `libasound2-dev` (`alsa.pc`) is not installed.

use anyhow::{bail, Context, Result};
use libloading::Library;
use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CStr, CString};

unsafe fn load_sym<T>(lib: &Library, name: &[u8]) -> Result<T> {
    let sym = lib
        .get::<*const c_void>(name)
        .with_context(|| format!("Missing symbol '{}'", String::from_utf8_lossy(name)))?;
    Ok(std::mem::transmute_copy(&*sym))
}

#[derive(Debug, Clone)]
pub struct MidiPortInfo {
    pub client_id: i32,
    pub port_id: i32,
    pub client_name: String,
    pub port_name: String,
}

impl MidiPortInfo {
    pub fn display_name(&self) -> String {
        format!(
            "[{}:{}] {} - {}",
            self.client_id, self.port_id, self.client_name, self.port_name
        )
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        let addr = format!("{}:{}", self.client_id, self.port_id);
        self.client_name.to_lowercase().contains(&q)
            || self.port_name.to_lowercase().contains(&q)
            || addr == q
    }
}

pub struct AlsaSeqMidiListener {
    _lib: Library,
    seq: *mut c_void,
    decoder: *mut c_void,
    snd_seq_event_input: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int,
    snd_midi_event_decode:
        unsafe extern "C" fn(*mut c_void, *mut u8, c_long, *const c_void) -> c_long,
    snd_midi_event_free: unsafe extern "C" fn(*mut c_void),
    snd_seq_close: unsafe extern "C" fn(*mut c_void) -> c_int,
}

unsafe impl Send for AlsaSeqMidiListener {}

impl AlsaSeqMidiListener {
    /// Lists all readable ALSA Sequencer MIDI ports on the system.
    pub fn list_ports() -> Result<Vec<MidiPortInfo>> {
        unsafe {
            let lib = Library::new("libasound.so.2").context("Could not load libasound.so.2")?;

            let snd_seq_open: unsafe extern "C" fn(
                *mut *mut c_void,
                *const c_char,
                c_int,
                c_int,
            ) -> c_int = load_sym(&lib, b"snd_seq_open\0")?;
            let snd_seq_close: unsafe extern "C" fn(*mut c_void) -> c_int =
                load_sym(&lib, b"snd_seq_close\0")?;
            let snd_strerror: unsafe extern "C" fn(c_int) -> *const c_char =
                load_sym(&lib, b"snd_strerror\0")?;
            let snd_seq_client_info_sizeof: unsafe extern "C" fn() -> usize =
                load_sym(&lib, b"snd_seq_client_info_sizeof\0")?;
            let snd_seq_port_info_sizeof: unsafe extern "C" fn() -> usize =
                load_sym(&lib, b"snd_seq_port_info_sizeof\0")?;
            let snd_seq_client_info_set_client: unsafe extern "C" fn(*mut c_void, c_int) =
                load_sym(&lib, b"snd_seq_client_info_set_client\0")?;
            let snd_seq_query_next_client: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int =
                load_sym(&lib, b"snd_seq_query_next_client\0")?;
            let snd_seq_client_info_get_client: unsafe extern "C" fn(*const c_void) -> c_int =
                load_sym(&lib, b"snd_seq_client_info_get_client\0")?;
            let snd_seq_client_info_get_name: unsafe extern "C" fn(*const c_void) -> *const c_char =
                load_sym(&lib, b"snd_seq_client_info_get_name\0")?;
            let snd_seq_port_info_set_client: unsafe extern "C" fn(*mut c_void, c_int) =
                load_sym(&lib, b"snd_seq_port_info_set_client\0")?;
            let snd_seq_port_info_set_port: unsafe extern "C" fn(*mut c_void, c_int) =
                load_sym(&lib, b"snd_seq_port_info_set_port\0")?;
            let snd_seq_query_next_port: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int =
                load_sym(&lib, b"snd_seq_query_next_port\0")?;
            let snd_seq_port_info_get_port: unsafe extern "C" fn(*const c_void) -> c_int =
                load_sym(&lib, b"snd_seq_port_info_get_port\0")?;
            let snd_seq_port_info_get_name: unsafe extern "C" fn(*const c_void) -> *const c_char =
                load_sym(&lib, b"snd_seq_port_info_get_name\0")?;
            let snd_seq_port_info_get_capability: unsafe extern "C" fn(*const c_void) -> c_uint =
                load_sym(&lib, b"snd_seq_port_info_get_capability\0")?;

            let mut seq: *mut c_void = std::ptr::null_mut();
            let default_str = CString::new("default")?;
            let rc = snd_seq_open(&mut seq, default_str.as_ptr(), 3, 0);
            if rc < 0 {
                let err_msg = CStr::from_ptr(snd_strerror(rc)).to_string_lossy();
                bail!(
                    "snd_seq_open failed ({}): {} (check /dev/snd/seq permissions or 'audio' group)",
                    rc,
                    err_msg
                );
            }

            let mut cinfo = vec![0u8; snd_seq_client_info_sizeof()];
            let mut pinfo = vec![0u8; snd_seq_port_info_sizeof()];
            let cptr = cinfo.as_mut_ptr() as *mut c_void;
            let pptr = pinfo.as_mut_ptr() as *mut c_void;

            let mut ports = Vec::new();
            snd_seq_client_info_set_client(cptr, -1);
            while snd_seq_query_next_client(seq, cptr) >= 0 {
                let cid = snd_seq_client_info_get_client(cptr);
                let cname = CStr::from_ptr(snd_seq_client_info_get_name(cptr))
                    .to_string_lossy()
                    .into_owned();
                snd_seq_port_info_set_client(pptr, cid);
                snd_seq_port_info_set_port(pptr, -1);
                while snd_seq_query_next_port(seq, pptr) >= 0 {
                    let pid = snd_seq_port_info_get_port(pptr);
                    let pname = CStr::from_ptr(snd_seq_port_info_get_name(pptr))
                        .to_string_lossy()
                        .into_owned();
                    let cap = snd_seq_port_info_get_capability(pptr);
                    // SND_SEQ_PORT_CAP_READ (1) | SND_SEQ_PORT_CAP_SUBS_READ (0x20) = 0x21
                    if (cap & 0x21) == 0x21 && cid != 0 {
                        ports.push(MidiPortInfo {
                            client_id: cid,
                            port_id: pid,
                            client_name: cname.clone(),
                            port_name: pname,
                        });
                    }
                }
            }
            snd_seq_close(seq);
            Ok(ports)
        }
    }

    /// Opens an ALSA Sequencer input port and connects it to the MIDI device matching `device_name`.
    pub fn open_and_bind(device_name: &str) -> Result<(Self, Option<MidiPortInfo>)> {
        let ports = Self::list_ports()?;
        let matched = ports.into_iter().find(|p| p.matches_query(device_name));

        unsafe {
            let lib = Library::new("libasound.so.2").context("Could not load libasound.so.2")?;

            let snd_seq_open: unsafe extern "C" fn(
                *mut *mut c_void,
                *const c_char,
                c_int,
                c_int,
            ) -> c_int = load_sym(&lib, b"snd_seq_open\0")?;
            let snd_seq_set_client_name: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int =
                load_sym(&lib, b"snd_seq_set_client_name\0")?;
            let snd_seq_create_simple_port: unsafe extern "C" fn(
                *mut c_void,
                *const c_char,
                c_uint,
                c_uint,
            ) -> c_int = load_sym(&lib, b"snd_seq_create_simple_port\0")?;
            let snd_seq_connect_from: unsafe extern "C" fn(
                *mut c_void,
                c_int,
                c_int,
                c_int,
            ) -> c_int = load_sym(&lib, b"snd_seq_connect_from\0")?;
            let snd_seq_event_input: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int =
                load_sym(&lib, b"snd_seq_event_input\0")?;
            let snd_midi_event_new: unsafe extern "C" fn(c_long, *mut *mut c_void) -> c_int =
                load_sym(&lib, b"snd_midi_event_new\0")?;
            let snd_midi_event_no_status: unsafe extern "C" fn(*mut c_void, c_int) =
                load_sym(&lib, b"snd_midi_event_no_status\0")?;
            let snd_midi_event_decode: unsafe extern "C" fn(
                *mut c_void,
                *mut u8,
                c_long,
                *const c_void,
            ) -> c_long = load_sym(&lib, b"snd_midi_event_decode\0")?;
            let snd_midi_event_free: unsafe extern "C" fn(*mut c_void) =
                load_sym(&lib, b"snd_midi_event_free\0")?;
            let snd_seq_close: unsafe extern "C" fn(*mut c_void) -> c_int =
                load_sym(&lib, b"snd_seq_close\0")?;

            let mut seq: *mut c_void = std::ptr::null_mut();
            let default_str = CString::new("default")?;
            let rc = snd_seq_open(&mut seq, default_str.as_ptr(), 2, 0); // SND_SEQ_OPEN_INPUT = 2
            if rc < 0 {
                bail!("snd_seq_open failed with code {}", rc);
            }

            let client_title = CString::new("Bandolibre-Synth")?;
            snd_seq_set_client_name(seq, client_title.as_ptr());

            let port_title = CString::new("MIDI-In")?;
            // WRITE | SUBS_WRITE = 0x02 | 0x40 = 0x42; MIDI_GENERIC | APPLICATION = 0x02 | 0x100000
            let my_port =
                snd_seq_create_simple_port(seq, port_title.as_ptr(), 0x42, 0x02 | 0x100000);
            if my_port < 0 {
                snd_seq_close(seq);
                bail!("snd_seq_create_simple_port failed: {}", my_port);
            }

            if let Some(ref target) = matched {
                let conn_rc =
                    snd_seq_connect_from(seq, my_port, target.client_id, target.port_id);
                if conn_rc < 0 {
                    snd_seq_close(seq);
                    bail!(
                        "Failed to subscribe to MIDI port {} ({})",
                        target.display_name(),
                        conn_rc
                    );
                }
            }

            let mut decoder: *mut c_void = std::ptr::null_mut();
            snd_midi_event_new(32, &mut decoder);
            snd_midi_event_no_status(decoder, 1);

            Ok((
                Self {
                    _lib: lib,
                    seq,
                    decoder,
                    snd_seq_event_input,
                    snd_midi_event_decode,
                    snd_midi_event_free,
                    snd_seq_close,
                },
                matched,
            ))
        }
    }

    /// Blocks until the next ALSA Sequencer MIDI event arrives and decodes it to raw MIDI bytes.
    pub fn recv_midi_packet(&mut self, buf: &mut [u8; 16]) -> Option<usize> {
        unsafe {
            let mut ev: *mut c_void = std::ptr::null_mut();
            let rc = (self.snd_seq_event_input)(self.seq, &mut ev);
            if rc < 0 || ev.is_null() {
                return None;
            }
            let n = (self.snd_midi_event_decode)(
                self.decoder,
                buf.as_mut_ptr(),
                buf.len() as c_long,
                ev,
            );
            if n > 0 {
                Some(n as usize)
            } else {
                None
            }
        }
    }
}

impl Drop for AlsaSeqMidiListener {
    fn drop(&mut self) {
        unsafe {
            if !self.decoder.is_null() {
                (self.snd_midi_event_free)(self.decoder);
            }
            if !self.seq.is_null() {
                (self.snd_seq_close)(self.seq);
            }
        }
    }
}

/// Real-time stereo `f32` audio output stream via ALSA PCM (`libasound.so.2`)
/// or PulseAudio/PipeWire Simple API (`libpulse-simple.so.0`).
pub enum LinuxAudioStream {
    Alsa {
        _lib: Library,
        pcm: *mut c_void,
        snd_pcm_writei: unsafe extern "C" fn(*mut c_void, *const c_void, c_long) -> c_long,
        snd_pcm_recover: unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int,
        snd_pcm_close: unsafe extern "C" fn(*mut c_void) -> c_int,
    },
    Pulse {
        _lib: Library,
        pa: *mut c_void,
        pa_simple_write: unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int,
        pa_simple_free: unsafe extern "C" fn(*mut c_void),
    },
}

impl LinuxAudioStream {
    pub fn open(device_name: &str, sample_rate: u32, latency_us: u32) -> Result<Self> {
        // Try ALSA PCM first unless user explicitly requested "pulse"
        if !device_name.eq_ignore_ascii_case("pulse") {
            if let Ok(alsa) = Self::open_alsa(device_name, sample_rate, latency_us) {
                return Ok(alsa);
            }
        }
        Self::open_pulse(sample_rate, latency_us)
    }

    fn open_alsa(device_name: &str, sample_rate: u32, latency_us: u32) -> Result<Self> {
        unsafe {
            let lib = Library::new("libasound.so.2").context("Could not load libasound.so.2")?;

            let snd_pcm_open: unsafe extern "C" fn(
                *mut *mut c_void,
                *const c_char,
                c_int,
                c_int,
            ) -> c_int = load_sym(&lib, b"snd_pcm_open\0")?;
            let snd_pcm_set_params: unsafe extern "C" fn(
                *mut c_void,
                c_int,
                c_int,
                c_uint,
                c_uint,
                c_int,
                c_uint,
            ) -> c_int = load_sym(&lib, b"snd_pcm_set_params\0")?;
            let snd_pcm_writei: unsafe extern "C" fn(*mut c_void, *const c_void, c_long) -> c_long =
                load_sym(&lib, b"snd_pcm_writei\0")?;
            let snd_pcm_recover: unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int =
                load_sym(&lib, b"snd_pcm_recover\0")?;
            let snd_pcm_close: unsafe extern "C" fn(*mut c_void) -> c_int =
                load_sym(&lib, b"snd_pcm_close\0")?;

            let mut pcm: *mut c_void = std::ptr::null_mut();
            let dev_c = CString::new(device_name)?;
            // SND_PCM_STREAM_PLAYBACK = 0
            let rc = snd_pcm_open(&mut pcm, dev_c.as_ptr(), 0, 0);
            if rc < 0 {
                bail!("snd_pcm_open('{}') failed with {}", device_name, rc);
            }

            // SND_PCM_FORMAT_FLOAT_LE = 14, SND_PCM_ACCESS_RW_INTERLEAVED = 3
            let rc_params = snd_pcm_set_params(pcm, 14, 3, 2, sample_rate, 1, latency_us);
            if rc_params < 0 {
                snd_pcm_close(pcm);
                bail!("snd_pcm_set_params failed with {}", rc_params);
            }

            Ok(Self::Alsa {
                _lib: lib,
                pcm,
                snd_pcm_writei,
                snd_pcm_recover,
                snd_pcm_close,
            })
        }
    }

    fn open_pulse(sample_rate: u32, _latency_us: u32) -> Result<Self> {
        #[repr(C)]
        struct PaSampleSpec {
            format: c_int,
            rate: u32,
            channels: u8,
        }
        #[repr(C)]
        struct PaBufferAttr {
            maxlength: u32,
            tlength: u32,
            prebuf: u32,
            minreq: u32,
            fragsize: u32,
        }

        unsafe {
            let lib = Library::new("libpulse-simple.so.0")
                .context("Could not load libpulse-simple.so.0")?;

            let pa_simple_new: unsafe extern "C" fn(
                *const c_char,
                *const c_char,
                c_int,
                *const c_char,
                *const c_char,
                *const PaSampleSpec,
                *const c_void,
                *const PaBufferAttr,
                *mut c_int,
            ) -> *mut c_void = load_sym(&lib, b"pa_simple_new\0")?;
            let pa_simple_write: unsafe extern "C" fn(
                *mut c_void,
                *const c_void,
                usize,
                *mut c_int,
            ) -> c_int = load_sym(&lib, b"pa_simple_write\0")?;
            let pa_simple_free: unsafe extern "C" fn(*mut c_void) =
                load_sym(&lib, b"pa_simple_free\0")?;

            // PA_SAMPLE_FLOAT32LE = 5, PA_STREAM_PLAYBACK = 1
            let ss = PaSampleSpec {
                format: 5,
                rate: sample_rate,
                channels: 2,
            };
            let attr = PaBufferAttr {
                maxlength: 2048,
                tlength: 1024,
                prebuf: u32::MAX,
                minreq: u32::MAX,
                fragsize: u32::MAX,
            };
            let app_name = CString::new("Bandolibre-Synth")?;
            let stream_name = CString::new("Physical Modeling Bandoneon")?;
            let mut err: c_int = 0;
            let pa = pa_simple_new(
                std::ptr::null(),
                app_name.as_ptr(),
                1,
                std::ptr::null(),
                stream_name.as_ptr(),
                &ss,
                std::ptr::null(),
                &attr,
                &mut err,
            );
            if pa.is_null() {
                bail!("pa_simple_new failed with error code {}", err);
            }

            Ok(Self::Pulse {
                _lib: lib,
                pa,
                pa_simple_write,
                pa_simple_free,
            })
        }
    }

    pub fn write_interleaved(&mut self, interleaved_f32: &[f32]) -> Result<()> {
        unsafe {
            match self {
                Self::Alsa {
                    pcm,
                    snd_pcm_writei,
                    snd_pcm_recover,
                    ..
                } => {
                    let frames = (interleaved_f32.len() / 2) as c_long;
                    let n = snd_pcm_writei(*pcm, interleaved_f32.as_ptr() as *const c_void, frames);
                    if n < 0 {
                        let _ = snd_pcm_recover(*pcm, n as c_int, 1);
                    }
                    Ok(())
                }
                Self::Pulse {
                    pa,
                    pa_simple_write,
                    ..
                } => {
                    let mut err: c_int = 0;
                    let bytes = std::mem::size_of_val(interleaved_f32);
                    let rc = pa_simple_write(
                        *pa,
                        interleaved_f32.as_ptr() as *const c_void,
                        bytes,
                        &mut err,
                    );
                    if rc < 0 {
                        bail!("pa_simple_write error {}", err);
                    }
                    Ok(())
                }
            }
        }
    }
}

impl Drop for LinuxAudioStream {
    fn drop(&mut self) {
        unsafe {
            match self {
                Self::Alsa {
                    pcm, snd_pcm_close, ..
                } => {
                    if !pcm.is_null() {
                        snd_pcm_close(*pcm);
                    }
                }
                Self::Pulse {
                    pa, pa_simple_free, ..
                } => {
                    if !pa.is_null() {
                        pa_simple_free(*pa);
                    }
                }
            }
        }
    }
}
