use std::{
    io::{self, Read, Seek, SeekFrom},
    panic::AssertUnwindSafe,
    sync::Arc,
};

use futures_util::StreamExt;
use matroska::{Settings, Tracktype};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_db::{
    CatalogPublicationError, CatalogPublicationRepository, ClaimedWorkJob, ProbeCandidate,
    ProbeRepository, ProbeRepositoryError, ProbeResult, ProbedStream, StorageSyncRepositoryError,
};
use tjxy_storage::{BackendError, ByteRange, StorageBackend, StorageObject, StorageObjectId};
use uuid::Uuid;

use crate::{
    StorageBackendRegistry, StorageChangeProjectorError,
    storage_read::{self, StorageReadError},
    strm::{MAX_STRM_BYTES, parse_strm},
};

const RANGE_BUDGET: u64 = 1024 * 1024;
const ADAPTIVE_RANGE_BUDGETS: &[u64] = &[
    4 * 1024 * 1024,
    16 * 1024 * 1024,
    64 * 1024 * 1024,
    128 * 1024 * 1024,
];
const SEQUENTIAL_FALLBACK_MAX: u64 = 128 * 1024 * 1024;
const GAP_RETRY_RANGE: u64 = 8 * 1024 * 1024;
const MAX_GAP_RETRIES: usize = 16;

#[derive(Clone)]
pub struct ProbeInput {
    size: u64,
    segments: Vec<ProbeSegment>,
}

impl ProbeInput {
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    fn into_reader(self) -> SparseProbeReader {
        SparseProbeReader {
            size: self.size,
            segments: self.segments,
            position: 0,
        }
    }
}

#[derive(Clone)]
struct ProbeSegment {
    start: u64,
    bytes: Vec<u8>,
}

pub trait MediaInspector: Send + Sync {
    /// Parses media metadata only from the supplied bounded sparse input.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeServiceError::Inspection`] when the container is unsupported or incomplete.
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError>;
}

#[derive(Default)]
pub struct MatroskaInspector;

impl MediaInspector for MatroskaInspector {
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError> {
        let parsed = std::panic::catch_unwind(AssertUnwindSafe(|| {
            matroska::Matroska::open(input.into_reader())
        }))
        .map_err(|_| {
            ProbeServiceError::Inspection("Matroska parser rejected seek metadata".into())
        })?
        .map_err(|error| ProbeServiceError::Inspection(error.to_string()))?;
        let mut streams = Vec::with_capacity(parsed.tracks.len());
        let mut video_codec = None;
        let mut resolution = None;
        for track in parsed.tracks.iter().filter(|track| track.enabled) {
            let (stream_type, width, height, channels) = match (&track.tracktype, &track.settings) {
                (Tracktype::Video, Settings::Video(video)) => (
                    "Video",
                    bounded_i32(video.pixel_width)?,
                    bounded_i32(video.pixel_height)?,
                    None,
                ),
                (Tracktype::Audio, Settings::Audio(audio)) => {
                    ("Audio", None, None, bounded_i32(audio.channels)?)
                }
                (Tracktype::Subtitle, _) => ("Subtitle", None, None, None),
                _ => continue,
            };
            let codec = normalize_codec(&track.codec_id);
            let (profile, level) = if stream_type == "Video" {
                codec_compatibility(&codec, track.codec_private.as_deref())
            } else {
                (None, None)
            };
            if stream_type == "Video" && video_codec.is_none() {
                video_codec = Some(codec.clone());
                if let (Some(width), Some(height)) = (width, height) {
                    resolution = Some(format!("{width}x{height}"));
                }
            }
            let stable = if track.uid == 0 {
                format!("track:{}:{stream_type}", track.number)
            } else {
                format!("uid:{}", track.uid)
            };
            streams.push(
                ProbedStream::new(
                    stable,
                    stream_type,
                    i32::try_from(track.number).map_err(|_| {
                        ProbeServiceError::Inspection("track index is too large".into())
                    })?,
                    Some(codec),
                    track.language.as_ref().map(ToString::to_string),
                    width,
                    height,
                    channels,
                    track.default,
                    track.forced,
                )
                .and_then(|stream| stream.with_video_compatibility(profile, level))
                .map_err(|_| {
                    ProbeServiceError::Inspection("invalid Matroska track metadata".into())
                })?,
            );
        }
        if streams.is_empty() {
            return Err(ProbeServiceError::Inspection(
                "Matroska file contains no supported tracks".into(),
            ));
        }
        let runtime_ticks = parsed
            .info
            .duration
            .map(|duration| duration.as_nanos() / 100)
            .map(i64::try_from)
            .transpose()
            .map_err(|_| ProbeServiceError::Inspection("duration is too large".into()))?;
        Ok(ProbeResult::new("mkv", streams)
            .map_err(|_| ProbeServiceError::Inspection("invalid Matroska Probe result".into()))?
            .with_video(video_codec, resolution)
            .with_timing(None, runtime_ticks))
    }
}

#[derive(Default)]
pub struct DefaultMediaInspector;

impl MediaInspector for DefaultMediaInspector {
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError> {
        if is_iso_bmff(&input) {
            IsoBmffInspector.inspect(input)
        } else if is_audio_container(&input) {
            AudioInspector.inspect(input)
        } else {
            MatroskaInspector.inspect(input)
        }
    }
}

struct IsoBmffInspector;

impl MediaInspector for IsoBmffInspector {
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError> {
        let mut movie = IsoBmffMovie::default();
        if let Some((start, end)) = find_moov_range(&input)? {
            let mut reader = SparseProbeReader {
                size: input.size,
                segments: input.segments.clone(),
                position: start,
            };
            parse_iso_boxes(&mut reader, end, &mut movie, None, 0)?;
        } else {
            // No moov magic in any covered segment: the movie header may sit
            // in the middle of a large file. Walk the top-level boxes from the
            // start instead; box bodies such as mdat are skipped by seek, and
            // uncovered box headers surface as probe gaps that the service
            // retry ladder fills.
            let mut reader = SparseProbeReader {
                size: input.size,
                segments: input.segments.clone(),
                position: 0,
            };
            parse_iso_boxes(&mut reader, input.size, &mut movie, None, 0)?;
        }
        let runtime_ticks = movie.duration_ticks()?;
        let resolution = movie.tracks.iter().find_map(|track| {
            (track.handler == Some(*b"vide"))
                .then(|| Some(format!("{}x{}", track.width?, track.height?)))?
        });
        let streams = movie
            .tracks
            .into_iter()
            .map(IsoBmffTrack::into_stream)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if streams.is_empty() {
            return Err(ProbeServiceError::Inspection(
                "ISO-BMFF file contains no supported tracks".into(),
            ));
        }
        let video = streams
            .iter()
            .find(|stream| stream.stream_type() == "Video");
        let video_codec = video.and_then(|stream| stream.codec().map(ToOwned::to_owned));
        Ok(ProbeResult::new("mp4", streams)
            .map_err(|_| ProbeServiceError::Inspection("invalid ISO-BMFF Probe result".into()))?
            .with_video(video_codec, resolution)
            .with_timing(None, runtime_ticks))
    }
}

#[derive(Default)]
struct IsoBmffMovie {
    timescale: Option<u64>,
    duration: Option<u64>,
    tracks: Vec<IsoBmffTrack>,
}

impl IsoBmffMovie {
    fn duration_ticks(&self) -> Result<Option<i64>, ProbeServiceError> {
        let Some(timescale) = self.timescale.filter(|timescale| *timescale > 0) else {
            return Ok(None);
        };
        let Some(duration) = self.duration else {
            return Ok(None);
        };
        let ticks = duration.checked_mul(10_000_000).ok_or_else(|| {
            ProbeServiceError::Inspection("ISO-BMFF duration is too large".into())
        })? / timescale;
        i64::try_from(ticks)
            .map(Some)
            .map_err(|_| ProbeServiceError::Inspection("ISO-BMFF duration is too large".into()))
    }
}

#[derive(Default)]
struct IsoBmffTrack {
    id: Option<u32>,
    handler: Option<[u8; 4]>,
    codec: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    profile: Option<String>,
    level: Option<i32>,
}

impl IsoBmffTrack {
    fn into_stream(self) -> Result<Option<ProbedStream>, ProbeServiceError> {
        let (stream_type, width, height, channels) = match self.handler {
            Some(handler) if handler == *b"vide" => ("Video", self.width, self.height, None),
            Some(handler) if handler == *b"soun" => ("Audio", None, None, self.channels),
            Some(handler) if handler == *b"subt" || handler == *b"text" => {
                ("Subtitle", None, None, None)
            }
            _ => return Ok(None),
        };
        let index = i32::try_from(self.id.unwrap_or_default()).map_err(|_| {
            ProbeServiceError::Inspection("ISO-BMFF track index is too large".into())
        })?;
        let stable = self.id.map_or_else(
            || format!("track:{index}:{stream_type}"),
            |id| format!("track:{id}"),
        );
        ProbedStream::new(
            stable,
            stream_type,
            index,
            self.codec,
            None,
            width,
            height,
            channels,
            false,
            false,
        )
        .and_then(|stream| stream.with_video_compatibility(self.profile, self.level))
        .map(Some)
        .map_err(|_| ProbeServiceError::Inspection("invalid ISO-BMFF track metadata".into()))
    }
}

#[derive(Default)]
pub struct AudioInspector;

struct AudioFacts {
    container: &'static str,
    codec: String,
    channels: Option<i32>,
    runtime_ticks: Option<i64>,
}

impl MediaInspector for AudioInspector {
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError> {
        let Some(segment) = input
            .segments
            .iter()
            .find(|segment| !segment.bytes.is_empty())
        else {
            return Err(ProbeServiceError::Inspection(
                "audio input contains no bytes".into(),
            ));
        };
        let bytes = &segment.bytes;
        let facts = if bytes.starts_with(b"fLaC") {
            parse_flac_streaminfo(bytes)?
        } else if bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WAVE" {
            parse_wav_header(bytes)?
        } else if bytes.starts_with(b"OggS") {
            parse_ogg_pages(&input)?
        } else {
            parse_mpeg_audio(&input, bytes)?
        };
        let stream = ProbedStream::new(
            "track:1:Audio",
            "Audio",
            1,
            Some(facts.codec),
            None,
            None,
            None,
            facts.channels,
            true,
            false,
        )
        .map_err(|_| ProbeServiceError::Inspection("invalid audio stream metadata".into()))?;
        Ok(ProbeResult::new(facts.container, vec![stream])
            .map_err(|_| ProbeServiceError::Inspection("invalid audio Probe result".into()))?
            .with_video(None, None)
            .with_timing(None, facts.runtime_ticks))
    }
}

fn is_audio_container(input: &ProbeInput) -> bool {
    let Some(segment) = input
        .segments
        .iter()
        .find(|segment| !segment.bytes.is_empty())
    else {
        return false;
    };
    let bytes = &segment.bytes;
    bytes.starts_with(b"fLaC")
        || (bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WAVE")
        || bytes.starts_with(b"OggS")
        || bytes.starts_with(b"ID3")
        || is_mpeg_audio_sync(bytes)
}

fn is_mpeg_audio_sync(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes[0] != 0xFF {
        return false;
    }
    let second = bytes[1];
    if second & 0xE0 != 0xE0 {
        return false;
    }
    let version = (second >> 3) & 0x3;
    let layer = (second >> 1) & 0x3;
    if layer == 0 {
        // ADTS AAC carries a 12-bit sync word and a zero layer.
        second & 0xF0 == 0xF0 && version != 1
    } else {
        version != 1
    }
}

fn duration_ticks_from_ratio(numerator: u128, denominator: u128) -> Option<i64> {
    if denominator == 0 {
        return None;
    }
    i64::try_from(numerator.saturating_mul(10_000_000) / denominator).ok()
}

fn parse_flac_streaminfo(bytes: &[u8]) -> Result<AudioFacts, ProbeServiceError> {
    let truncated =
        |what: &str| ProbeServiceError::Inspection(format!("truncated FLAC {what} header"));
    // "fLaC" then one metadata block header plus the 34-byte STREAMINFO body.
    if bytes.len() < 4 + 4 + 34 {
        return Err(truncated("STREAMINFO"));
    }
    if bytes[4] & 0x7F != 0 {
        return Err(ProbeServiceError::Inspection(
            "FLAC STREAMINFO block is not first".into(),
        ));
    }
    let packed = u64::from_be_bytes(bytes[18..26].try_into().expect("length checked"));
    let sample_rate = (packed >> 44) & 0xF_FFFF;
    let channels = bounded_i32(((packed >> 41) & 0x7) + 1)?.unwrap_or_default();
    let total_samples = packed & 0xF_FFFF_FFFF;
    let runtime_ticks = (sample_rate > 0)
        .then(|| duration_ticks_from_ratio(u128::from(total_samples), u128::from(sample_rate)))
        .flatten();
    Ok(AudioFacts {
        container: "flac",
        codec: "flac".to_owned(),
        channels: Some(channels),
        runtime_ticks,
    })
}

fn parse_wav_header(bytes: &[u8]) -> Result<AudioFacts, ProbeServiceError> {
    let mut position = 12_usize;
    let mut fmt: Option<(u16, i32, u32, u16)> = None;
    let mut data_size = None;
    while position + 8 <= bytes.len() {
        let identifier = &bytes[position..position + 4];
        let size = u32::from_le_bytes(
            bytes[position + 4..position + 8]
                .try_into()
                .expect("length checked"),
        );
        if identifier == b"fmt " && fmt.is_none() {
            fmt = Some(wav_format_chunk(&bytes[position + 8..])?);
        } else if identifier == b"data" {
            data_size = Some(u64::from(size));
        }
        position += 8 + size as usize + usize::from(size % 2 == 1);
    }
    let Some((format, channels, byte_rate, bits)) = fmt else {
        return Err(ProbeServiceError::Inspection(
            "WAV header has no fmt chunk".into(),
        ));
    };
    let codec = match (format, bits) {
        (1, 8) => "pcm_u8",
        (1, 16) => "pcm_s16le",
        (1, 24) => "pcm_s24le",
        (1, 32) => "pcm_s32le",
        (3, 32) => "pcm_f32le",
        (3, 64) => "pcm_f64le",
        (6, _) => "alaw",
        (7, _) => "mulaw",
        _ => "pcm",
    }
    .to_owned();
    let runtime_ticks = match (data_size, byte_rate) {
        (Some(data_size), byte_rate) if byte_rate > 0 => {
            duration_ticks_from_ratio(u128::from(data_size), u128::from(byte_rate))
        }
        _ => None,
    };
    Ok(AudioFacts {
        container: "wav",
        codec,
        channels: Some(channels),
        runtime_ticks,
    })
}

fn wav_format_chunk(body: &[u8]) -> Result<(u16, i32, u32, u16), ProbeServiceError> {
    if body.len() < 16 {
        return Err(ProbeServiceError::Inspection(
            "truncated WAV fmt chunk".into(),
        ));
    }
    Ok((
        u16::from_le_bytes(body[0..2].try_into().expect("length checked")),
        i32::from(u16::from_le_bytes(
            body[2..4].try_into().expect("length checked"),
        )),
        // body[4..8] carries the sample rate; only the byte rate and the bit
        // depth matter for the probe facts.
        u32::from_le_bytes(body[8..12].try_into().expect("length checked")),
        u16::from_le_bytes(body[14..16].try_into().expect("length checked")),
    ))
}

fn parse_ogg_pages(input: &ProbeInput) -> Result<AudioFacts, ProbeServiceError> {
    let head = input
        .segments
        .iter()
        .find(|segment| !segment.bytes.is_empty())
        .map(|segment| &segment.bytes[..])
        .ok_or_else(|| ProbeServiceError::Inspection("audio input contains no bytes".into()))?;
    if head.len() < 27 {
        return Err(ProbeServiceError::Inspection(
            "truncated Ogg page header".into(),
        ));
    }
    let segment_count = head[26] as usize;
    if head.len() < 27 + segment_count {
        return Err(ProbeServiceError::Inspection(
            "truncated Ogg segment table".into(),
        ));
    }
    let body_start = 27 + segment_count;
    let body = head.get(body_start..).unwrap_or(&[]);
    let (codec, container, channels, rate) =
        if body.len() >= 7 && body[0] == 0x01 && body[1..7] == *b"vorbis" {
            if body.len() < 16 {
                return Err(ProbeServiceError::Inspection(
                    "truncated Vorbis identification header".into(),
                ));
            }
            let channels = i32::from(body[11]);
            let rate = u32::from_le_bytes(body[12..16].try_into().expect("length checked"));
            ("vorbis", "ogg", Some(channels), rate)
        } else if body.len() >= 8 && body[0..8] == *b"OpusHead" {
            if body.len() < 12 {
                return Err(ProbeServiceError::Inspection(
                    "truncated OpusHead header".into(),
                ));
            }
            let channels = i32::from(body[9]);
            // Opus granule positions always advance at 48 kHz regardless of the
            // original input sample rate.
            ("opus", "opus", Some(channels), 48_000)
        } else {
            return Err(ProbeServiceError::Inspection(
                "Ogg first page carries neither Vorbis nor Opus".into(),
            ));
        };
    let last_granule = input
        .segments
        .iter()
        .flat_map(|segment| {
            (0..segment.bytes.len().saturating_sub(14))
                .filter(|offset| segment.bytes[*offset..*offset + 4] == *b"OggS")
                .map(|offset| {
                    u64::from_le_bytes(
                        segment.bytes[offset + 6..offset + 14]
                            .try_into()
                            .expect("length checked"),
                    )
                })
        })
        .max();
    let runtime_ticks = match (last_granule, rate) {
        (Some(granule), rate) if rate > 0 => {
            duration_ticks_from_ratio(u128::from(granule), u128::from(rate))
        }
        _ => None,
    };
    Ok(AudioFacts {
        container,
        codec: codec.to_owned(),
        channels,
        runtime_ticks,
    })
}

const MP3_BITRATES_V1_L3: [u32; 16] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
];
const MP3_BITRATES_V2_L3: [u32; 16] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
];

fn parse_mpeg_audio(input: &ProbeInput, bytes: &[u8]) -> Result<AudioFacts, ProbeServiceError> {
    let mut offset = 0_usize;
    if bytes.starts_with(b"ID3") {
        if bytes.len() < 10 {
            return Err(ProbeServiceError::Inspection("truncated ID3 header".into()));
        }
        let size = ((bytes[6] as usize & 0x7F) << 21)
            | ((bytes[7] as usize & 0x7F) << 14)
            | ((bytes[8] as usize & 0x7F) << 7)
            | (bytes[9] as usize & 0x7F);
        offset = 10 + size;
        if offset + 4 > bytes.len() {
            return Err(ProbeServiceError::Inspection(
                "MP3 frame header is beyond the covered input".into(),
            ));
        }
    }
    if !is_mpeg_audio_sync(&bytes[offset..]) {
        return Err(ProbeServiceError::Inspection(
            "unsupported audio frame header".into(),
        ));
    }
    let second = bytes[offset + 1];
    let version = (second >> 3) & 0x3;
    let layer = (second >> 1) & 0x3;
    if layer == 0 {
        return parse_adts_header(&bytes[offset..], input.size);
    }
    let third = bytes[offset + 2];
    let bitrate_index = ((third >> 4) & 0xF) as usize;
    let sample_rate_index = (third >> 2) & 0x3;
    let sample_rate = match (version, sample_rate_index) {
        (0b11, 0) => 44_100_u32,
        (0b11, 1) => 48_000_u32,
        (0b11, 2) => 32_000_u32,
        (0b10, 0) => 22_050_u32,
        (0b10, 1) => 24_000_u32,
        (0b10, 2) => 16_000_u32,
        (0b00, 0) => 11_025_u32,
        (0b00, 1) => 12_000_u32,
        (0b00, 2) => 8_000_u32,
        _ => {
            return Err(ProbeServiceError::Inspection(
                "reserved MP3 sample rate".into(),
            ));
        }
    };
    let mode = bytes[offset + 3] >> 6;
    let channels = if mode == 0b11 { Some(1) } else { Some(2) };
    let runtime_ticks = (layer == 0b01)
        .then(|| {
            let side_info = if version == 0b11 {
                if mode == 0b11 { 17 } else { 32 }
            } else if mode == 0b11 {
                9
            } else {
                17
            };
            mp3_layer3_runtime_ticks(
                bytes,
                offset,
                side_info,
                version,
                sample_rate,
                input.size,
                bitrate_index,
            )
        })
        .flatten();
    Ok(AudioFacts {
        container: "mp3",
        codec: "mp3".to_owned(),
        channels,
        runtime_ticks,
    })
}

#[allow(clippy::too_many_arguments)] // Fixed MP3 header positions plus the object size for the CBR estimate.
fn mp3_layer3_runtime_ticks(
    bytes: &[u8],
    offset: usize,
    side_info: usize,
    version: u8,
    sample_rate: u32,
    object_size: u64,
    bitrate_index: usize,
) -> Option<i64> {
    // Layer III: honor a Xing/Info VBR header when present, otherwise
    // estimate from the constant bitrate and the object size.
    let xing_position = offset + 4 + side_info;
    let table = if version == 0b11 {
        MP3_BITRATES_V1_L3
    } else {
        MP3_BITRATES_V2_L3
    };
    let bitrate_kbps = table[bitrate_index];
    if xing_position + 12 <= bytes.len()
        && (bytes[xing_position..xing_position + 4] == *b"Xing"
            || bytes[xing_position..xing_position + 4] == *b"Info")
    {
        let flags = u32::from_be_bytes(
            bytes[xing_position + 4..xing_position + 8]
                .try_into()
                .expect("length checked"),
        );
        if flags & 0x8000_0000 != 0 {
            let frames = u32::from_be_bytes(
                bytes[xing_position + 8..xing_position + 12]
                    .try_into()
                    .expect("length checked"),
            );
            let samples_per_frame: u32 = if version == 0b11 { 1_152 } else { 576 };
            return duration_ticks_from_ratio(
                u128::from(frames) * u128::from(samples_per_frame),
                u128::from(sample_rate),
            );
        }
        return None;
    }
    if bitrate_kbps > 0 {
        return duration_ticks_from_ratio(
            u128::from(object_size.saturating_sub(offset as u64)) * 8,
            u128::from(bitrate_kbps) * 1_000,
        );
    }
    None
}

fn parse_adts_header(bytes: &[u8], object_size: u64) -> Result<AudioFacts, ProbeServiceError> {
    if bytes.len() < 6 {
        return Err(ProbeServiceError::Inspection(
            "truncated ADTS header".into(),
        ));
    }
    let sample_rate = match (bytes[2] >> 2) & 0xF {
        0 => 96_000_u32,
        1 => 88_200_u32,
        2 => 64_000_u32,
        3 => 48_000_u32,
        4 => 44_100_u32,
        5 => 32_000_u32,
        6 => 24_000_u32,
        7 => 22_050_u32,
        8 => 16_000_u32,
        9 => 12_000_u32,
        10 => 11_025_u32,
        11 => 8_000_u32,
        12 => 7_350_u32,
        _ => {
            return Err(ProbeServiceError::Inspection(
                "reserved ADTS sample rate".into(),
            ));
        }
    };
    let channel_config = ((bytes[2] & 0x01) << 2) | (bytes[3] >> 6);
    let channels = (channel_config > 0).then_some(i32::from(channel_config));
    // ADTS carries no stream-level duration; estimate it from the first
    // frame's declared length and the object size, assuming AAC-LC frames of
    // 1 024 samples each.
    let frame_length = ((u32::from(bytes[3] & 0x03)) << 11)
        | (u32::from(bytes[4]) << 3)
        | (u32::from(bytes[5]) >> 5);
    let runtime_ticks = (frame_length >= 8)
        .then(|| {
            let frames = object_size / u64::from(frame_length);
            duration_ticks_from_ratio(u128::from(frames) * 1_024, u128::from(sample_rate))
        })
        .flatten();
    Ok(AudioFacts {
        container: "aac",
        codec: "aac".to_owned(),
        channels,
        runtime_ticks,
    })
}

#[derive(Clone, Copy)]
struct IsoBox {
    kind: [u8; 4],
    body_start: u64,
    end: u64,
}

fn is_iso_bmff(input: &ProbeInput) -> bool {
    input.segments.first().is_some_and(|segment| {
        segment.bytes.len() >= 8 && matches!(&segment.bytes[4..8], b"ftyp" | b"moov" | b"moof")
    })
}

/// One contiguous byte range covered by a probe input's segments.
struct ProbeExtent {
    start: u64,
    end: u64,
}

fn probe_extents(input: &ProbeInput) -> Vec<ProbeExtent> {
    let mut segments = input.segments.clone();
    segments.sort_by_key(|segment| segment.start);
    let mut extents: Vec<ProbeExtent> = Vec::new();
    for segment in segments {
        let start = segment.start;
        let end = segment.start.saturating_add(segment.bytes.len() as u64);
        match extents.last_mut() {
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => extents.push(ProbeExtent { start, end }),
        }
    }
    extents
}

fn find_moov_range(input: &ProbeInput) -> Result<Option<(u64, u64)>, ProbeServiceError> {
    let extents = probe_extents(input);
    for segment in &input.segments {
        for offset in 0..=segment.bytes.len().saturating_sub(8) {
            if segment.bytes.get(offset + 4..offset + 8) != Some(b"moov") {
                continue;
            }
            let Some(size_bytes) = segment.bytes.get(offset..offset + 4) else {
                continue;
            };
            let Some(size_words) = size_bytes.try_into().ok() else {
                continue;
            };
            let size = u64::from(u32::from_be_bytes(size_words));
            if size < 8 {
                continue;
            }
            let Some(start) = segment.start.checked_add(offset as u64) else {
                continue;
            };
            let Some(end) = start.checked_add(size) else {
                continue;
            };
            let segment_end = segment.start + segment.bytes.len() as u64;
            let covered_end = extents
                .iter()
                .find(|extent| extent.start <= start && start < extent.end)
                .map_or(segment_end, |extent| extent.end.max(segment_end));
            if end <= covered_end {
                return Ok(Some((start, end)));
            }
            // The moov box is only partially covered. Surface the first
            // uncovered offset so the probe service gap-retry ladder fetches
            // exactly the missing window instead of failing permanently.
            return Err(ProbeServiceError::Inspection(format!(
                "Probe byte budget gap at offset {covered_end}"
            )));
        }
    }
    Ok(None)
}

const MAX_BOX_DEPTH: u32 = 64;

fn parse_iso_boxes(
    reader: &mut SparseProbeReader,
    end: u64,
    movie: &mut IsoBmffMovie,
    current_track: Option<usize>,
    depth: u32,
) -> Result<(), ProbeServiceError> {
    if depth > MAX_BOX_DEPTH {
        return Err(ProbeServiceError::Inspection(
            "ISO-BMFF box nesting is too deep".into(),
        ));
    }
    while reader.position < end {
        let header = read_iso_box(reader, end)?;
        if header.kind == *b"moov"
            || header.kind == *b"mdia"
            || header.kind == *b"minf"
            || header.kind == *b"stbl"
        {
            parse_iso_boxes(reader, header.end, movie, current_track, depth + 1)?;
        } else if header.kind == *b"trak" {
            movie.tracks.push(IsoBmffTrack::default());
            let track = movie.tracks.len() - 1;
            parse_iso_boxes(reader, header.end, movie, Some(track), depth + 1)?;
        } else if header.kind == *b"mvhd" {
            parse_mvhd(&read_iso_prefix(reader, header, 32)?, movie)?;
        } else if header.kind == *b"tkhd" {
            if let Some(track) = current_track {
                movie.tracks[track].id = parse_tkhd(&read_iso_prefix(reader, header, 32)?)?;
            } else {
                reader.position = header.end;
            }
        } else if header.kind == *b"hdlr" {
            if let Some(track) = current_track {
                movie.tracks[track].handler = parse_handler(&read_iso_prefix(reader, header, 12)?)?;
            } else {
                reader.position = header.end;
            }
        } else if header.kind == *b"stsd" {
            if let Some(track) = current_track {
                parse_sample_description(
                    &read_iso_prefix(reader, header, 512)?,
                    &mut movie.tracks[track],
                )?;
            } else {
                reader.position = header.end;
            }
        } else {
            reader.position = header.end;
        }
    }
    if reader.position != end {
        return Err(ProbeServiceError::Inspection(
            "ISO-BMFF box exceeds its parent".into(),
        ));
    }
    Ok(())
}

fn read_iso_box(reader: &mut SparseProbeReader, end: u64) -> Result<IsoBox, ProbeServiceError> {
    if end.saturating_sub(reader.position) < 8 {
        return Err(ProbeServiceError::Inspection(
            "truncated ISO-BMFF box header".into(),
        ));
    }
    let start = reader.position;
    let mut header = [0_u8; 8];
    reader
        .read_exact(&mut header)
        .map_err(|error| probe_reader_error(&error, "incomplete ISO-BMFF box header"))?;
    let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().unwrap()));
    let mut header_size = 8_u64;
    if size == 1 {
        let mut extended = [0_u8; 8];
        reader.read_exact(&mut extended).map_err(|error| {
            probe_reader_error(&error, "incomplete ISO-BMFF extended box header")
        })?;
        size = u64::from_be_bytes(extended);
        header_size = 16;
    } else if size == 0 {
        size = end - start;
    }
    let Some(box_end) = start.checked_add(size) else {
        return Err(ProbeServiceError::Inspection(
            "invalid ISO-BMFF box size".into(),
        ));
    };
    if size < header_size {
        return Err(ProbeServiceError::Inspection(
            "invalid ISO-BMFF box size".into(),
        ));
    }
    if box_end > end {
        return Err(ProbeServiceError::Inspection(
            "invalid ISO-BMFF box size".into(),
        ));
    }
    Ok(IsoBox {
        kind: header[4..8].try_into().unwrap(),
        body_start: start + header_size,
        end: box_end,
    })
}

fn read_iso_prefix(
    reader: &mut SparseProbeReader,
    header: IsoBox,
    limit: usize,
) -> Result<Vec<u8>, ProbeServiceError> {
    let length = usize::try_from((header.end - header.body_start).min(limit as u64))
        .map_err(|_| ProbeServiceError::Inspection("ISO-BMFF box is too large".into()))?;
    let mut body = vec![0_u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| probe_reader_error(&error, "incomplete ISO-BMFF box body"))?;
    reader.position = header.end;
    Ok(body)
}

/// Maps a sparse reader failure to an inspection error, preserving the retry
/// ladder's gap marker so partially covered inputs request targeted fills.
fn probe_reader_error(error: &io::Error, fallback: &str) -> ProbeServiceError {
    let message = error.to_string();
    if message.contains("Probe byte budget gap") {
        ProbeServiceError::Inspection(message)
    } else {
        ProbeServiceError::Inspection(fallback.to_owned())
    }
}

fn parse_mvhd(body: &[u8], movie: &mut IsoBmffMovie) -> Result<(), ProbeServiceError> {
    let version = *body
        .first()
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF movie header".into()))?;
    let (timescale_offset, duration_offset, duration_length) = match version {
        0 => (12, 16, 4),
        1 => (20, 24, 8),
        _ => {
            return Err(ProbeServiceError::Inspection(
                "unsupported ISO-BMFF movie version".into(),
            ));
        }
    };
    movie.timescale = Some(u64::from(read_be_u32(body, timescale_offset)?));
    movie.duration = Some(read_be_uint(body, duration_offset, duration_length)?);
    Ok(())
}

fn parse_tkhd(body: &[u8]) -> Result<Option<u32>, ProbeServiceError> {
    let version = *body
        .first()
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF track header".into()))?;
    let offset = match version {
        0 => 12,
        1 => 20,
        _ => {
            return Err(ProbeServiceError::Inspection(
                "unsupported ISO-BMFF track version".into(),
            ));
        }
    };
    Ok(Some(read_be_u32(body, offset)?))
}

fn parse_handler(body: &[u8]) -> Result<Option<[u8; 4]>, ProbeServiceError> {
    let handler = body
        .get(8..12)
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF handler".into()))?
        .try_into()
        .unwrap();
    Ok(Some(handler))
}

fn parse_sample_description(
    body: &[u8],
    track: &mut IsoBmffTrack,
) -> Result<(), ProbeServiceError> {
    if read_be_u32(body, 4)? == 0 {
        return Ok(());
    }
    let entry_size = read_be_u32(body, 8)? as usize;
    let minimum_entry_size = if track.handler == Some(*b"vide") {
        36
    } else if track.handler == Some(*b"soun") {
        26
    } else {
        8
    };
    if entry_size < minimum_entry_size || body.len() < 16 {
        return Err(ProbeServiceError::Inspection(
            "truncated ISO-BMFF sample entry".into(),
        ));
    }
    let kind: [u8; 4] = body[12..16].try_into().unwrap();
    track.codec = Some(normalize_iso_codec(kind));
    if track.handler == Some(*b"vide") {
        track.width = Some(i32::from(read_be_u16(body, 40)?));
        track.height = Some(i32::from(read_be_u16(body, 42)?));
        let entry_end = 8_usize
            .checked_add(entry_size)
            .ok_or_else(|| ProbeServiceError::Inspection("invalid sample entry size".into()))?;
        if let Some(entry) = body.get(8..entry_end) {
            (track.profile, track.level) = sample_entry_compatibility(kind, entry);
        }
    } else if track.handler == Some(*b"soun") {
        track.channels = Some(i32::from(read_be_u16(body, 32)?));
    }
    Ok(())
}

fn sample_entry_compatibility(codec: [u8; 4], entry: &[u8]) -> (Option<String>, Option<i32>) {
    let mut offset = 86_usize;
    while let Some(header) = entry.get(offset..offset.saturating_add(8)) {
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        if size < 8 {
            break;
        }
        let Some(end) = offset.checked_add(size) else {
            break;
        };
        let Some(body) = entry.get(offset + 8..end) else {
            break;
        };
        let kind = &header[4..8];
        if matches!((&codec, kind), (b"avc1" | b"avc3", b"avcC"))
            || matches!((&codec, kind), (b"hvc1" | b"hev1", b"hvcC"))
        {
            return codec_compatibility(&normalize_iso_codec(codec), Some(body));
        }
        offset = end;
    }
    (None, None)
}

fn codec_compatibility(codec: &str, configuration: Option<&[u8]>) -> (Option<String>, Option<i32>) {
    let Some(configuration) = configuration else {
        return (None, None);
    };
    match codec {
        "h264" if configuration.first() == Some(&1) => {
            let profile = configuration.get(1).and_then(|profile| match profile {
                66 => Some("Baseline"),
                77 => Some("Main"),
                88 => Some("Extended"),
                100 => Some("High"),
                110 => Some("High 10"),
                122 => Some("High 4:2:2"),
                244 => Some("High 4:4:4 Predictive"),
                _ => None,
            });
            (
                profile.map(str::to_owned),
                configuration.get(3).copied().map(i32::from),
            )
        }
        "hevc" if configuration.first() == Some(&1) => {
            let profile = configuration
                .get(1)
                .map(|value| value & 0x1f)
                .and_then(|profile| match profile {
                    1 => Some("Main"),
                    2 => Some("Main 10"),
                    3 => Some("Main Still Picture"),
                    _ => None,
                });
            (
                profile.map(str::to_owned),
                configuration.get(12).copied().map(i32::from),
            )
        }
        _ => (None, None),
    }
}

fn normalize_iso_codec(kind: [u8; 4]) -> String {
    match &kind {
        b"avc1" | b"avc3" => "h264".to_owned(),
        b"hvc1" | b"hev1" => "hevc".to_owned(),
        b"vp09" => "vp9".to_owned(),
        b"av01" => "av1".to_owned(),
        b"mp4a" => "aac".to_owned(),
        b"Opus" | b"opus" => "opus".to_owned(),
        b"fLaC" | b"flac" => "flac".to_owned(),
        b"ac-3" => "ac3".to_owned(),
        b"ec-3" => "eac3".to_owned(),
        b"tx3g" => "mov_text".to_owned(),
        b"wvtt" => "webvtt".to_owned(),
        other => String::from_utf8_lossy(other).to_ascii_lowercase(),
    }
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Result<u16, ProbeServiceError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_be_bytes)
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF field".into()))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Result<u32, ProbeServiceError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF field".into()))
}

fn read_be_uint(bytes: &[u8], offset: usize, length: usize) -> Result<u64, ProbeServiceError> {
    let field = bytes
        .get(offset..offset + length)
        .ok_or_else(|| ProbeServiceError::Inspection("truncated ISO-BMFF field".into()))?;
    let mut value = 0_u64;
    for byte in field {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

pub struct ProbeService {
    database: sea_orm::DatabaseConnection,
    backends: StorageBackendRegistry,
    inspector: Arc<dyn MediaInspector>,
}

impl ProbeService {
    #[must_use]
    pub fn new(database: sea_orm::DatabaseConnection) -> Self {
        Self {
            database,
            backends: StorageBackendRegistry::new(),
            inspector: Arc::new(DefaultMediaInspector),
        }
    }

    #[must_use]
    pub fn with_backend_registry(mut self, backends: StorageBackendRegistry) -> Self {
        self.backends = backends;
        self
    }

    #[must_use]
    pub fn with_backend<Backend>(self, account_id: Uuid, backend: Arc<Backend>) -> Self
    where
        Backend: StorageBackend + 'static,
    {
        self.backends.insert_unscoped(account_id, backend);
        self
    }

    /// Binds a provider-neutral backend selected at runtime.
    #[must_use]
    pub fn with_dyn_backend(self, account_id: Uuid, backend: Arc<dyn StorageBackend>) -> Self {
        self.backends.insert_unscoped(account_id, backend);
        self
    }

    #[must_use]
    pub fn with_inspector(mut self, inspector: Arc<dyn MediaInspector>) -> Self {
        self.inspector = inspector;
        self
    }

    /// Executes one claimed Probe with bounded reads and before/after revision checks.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeServiceError`] without completing the job when the backend,
    /// inspection, snapshot, or fenced commit fails.
    #[allow(clippy::too_many_lines)] // Keeps descriptor and target snapshot fencing in one Probe transaction flow.
    pub async fn execute(&self, claimed: &ClaimedWorkJob) -> Result<i64, ProbeServiceError> {
        let repository = ProbeRepository::new(&self.database);
        let candidate = repository
            .candidate(claimed)
            .await?
            .ok_or(ProbeServiceError::CandidateUnavailable)?;
        tracing::debug!(
            storage_object_id = %candidate.storage_object_id().as_uuid(),
            catalog_item_id = %candidate.item_id().as_uuid(),
            provider = candidate.provider(),
            locator_kind = candidate.locator_kind(),
            size = candidate.size(),
            revision = candidate.location_revision(),
            stage = "candidate",
            "media probe candidate selected"
        );
        let backend = self
            .backends
            .backend(candidate.storage_account_id())
            .ok_or(ProbeServiceError::BackendUnavailable)?;
        let object_id = StorageObjectId::new(
            candidate.provider().to_owned(),
            candidate.provider_object_id().to_owned(),
        )?;
        ensure_probe_candidate(&repository, claimed, &candidate).await?;
        let before = storage_read::get_object(
            &self.database,
            backend.as_ref(),
            candidate.storage_object_id(),
            &object_id,
        )
        .await
        .map_err(probe_storage_read_error)?;
        validate_object_snapshot(&candidate, &before)?;
        let mut probe_object_id = object_id.clone();
        let mut probe_size = candidate.size();
        let mut probe_record_id = Some(candidate.storage_object_id());
        let mut probe_backend = Arc::clone(&backend);
        let mut target_before = None;
        let mut probe_location_revision = candidate.location_revision().to_owned();
        if candidate.locator_kind() == "strm" {
            let descriptor_size = usize::try_from(candidate.size()).unwrap_or(usize::MAX);
            if descriptor_size > MAX_STRM_BYTES {
                let message = "STRM descriptor exceeds 8 KiB".to_owned();
                repository
                    .commit_failure(claimed, &candidate, &message)
                    .await?;
                return Err(ProbeServiceError::InspectionFailed(message));
            }
            let descriptor = read_exact_range(
                &self.database,
                backend.as_ref(),
                Some(candidate.storage_object_id()),
                &object_id,
                0,
                candidate.size(),
                &repository,
                claimed,
                &candidate,
            )
            .await?;
            let target = match parse_strm(&descriptor) {
                Ok(target) => target,
                Err(error) => {
                    let message = error.to_string();
                    repository
                        .commit_failure(claimed, &candidate, &message)
                        .await?;
                    return Err(ProbeServiceError::InspectionFailed(message));
                }
            };
            tracing::debug!(
                storage_object_id = %candidate.storage_object_id().as_uuid(),
                stage = "strm_target_resolved",
                target_hash = %format!("{:x}", Sha256::digest(target.as_bytes())),
                "STRM target resolved with redacted reference"
            );
            let allowed_accounts = CatalogPublicationRepository::new(&self.database)
                .playback_storage_accounts(candidate.item_id())
                .await?;
            let resolved = match self
                .backends
                .resolve_local_reference(
                    candidate.storage_account_id(),
                    &allowed_accounts,
                    &object_id,
                    target,
                )
                .await
            {
                Ok(target) => target,
                Err(error) => {
                    let message = format!("STRM target is unavailable: {error}");
                    repository
                        .commit_failure(claimed, &candidate, &message)
                        .await?;
                    return Err(ProbeServiceError::InspectionFailed(message));
                }
            };
            let Some(target_size) = resolved.object.size() else {
                let message = "STRM target is not a regular file".to_owned();
                repository
                    .commit_failure(claimed, &candidate, &message)
                    .await?;
                return Err(ProbeServiceError::InspectionFailed(message));
            };
            tracing::debug!(
                storage_object_id = %candidate.storage_object_id().as_uuid(),
                stage = "strm_target_metadata",
                target_size,
                "STRM target metadata resolved"
            );
            probe_size = target_size;
            probe_location_revision = strm_probe_revision(
                &candidate,
                resolved.account_id,
                &resolved.object,
                target_size,
            );
            probe_object_id = resolved.object.id().clone();
            probe_record_id = None;
            probe_backend = resolved.backend;
            target_before = Some(resolved.object);
        }
        let input = read_probe_input(
            &self.database,
            probe_backend.as_ref(),
            probe_record_id,
            &probe_object_id,
            probe_size,
            &repository,
            claimed,
            &candidate,
        )
        .await?;
        let mut probe_input = input;
        let mut result = self.inspector.inspect(probe_input.clone());
        let mut gap_retries = 0;
        while let Err(ProbeServiceError::Inspection(message)) = &result {
            let Some(gap_offset) = probe_gap_offset(message) else {
                break;
            };
            if gap_retries >= MAX_GAP_RETRIES || gap_offset >= probe_size {
                break;
            }
            let start = gap_offset.saturating_sub(GAP_RETRY_RANGE / 2);
            let end = probe_size.min(start.saturating_add(GAP_RETRY_RANGE));
            let bytes = read_exact_range(
                &self.database,
                probe_backend.as_ref(),
                probe_record_id,
                &probe_object_id,
                start,
                end,
                &repository,
                claimed,
                &candidate,
            )
            .await?;
            probe_input.segments.push(ProbeSegment { start, bytes });
            probe_input.segments.sort_by_key(|segment| segment.start);
            gap_retries += 1;
            tracing::debug!(
                size = probe_size,
                gap_offset,
                start,
                end,
                retry = gap_retries,
                "media probe filled sparse read gap"
            );
            result = self.inspector.inspect(probe_input.clone());
        }
        if matches!(
            &result,
            Err(ProbeServiceError::Inspection(message))
                if message.contains("Probe byte budget gap")
        ) {
            for &range_budget in ADAPTIVE_RANGE_BUDGETS {
                if probe_size <= range_budget * 2 {
                    break;
                }
                let mut retry_input = read_probe_input_with_budget(
                    &self.database,
                    probe_backend.as_ref(),
                    probe_record_id,
                    &probe_object_id,
                    probe_size,
                    range_budget,
                    &repository,
                    claimed,
                    &candidate,
                )
                .await?;
                retry_input
                    .segments
                    .extend(probe_input.segments.iter().cloned());
                retry_input.segments.sort_by_key(|segment| segment.start);
                probe_input = retry_input.clone();
                result = self.inspector.inspect(retry_input);
                if !matches!(
                    &result,
                    Err(ProbeServiceError::Inspection(message))
                        if message.contains("Probe byte budget gap")
                ) {
                    break;
                }
            }
            if matches!(
                &result,
                Err(ProbeServiceError::Inspection(message))
                    if message.contains("Probe byte budget gap")
            ) && probe_size <= SEQUENTIAL_FALLBACK_MAX
            {
                let full_input = read_exact_probe_input(
                    &self.database,
                    probe_backend.as_ref(),
                    probe_record_id,
                    &probe_object_id,
                    probe_size,
                    &repository,
                    claimed,
                    &candidate,
                )
                .await?;
                result = self.inspector.inspect(full_input);
            }
        }
        let result = match result {
            Ok(result) => Ok(result),
            Err(ProbeServiceError::Inspection(message)) => {
                repository
                    .commit_failure(claimed, &candidate, &message)
                    .await?;
                return Err(ProbeServiceError::InspectionFailed(message));
            }
            Err(error) => return Err(error),
        };
        let result = match result {
            Ok(result) => result,
            Err(ProbeServiceError::Inspection(message)) => {
                repository
                    .commit_failure(claimed, &candidate, &message)
                    .await?;
                return Err(ProbeServiceError::InspectionFailed(message));
            }
            Err(error) => return Err(error),
        };
        ensure_probe_candidate(&repository, claimed, &candidate).await?;
        let after = storage_read::get_object(
            &self.database,
            backend.as_ref(),
            candidate.storage_object_id(),
            &object_id,
        )
        .await
        .map_err(probe_storage_read_error)?;
        validate_object_snapshot(&candidate, &after)?;
        if object_revision(&before) != object_revision(&after) || before.size() != after.size() {
            return Err(ProbeServiceError::ObjectChanged);
        }
        if let Some(target_before) = target_before {
            let target_after = probe_backend.get_object(&probe_object_id).await?;
            if object_revision(&target_before) != object_revision(&target_after)
                || target_before.size() != target_after.size()
            {
                return Err(ProbeServiceError::ObjectChanged);
            }
        }
        repository
            .commit_success_with_location_revision(
                claimed,
                &candidate,
                &result,
                &probe_location_revision,
            )
            .await
            .map_err(Into::into)
    }
}

fn strm_probe_revision(
    candidate: &ProbeCandidate,
    target_account_id: Uuid,
    target: &StorageObject,
    target_size: u64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(candidate.location_revision().as_bytes());
    digest.update(target_account_id.as_bytes());
    digest.update(target.id().provider().as_bytes());
    digest.update(target.id().provider_object_id().as_bytes());
    digest.update(target.remote_revision().unwrap_or_default().as_bytes());
    digest.update(target_size.to_be_bytes());
    format!("strm:{:x}", digest.finalize())
}

#[derive(Debug, Error)]
pub enum ProbeServiceError {
    #[error("Probe candidate is no longer active or authorized")]
    CandidateUnavailable,
    #[error("storage backend is not configured")]
    BackendUnavailable,
    #[error("storage object changed while probing")]
    ObjectChanged,
    #[error("media inspection failed: {0}")]
    Inspection(String),
    #[error("media inspection failed and was recorded: {0}")]
    InspectionFailed(String),
    #[error("storage operation failed: {0}")]
    Storage(#[from] BackendError),
    #[error("Probe storage availability persistence failed: {0}")]
    Availability(#[from] StorageSyncRepositoryError),
    #[error("Probe storage availability projection failed: {0}")]
    AvailabilityProjection(#[from] StorageChangeProjectorError),
    #[error("Probe persistence failed: {0}")]
    Repository(#[from] ProbeRepositoryError),
    #[error("Probe catalog publication lookup failed: {0}")]
    Publication(#[from] CatalogPublicationError),
}

#[allow(clippy::too_many_arguments)] // Each backend read is fenced by the durable claim and exact SQL candidate.
async fn read_probe_input(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: Option<tjxy_common::StorageObjectRecordId>,
    object_id: &StorageObjectId,
    size: u64,
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    candidate: &ProbeCandidate,
) -> Result<ProbeInput, ProbeServiceError> {
    read_probe_input_with_budget(
        database,
        backend,
        record_id,
        object_id,
        size,
        RANGE_BUDGET,
        repository,
        claimed,
        candidate,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn read_probe_input_with_budget(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: Option<tjxy_common::StorageObjectRecordId>,
    object_id: &StorageObjectId,
    size: u64,
    range_budget: u64,
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    candidate: &ProbeCandidate,
) -> Result<ProbeInput, ProbeServiceError> {
    let segments = if size <= range_budget * 2 {
        vec![ProbeSegment {
            start: 0,
            bytes: read_exact_range(
                database, backend, record_id, object_id, 0, size, repository, claimed, candidate,
            )
            .await?,
        }]
    } else {
        vec![
            ProbeSegment {
                start: 0,
                bytes: read_exact_range(
                    database,
                    backend,
                    record_id,
                    object_id,
                    0,
                    range_budget,
                    repository,
                    claimed,
                    candidate,
                )
                .await?,
            },
            ProbeSegment {
                start: size - range_budget,
                bytes: read_exact_range(
                    database,
                    backend,
                    record_id,
                    object_id,
                    size - range_budget,
                    size,
                    repository,
                    claimed,
                    candidate,
                )
                .await?,
            },
        ]
    };
    Ok(ProbeInput { size, segments })
}

#[allow(clippy::too_many_arguments)]
async fn read_exact_probe_input(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: Option<tjxy_common::StorageObjectRecordId>,
    object_id: &StorageObjectId,
    size: u64,
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    candidate: &ProbeCandidate,
) -> Result<ProbeInput, ProbeServiceError> {
    Ok(ProbeInput {
        size,
        segments: vec![ProbeSegment {
            start: 0,
            bytes: read_exact_range(
                database, backend, record_id, object_id, 0, size, repository, claimed, candidate,
            )
            .await?,
        }],
    })
}

#[allow(clippy::too_many_arguments)] // Range reads repeat the same claim, candidate, and object authorization fence.
async fn read_exact_range(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: Option<tjxy_common::StorageObjectRecordId>,
    object_id: &StorageObjectId,
    start: u64,
    end: u64,
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    candidate: &ProbeCandidate,
) -> Result<Vec<u8>, ProbeServiceError> {
    if start == end {
        return Ok(Vec::new());
    }
    ensure_probe_candidate(repository, claimed, candidate).await?;
    let range = ByteRange::new(start, end)?;
    let mut stream = if let Some(record_id) = record_id {
        storage_read::open_range(
            database,
            backend,
            record_id,
            object_id,
            range,
            &storage_read::ReadAvailabilityThrottle::unthrottled(),
        )
        .await
        .map_err(probe_storage_read_error)?
    } else {
        backend.open_range(object_id, range).await?
    };
    let expected = usize::try_from(end - start)
        .map_err(|_| ProbeServiceError::Inspection("Probe range is too large".into()))?;
    let mut bytes = Vec::with_capacity(expected);
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
        if bytes.len() > expected {
            return Err(ProbeServiceError::Inspection(
                "backend exceeded the requested Probe range".into(),
            ));
        }
    }
    if bytes.len() != expected {
        return Err(ProbeServiceError::Inspection(
            "backend returned an incomplete Probe range".into(),
        ));
    }
    Ok(bytes)
}

async fn ensure_probe_candidate(
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    expected: &ProbeCandidate,
) -> Result<(), ProbeServiceError> {
    let current = repository
        .candidate(claimed)
        .await?
        .ok_or(ProbeServiceError::CandidateUnavailable)?;
    if &current != expected {
        return Err(ProbeServiceError::CandidateUnavailable);
    }
    Ok(())
}

fn probe_storage_read_error(error: StorageReadError) -> ProbeServiceError {
    match error {
        StorageReadError::Backend(error) => ProbeServiceError::Storage(error),
        StorageReadError::Availability(error) => ProbeServiceError::Availability(error),
        StorageReadError::Projection(error) => ProbeServiceError::AvailabilityProjection(error),
    }
}

fn probe_gap_offset(message: &str) -> Option<u64> {
    message
        .strip_prefix("Probe byte budget gap at offset ")?
        .parse()
        .ok()
}

fn validate_object_snapshot(
    candidate: &ProbeCandidate,
    object: &StorageObject,
) -> Result<(), ProbeServiceError> {
    if object.size() != Some(candidate.size())
        || candidate
            .remote_revision()
            .is_some_and(|revision| object.remote_revision() != Some(revision))
    {
        return Err(ProbeServiceError::ObjectChanged);
    }
    Ok(())
}

fn object_revision(object: &StorageObject) -> (Option<&str>, Option<&str>, Option<&str>) {
    (object.remote_revision(), object.etag(), object.checksum())
}

fn bounded_i32(value: u64) -> Result<Option<i32>, ProbeServiceError> {
    i32::try_from(value)
        .map(Some)
        .map_err(|_| ProbeServiceError::Inspection("track metadata is too large".into()))
}

fn normalize_codec(codec_id: &str) -> String {
    match codec_id {
        "V_MPEG4/ISO/AVC" => "h264".to_owned(),
        "V_MPEGH/ISO/HEVC" => "hevc".to_owned(),
        "V_MPEG4/ISO/ASP" => "mpeg4".to_owned(),
        "V_MPEG2" => "mpeg2video".to_owned(),
        "V_MPEG1" => "mpeg1video".to_owned(),
        "V_MS/VFW/FOURCC" => "vc1".to_owned(),
        "V_VP9" => "vp9".to_owned(),
        "V_VP8" => "vp8".to_owned(),
        "V_AV1" => "av1".to_owned(),
        "V_THEORA" => "theora".to_owned(),
        "A_AAC" | "A_AAC/MPEG4/LC" => "aac".to_owned(),
        "A_OPUS" => "opus".to_owned(),
        "A_VORBIS" => "vorbis".to_owned(),
        "A_AC3" => "ac3".to_owned(),
        "A_EAC3" => "eac3".to_owned(),
        "A_DTS" => "dts".to_owned(),
        "A_TRUEHD" => "truehd".to_owned(),
        "A_FLAC" => "flac".to_owned(),
        "A_ALAC" => "alac".to_owned(),
        "A_MPEG/L3" => "mp3".to_owned(),
        "S_TEXT/UTF8" => "subrip".to_owned(),
        "S_TEXT/WEBVTT" => "webvtt".to_owned(),
        "S_ASS" => "ass".to_owned(),
        "S_SSA" => "ssa".to_owned(),
        "S_HDMV/PGS" => "pgssub".to_owned(),
        "S_VOBSUB" => "dvdsub".to_owned(),
        other => other
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect(),
    }
}

struct SparseProbeReader {
    size: u64,
    segments: Vec<ProbeSegment>,
    position: u64,
}

impl Read for SparseProbeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position == self.size {
            return Ok(0);
        }
        let segment = self
            .segments
            .iter()
            .find(|segment| {
                self.position >= segment.start
                    && self.position < segment.start + segment.bytes.len() as u64
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("Probe byte budget gap at offset {}", self.position),
                )
            })?;
        let offset = usize::try_from(self.position - segment.start)
            .map_err(|_| io::Error::other("Probe offset overflow"))?;
        let available = &segment.bytes[offset..];
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for SparseProbeReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::End(offset) => i128::from(self.size) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if !(0..=i128::from(self.size)).contains(&target) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Probe seek is outside the object",
            ));
        }
        self.position =
            u64::try_from(target).map_err(|_| io::Error::other("Probe seek overflow"))?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_reader_allows_cached_seeks_and_rejects_budget_gaps() {
        let mut reader = SparseProbeReader {
            size: 10,
            segments: vec![
                ProbeSegment {
                    start: 0,
                    bytes: b"abc".to_vec(),
                },
                ProbeSegment {
                    start: 7,
                    bytes: b"xyz".to_vec(),
                },
            ],
            position: 0,
        };
        let mut bytes = [0_u8; 3];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"abc");
        reader.seek(SeekFrom::End(-3)).unwrap();
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"xyz");
        reader.seek(SeekFrom::Start(4)).unwrap();
        let error = reader.read(&mut bytes).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(error.to_string(), "Probe byte budget gap at offset 4");
    }

    #[test]
    fn parses_sparse_probe_gap_offsets() {
        assert_eq!(
            probe_gap_offset("Probe byte budget gap at offset 42"),
            Some(42)
        );
        assert_eq!(probe_gap_offset("Probe byte budget gap"), None);
        assert_eq!(
            probe_gap_offset("Probe byte budget gap at offset nope"),
            None
        );
    }

    #[test]
    fn codec_ids_are_normalized_for_jellyfin_fields() {
        assert_eq!(normalize_codec("V_MPEG4/ISO/AVC"), "h264");
        assert_eq!(normalize_codec("A_OPUS"), "opus");
        assert_eq!(normalize_codec("X CUSTOM/CODEC"), "x_custom_codec");
    }

    #[test]
    fn matroska_codec_ids_map_to_ffprobe_codec_names() {
        assert_eq!(normalize_codec("V_MPEG2"), "mpeg2video");
        assert_eq!(normalize_codec("V_MPEG1"), "mpeg1video");
        assert_eq!(normalize_codec("V_MPEG4/ISO/ASP"), "mpeg4");
        assert_eq!(normalize_codec("V_MS/VFW/FOURCC"), "vc1");
        assert_eq!(normalize_codec("V_VP8"), "vp8");
        assert_eq!(normalize_codec("V_THEORA"), "theora");
        assert_eq!(normalize_codec("A_AAC/MPEG4/LC"), "aac");
        assert_eq!(normalize_codec("A_AC3"), "ac3");
        assert_eq!(normalize_codec("A_EAC3"), "eac3");
        assert_eq!(normalize_codec("A_DTS"), "dts");
        assert_eq!(normalize_codec("A_TRUEHD"), "truehd");
        assert_eq!(normalize_codec("A_FLAC"), "flac");
        assert_eq!(normalize_codec("A_ALAC"), "alac");
        assert_eq!(normalize_codec("A_MPEG/L3"), "mp3");
        assert_eq!(normalize_codec("S_ASS"), "ass");
        assert_eq!(normalize_codec("S_SSA"), "ssa");
        assert_eq!(normalize_codec("S_HDMV/PGS"), "pgssub");
        assert_eq!(normalize_codec("S_VOBSUB"), "dvdsub");
    }

    #[test]
    fn iso_codec_tags_map_to_ffprobe_codec_names() {
        assert_eq!(normalize_iso_codec(*b"ac-3"), "ac3");
        assert_eq!(normalize_iso_codec(*b"ec-3"), "eac3");
        assert_eq!(normalize_iso_codec(*b"fLaC"), "flac");
        assert_eq!(normalize_iso_codec(*b"tx3g"), "mov_text");
        assert_eq!(normalize_iso_codec(*b"avc1"), "h264");
        assert_eq!(normalize_iso_codec(*b"mp4a"), "aac");
    }

    #[test]
    fn matroska_inspector_parses_tracks_from_a_bounded_complete_input() {
        fn element(id: &[u8], payload: &[u8]) -> Vec<u8> {
            assert!(payload.len() < 127);
            let mut bytes = id.to_vec();
            bytes.push(0x80 | u8::try_from(payload.len()).unwrap());
            bytes.extend_from_slice(payload);
            bytes
        }

        let mut video = Vec::new();
        video.extend(element(&[0xB0], &[0x01, 0x40]));
        video.extend(element(&[0xBA], &[0xB4]));
        let mut track = Vec::new();
        track.extend(element(&[0xD7], &[1]));
        track.extend(element(&[0x73, 0xC5], &[7]));
        track.extend(element(&[0x83], &[1]));
        track.extend(element(&[0x86], b"V_MPEG4/ISO/AVC"));
        track.extend(element(&[0x63, 0xA2], &[1, 100, 0, 41]));
        track.extend(element(&[0xE0], &video));
        let tracks = element(&[0x16, 0x54, 0xAE, 0x6B], &element(&[0xAE], &track));
        let mut segment_payload = element(&[0x15, 0x49, 0xA9, 0x66], &[]);
        segment_payload.extend(tracks);
        let mut bytes = element(&[0x1A, 0x45, 0xDF, 0xA3], &[]);
        bytes.extend(element(&[0x18, 0x53, 0x80, 0x67], &segment_payload));

        let result = MatroskaInspector
            .inspect(ProbeInput {
                size: bytes.len() as u64,
                segments: vec![ProbeSegment { start: 0, bytes }],
            })
            .unwrap();
        assert_eq!(result.container(), "mkv");
        assert_eq!(result.streams().len(), 1);
        assert_eq!(result.streams()[0].stream_type(), "Video");
        assert_eq!(result.streams()[0].codec(), Some("h264"));
        assert_eq!(result.streams()[0].profile(), Some("High"));
        assert_eq!(result.streams()[0].level(), Some(41));
    }

    #[test]
    fn default_inspector_parses_iso_bmff_video_and_audio_tracks() {
        fn atom(kind: impl AsRef<[u8]>, payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.extend(payload);
            bytes
        }
        fn sample_entry(kind: impl AsRef<[u8]>, mut payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.append(&mut payload);
            bytes
        }
        fn track(id: u32, handler: impl AsRef<[u8]>, entry: Vec<u8>) -> Vec<u8> {
            let mut tkhd = vec![0; 20];
            tkhd[12..16].copy_from_slice(&id.to_be_bytes());
            let mut hdlr = vec![0; 12];
            hdlr[8..12].copy_from_slice(handler.as_ref());
            let mut stsd = vec![0; 8];
            stsd[4..8].copy_from_slice(&1_u32.to_be_bytes());
            stsd.extend(entry);
            let stbl = atom(b"stbl", atom(b"stsd", stsd));
            let mdia = atom(b"mdia", [atom(b"hdlr", hdlr), atom(b"minf", stbl)].concat());
            atom(b"trak", [atom(b"tkhd", tkhd), mdia].concat())
        }

        let mut video = vec![0; 78];
        video[24..26].copy_from_slice(&640_u16.to_be_bytes());
        video[26..28].copy_from_slice(&360_u16.to_be_bytes());
        video.extend(atom(b"avcC", vec![1, 100, 0, 41]));
        let mut audio = vec![0; 18];
        audio[16..18].copy_from_slice(&2_u16.to_be_bytes());
        let bytes = atom(
            b"moov",
            [
                atom(b"mvhd", {
                    let mut mvhd = vec![0; 20];
                    mvhd[12..16].copy_from_slice(&1_000_u32.to_be_bytes());
                    mvhd[16..20].copy_from_slice(&2_000_u32.to_be_bytes());
                    mvhd
                }),
                track(1, b"vide", sample_entry(b"avc1", video)),
                track(2, b"soun", sample_entry(b"mp4a", audio)),
            ]
            .concat(),
        );

        let result = DefaultMediaInspector
            .inspect(ProbeInput {
                size: bytes.len() as u64,
                segments: vec![ProbeSegment { start: 0, bytes }],
            })
            .unwrap();
        assert_eq!(result.container(), "mp4");
        assert_eq!(result.streams().len(), 2);
        assert_eq!(result.streams()[0].codec(), Some("h264"));
        assert_eq!(result.streams()[0].profile(), Some("High"));
        assert_eq!(result.streams()[0].level(), Some(41));
        assert_eq!(result.streams()[1].codec(), Some("aac"));

        let mut hevc = vec![0; 13];
        hevc[0] = 1;
        hevc[1] = 2;
        hevc[12] = 120;
        assert_eq!(
            codec_compatibility("hevc", Some(&hevc)),
            (Some("Main 10".to_owned()), Some(120))
        );
    }

    #[test]
    fn default_inspector_finds_a_tail_moov_after_a_sparse_probe_gap() {
        fn atom(kind: impl AsRef<[u8]>, payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.extend(payload);
            bytes
        }
        fn video_track() -> Vec<u8> {
            let mut tkhd = vec![0; 20];
            tkhd[12..16].copy_from_slice(&1_u32.to_be_bytes());
            let mut hdlr = vec![0; 12];
            hdlr[8..12].copy_from_slice(b"vide");
            let mut video = vec![0; 28];
            video[24..26].copy_from_slice(&1920_u16.to_be_bytes());
            video[26..28].copy_from_slice(&1080_u16.to_be_bytes());
            let mut stsd = vec![0; 8];
            stsd[4..8].copy_from_slice(&1_u32.to_be_bytes());
            stsd.extend(atom(b"avc1", video));
            atom(
                b"trak",
                [
                    atom(b"tkhd", tkhd),
                    atom(
                        b"mdia",
                        [
                            atom(b"hdlr", hdlr),
                            atom(b"minf", atom(b"stbl", atom(b"stsd", stsd))),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            )
        }

        let head = [
            atom(b"ftyp", b"isom\0\0\0\0isom".to_vec()),
            1_000_u32.to_be_bytes().to_vec(),
            b"mdat".to_vec(),
        ]
        .concat();
        let tail = [vec![0xAA; 17], atom(b"moov", video_track())].concat();
        let tail_start = 1_024_u64;
        let result = DefaultMediaInspector
            .inspect(ProbeInput {
                size: tail_start + tail.len() as u64,
                segments: vec![
                    ProbeSegment {
                        start: 0,
                        bytes: head,
                    },
                    ProbeSegment {
                        start: tail_start,
                        bytes: tail,
                    },
                ],
            })
            .unwrap();
        assert_eq!(result.container(), "mp4");
        assert_eq!(result.streams().len(), 1);
        assert_eq!(result.streams()[0].codec(), Some("h264"));
    }

    #[test]
    fn default_inspector_requests_gap_fills_when_moov_exceeds_covered_segments() {
        fn atom(kind: impl AsRef<[u8]>, payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.extend(payload);
            bytes
        }
        fn video_track() -> Vec<u8> {
            let mut tkhd = vec![0; 20];
            tkhd[12..16].copy_from_slice(&1_u32.to_be_bytes());
            let mut hdlr = vec![0; 12];
            hdlr[8..12].copy_from_slice(b"vide");
            let mut video = vec![0; 28];
            video[24..26].copy_from_slice(&1920_u16.to_be_bytes());
            video[26..28].copy_from_slice(&1080_u16.to_be_bytes());
            let mut stsd = vec![0; 8];
            stsd[4..8].copy_from_slice(&1_u32.to_be_bytes());
            stsd.extend(atom(b"avc1", video));
            atom(
                b"trak",
                [
                    atom(b"tkhd", tkhd),
                    atom(
                        b"mdia",
                        [
                            atom(b"hdlr", hdlr),
                            atom(b"minf", atom(b"stbl", atom(b"stsd", stsd))),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            )
        }

        // A long movie's moov easily exceeds the one MiB probe budget; inflate
        // it with a large skipped `free` box so the moov magic sits inside the
        // covered tail while the box end does not.
        let moov = atom(
            b"moov",
            [video_track(), atom(b"free", vec![0_u8; 96 * 1024])].concat(),
        );
        let head = [
            atom(b"ftyp", b"isom\0\0\0\0isom".to_vec()),
            1_000_u32.to_be_bytes().to_vec(),
            b"mdat".to_vec(),
        ]
        .concat();
        let tail_start = 1_024_u64;
        let moov_offset_in_tail = 17_usize;
        let covered = 1_024_usize;
        let truncated_tail = [vec![0xAA; moov_offset_in_tail], moov[..covered].to_vec()].concat();
        let covered_end = tail_start + truncated_tail.len() as u64;
        let sparse_input = ProbeInput {
            size: tail_start + moov_offset_in_tail as u64 + moov.len() as u64,
            segments: vec![
                ProbeSegment {
                    start: 0,
                    bytes: head,
                },
                ProbeSegment {
                    start: tail_start,
                    bytes: truncated_tail,
                },
            ],
        };
        let error = DefaultMediaInspector
            .inspect(sparse_input.clone())
            .unwrap_err();
        let ProbeServiceError::Inspection(message) = &error else {
            panic!("a truncated moov must surface an inspection error, got {error}");
        };
        assert_eq!(
            probe_gap_offset(message),
            Some(covered_end),
            "a truncated moov must surface a gap error at the coverage edge"
        );

        // The service gap-retry ladder merges a window covering the remainder;
        // with that segment the same sparse input now parses completely.
        let mut filled = sparse_input;
        filled.segments.push(ProbeSegment {
            start: covered_end,
            bytes: moov[covered - moov_offset_in_tail..].to_vec(),
        });
        let result = DefaultMediaInspector.inspect(filled).unwrap();
        assert_eq!(result.container(), "mp4");
        assert_eq!(result.streams().len(), 1);
        assert_eq!(result.streams()[0].codec(), Some("h264"));
    }

    #[test]
    fn iso_box_nesting_beyond_the_depth_limit_is_rejected() {
        fn atom(kind: impl AsRef<[u8]>, payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.extend(payload);
            bytes
        }
        let mut nested = atom(b"free", Vec::new());
        for _ in 0..(MAX_BOX_DEPTH + 4) {
            nested = atom(b"moov", nested);
        }
        let bytes = [atom(b"ftyp", b"isom\0\0\0\0isom".to_vec()), nested].concat();
        let error = IsoBmffInspector
            .inspect(ProbeInput {
                size: bytes.len() as u64,
                segments: vec![ProbeSegment { start: 0, bytes }],
            })
            .unwrap_err();
        let ProbeServiceError::Inspection(message) = &error else {
            panic!("deep nesting must surface an inspection error, got {error}");
        };
        assert_eq!(message, "ISO-BMFF box nesting is too deep");
    }

    fn single_segment_input(bytes: Vec<u8>, size: Option<u64>) -> ProbeInput {
        let size = size.unwrap_or(bytes.len() as u64);
        ProbeInput {
            size,
            segments: vec![ProbeSegment { start: 0, bytes }],
        }
    }

    fn flac_streaminfo_bytes(sample_rate: u64, total_samples: u64) -> Vec<u8> {
        let mut flac = b"fLaC".to_vec();
        flac.push(0x00); // last-flag + STREAMINFO type
        flac.extend_from_slice(&34_u32.to_be_bytes()[1..]);
        flac.extend_from_slice(&[0; 10]); // block and frame size bounds
        let packed = (sample_rate << 44) | (1_u64 << 41) | (15_u64 << 36) | total_samples;
        flac.extend_from_slice(&packed.to_be_bytes());
        flac.extend_from_slice(&[0; 16]); // md5 remainder of STREAMINFO
        flac
    }

    #[test]
    fn audio_inspector_parses_flac_and_wav_containers() {
        // FLAC: STREAMINFO with 48 kHz, 2 channels, 96 000 total samples.
        let flac = flac_streaminfo_bytes(48_000, 0x1_7700);
        let result = AudioInspector
            .inspect(single_segment_input(flac, None))
            .unwrap();
        assert_eq!(result.container(), "flac");
        assert_eq!(result.streams().len(), 1);
        assert_eq!(result.streams()[0].codec(), Some("flac"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(2 * 10_000_000));

        // WAV: PCM 16-bit stereo at 48 kHz with 2 seconds of data.
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&4_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&48_000_u32.to_le_bytes());
        wav.extend_from_slice(&192_000_u32.to_le_bytes());
        wav.extend_from_slice(&4_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&384_000_u32.to_le_bytes());
        let result = AudioInspector
            .inspect(single_segment_input(wav, None))
            .unwrap();
        assert_eq!(result.container(), "wav");
        assert_eq!(result.streams()[0].codec(), Some("pcm_s16le"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(2 * 10_000_000));
    }

    #[test]
    fn audio_inspector_parses_ogg_opus_and_vorbis_containers() {
        fn ogg_tail_page(granule: u64) -> Vec<u8> {
            [
                b"OggS".as_slice(),
                &[0x00_u8, 0x04],
                &granule.to_le_bytes(),
                &0_u32.to_le_bytes(),
            ]
            .concat()
        }
        fn ogg_head_page(body: &[u8]) -> Vec<u8> {
            [
                b"OggS".as_slice(),
                &[0x00_u8, 0x02],
                &0_u64.to_le_bytes(),
                &0_u32.to_le_bytes(),
                &0_u32.to_le_bytes(),
                &0_u32.to_le_bytes(),
                &[
                    0x01_u8,
                    u8::try_from(body.len()).expect("test packet fits one lacing value"),
                ],
                body,
            ]
            .concat()
        }

        // Opus: first page carries OpusHead; a tail page supplies the granule.
        let opus_body = [
            b"OpusHead".as_slice(),
            &[1_u8, 2],
            &0_u16.to_le_bytes(),
            &48_000_u32.to_le_bytes(),
            &[0_u8; 3],
        ]
        .concat();
        let opus_head = ogg_head_page(&opus_body);
        let opus_tail = ogg_tail_page(0x1_7700);
        let opus_input = ProbeInput {
            size: (opus_head.len() + opus_tail.len()) as u64,
            segments: vec![
                ProbeSegment {
                    start: 0,
                    bytes: opus_head,
                },
                ProbeSegment {
                    start: 0,
                    bytes: opus_tail,
                },
            ],
        };
        let result = AudioInspector.inspect(opus_input).unwrap();
        assert_eq!(result.container(), "opus");
        assert_eq!(result.streams()[0].codec(), Some("opus"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(2 * 10_000_000));

        // Vorbis: first page carries the identification header.
        let vorbis_packet = [
            [0x01_u8].as_slice(),
            b"vorbis".as_slice(),
            &0_u32.to_le_bytes(),
            &[2_u8],
            &48_000_u32.to_le_bytes(),
            &0_u32.to_le_bytes(),
            &0_u32.to_le_bytes(),
            &0_u32.to_le_bytes(),
            &[0_u8, 0],
        ]
        .concat();
        let vorbis_head = ogg_head_page(&vorbis_packet);
        let vorbis_tail = ogg_tail_page(0x1_7700);
        let vorbis_input = ProbeInput {
            size: (vorbis_head.len() + vorbis_tail.len()) as u64,
            segments: vec![
                ProbeSegment {
                    start: 0,
                    bytes: vorbis_head,
                },
                ProbeSegment {
                    start: 64,
                    bytes: vorbis_tail,
                },
            ],
        };
        let result = AudioInspector.inspect(vorbis_input).unwrap();
        assert_eq!(result.container(), "ogg");
        assert_eq!(result.streams()[0].codec(), Some("vorbis"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(2 * 10_000_000));
    }

    #[test]
    fn audio_inspector_parses_mpeg_audio_containers() {
        // MP3: MPEG1 Layer III, 128 kbps joint stereo, 44.1 kHz, 2 seconds of
        // CBR data implied by the object size.
        let mut mp3 = vec![0xFF, 0xFB, 0x90, 0x40];
        mp3.resize(32_000, 0);
        let result = AudioInspector
            .inspect(single_segment_input(mp3, None))
            .unwrap();
        assert_eq!(result.container(), "mp3");
        assert_eq!(result.streams()[0].codec(), Some("mp3"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(2 * 10_000_000));

        // MP3 with a Xing frame count: 77 frames at 1 152 samples each.
        let mut xing = vec![0xFF, 0xFB, 0x90, 0x40];
        xing.resize(36, 0); // header + 32 bytes of side info
        xing.extend_from_slice(b"Xing");
        xing.extend_from_slice(&0x8000_0000_u32.to_be_bytes());
        xing.extend_from_slice(&77_u32.to_be_bytes());
        let result = AudioInspector
            .inspect(single_segment_input(xing, Some(32_000)))
            .unwrap();
        assert_eq!(result.container(), "mp3");
        let expected = (77_u128 * 1_152 * 10_000_000) / 44_100;
        assert_eq!(
            result.runtime_ticks(),
            i64::try_from(expected).ok(),
            "Xing frame counts must override the CBR estimate"
        );

        // ADTS AAC: 48 kHz, stereo, 428-byte first frame in a 40 000-byte
        // object implies 93 frames of 1 024 samples ≈ 1.984 s.
        let adts = vec![0xFF, 0xF1, 0x4C, 0x80, 0x35, 0x80];
        let result = AudioInspector
            .inspect(single_segment_input(adts, Some(40_000)))
            .unwrap();
        assert_eq!(result.container(), "aac");
        assert_eq!(result.streams()[0].codec(), Some("aac"));
        assert_eq!(result.streams()[0].channels(), Some(2));
        assert_eq!(result.runtime_ticks(), Some(19_840_000));
    }

    #[test]
    fn default_inspector_dispatches_bare_audio_before_matroska() {
        let mut flac = b"fLaC".to_vec();
        flac.push(0x00);
        flac.extend_from_slice(&34_u32.to_be_bytes()[1..]);
        flac.extend_from_slice(&[0; 10]);
        let packed = (0xAC44_u64 << 44) | (1_u64 << 41) | (15_u64 << 36) | 0xAC44;
        flac.extend_from_slice(&packed.to_be_bytes());
        flac.extend_from_slice(&[0; 16]);
        let result = DefaultMediaInspector
            .inspect(ProbeInput {
                size: flac.len() as u64,
                segments: vec![ProbeSegment {
                    start: 0,
                    bytes: flac,
                }],
            })
            .unwrap();
        assert_eq!(result.container(), "flac");
        assert_eq!(result.streams()[0].codec(), Some("flac"));
    }

    #[test]
    fn default_inspector_walks_top_level_boxes_to_a_middle_moov() {
        fn atom(kind: impl AsRef<[u8]>, payload: Vec<u8>) -> Vec<u8> {
            let size = u32::try_from(payload.len() + 8).unwrap();
            let mut bytes = size.to_be_bytes().to_vec();
            bytes.extend(kind.as_ref());
            bytes.extend(payload);
            bytes
        }
        fn video_track() -> Vec<u8> {
            let mut tkhd = vec![0; 20];
            tkhd[12..16].copy_from_slice(&1_u32.to_be_bytes());
            let mut hdlr = vec![0; 12];
            hdlr[8..12].copy_from_slice(b"vide");
            let mut video = vec![0; 28];
            video[24..26].copy_from_slice(&1920_u16.to_be_bytes());
            video[26..28].copy_from_slice(&1080_u16.to_be_bytes());
            let mut stsd = vec![0; 8];
            stsd[4..8].copy_from_slice(&1_u32.to_be_bytes());
            stsd.extend(atom(b"avc1", video));
            atom(
                b"trak",
                [
                    atom(b"tkhd", tkhd),
                    atom(
                        b"mdia",
                        [
                            atom(b"hdlr", hdlr),
                            atom(b"minf", atom(b"stbl", atom(b"stsd", stsd))),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            )
        }

        // ftyp then an mdat whose body spans the uncovered middle of the file;
        // the moov sits after it, far beyond both default probe windows.
        let moov = atom(b"moov", video_track());
        let moov_start = 2 * 1024 * 1024_u64;
        let ftyp = atom(b"ftyp", b"isom\0\0\0\0isom".to_vec());
        let mdat_body =
            usize::try_from(moov_start - ftyp.len() as u64 - 8).expect("mdat size fits usize");
        let mdat = atom(b"mdat", vec![0_u8; mdat_body]);
        let head = [ftyp, mdat[..8].to_vec()].concat();
        let sparse_input = ProbeInput {
            size: moov_start + moov.len() as u64,
            segments: vec![ProbeSegment {
                start: 0,
                bytes: head,
            }],
        };
        let error = DefaultMediaInspector
            .inspect(sparse_input.clone())
            .unwrap_err();
        let ProbeServiceError::Inspection(message) = &error else {
            panic!("a middle moov must surface an inspection error, got {error}");
        };
        assert_eq!(
            probe_gap_offset(message),
            Some(moov_start),
            "the box walk must request the uncovered moov header"
        );

        // Once the gap window covers the moov, the magic scan finds it and the
        // movie parses completely.
        let mut filled = sparse_input;
        filled.segments.push(ProbeSegment {
            start: moov_start,
            bytes: moov,
        });
        let result = DefaultMediaInspector.inspect(filled).unwrap();
        assert_eq!(result.container(), "mp4");
        assert_eq!(result.streams().len(), 1);
        assert_eq!(result.streams()[0].codec(), Some("h264"));
    }

    #[test]
    fn default_inspector_parses_the_real_smoke_mp4_fixture() {
        let bytes = include_bytes!(
            "../../server/tests/fixtures/jellyfin-smoke/Smoke Show/Season 01/Smoke Show S01E01.mp4"
        );
        let result = DefaultMediaInspector
            .inspect(ProbeInput {
                size: bytes.len() as u64,
                segments: vec![ProbeSegment {
                    start: 0,
                    bytes: bytes.to_vec(),
                }],
            })
            .unwrap();
        assert_eq!(result.container(), "mp4");
        assert_eq!(result.streams().len(), 2);
        assert_eq!(result.streams()[0].profile(), Some("High"));
        assert_eq!(result.streams()[0].level(), Some(10));
    }
}
