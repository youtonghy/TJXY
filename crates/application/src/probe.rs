use std::{
    io::{self, Read, Seek, SeekFrom},
    panic::AssertUnwindSafe,
    sync::Arc,
};

use futures_util::StreamExt;
use matroska::{Settings, Tracktype};
use thiserror::Error;
use tjxy_db::{
    ClaimedWorkJob, ProbeCandidate, ProbeRepository, ProbeRepositoryError, ProbeResult,
    ProbedStream, StorageSyncRepositoryError,
};
use tjxy_storage::{BackendError, ByteRange, StorageBackend, StorageObject, StorageObjectId};
use uuid::Uuid;

use crate::{
    StorageBackendRegistry, StorageChangeProjectorError,
    storage_read::{self, StorageReadError},
};

const RANGE_BUDGET: u64 = 1024 * 1024;

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
        } else {
            MatroskaInspector.inspect(input)
        }
    }
}

struct IsoBmffInspector;

impl MediaInspector for IsoBmffInspector {
    fn inspect(&self, input: ProbeInput) -> Result<ProbeResult, ProbeServiceError> {
        let mut movie = IsoBmffMovie::default();
        for segment in input.segments {
            let Some((start, end)) = find_moov_range(&segment) else {
                continue;
            };
            let mut reader = SparseProbeReader {
                size: segment.start + segment.bytes.len() as u64,
                segments: vec![segment],
                position: start,
            };
            parse_iso_boxes(&mut reader, end, &mut movie, None)?;
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

fn find_moov_range(segment: &ProbeSegment) -> Option<(u64, u64)> {
    for offset in 0..=segment.bytes.len().saturating_sub(8) {
        if segment.bytes.get(offset + 4..offset + 8) != Some(b"moov") {
            continue;
        }
        let size = u64::from(u32::from_be_bytes(
            segment.bytes.get(offset..offset + 4)?.try_into().ok()?,
        ));
        if size < 8 {
            continue;
        }
        let start = segment.start.checked_add(offset as u64)?;
        let end = start.checked_add(size)?;
        let segment_end = segment.start.checked_add(segment.bytes.len() as u64)?;
        if end <= segment_end {
            return Some((start, end));
        }
    }
    None
}

fn parse_iso_boxes(
    reader: &mut SparseProbeReader,
    end: u64,
    movie: &mut IsoBmffMovie,
    current_track: Option<usize>,
) -> Result<(), ProbeServiceError> {
    while reader.position < end {
        let header = read_iso_box(reader, end)?;
        if header.kind == *b"moov"
            || header.kind == *b"mdia"
            || header.kind == *b"minf"
            || header.kind == *b"stbl"
        {
            parse_iso_boxes(reader, header.end, movie, current_track)?;
        } else if header.kind == *b"trak" {
            movie.tracks.push(IsoBmffTrack::default());
            parse_iso_boxes(reader, header.end, movie, Some(movie.tracks.len() - 1))?;
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
        .map_err(|_| ProbeServiceError::Inspection("incomplete ISO-BMFF box header".into()))?;
    let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().unwrap()));
    let mut header_size = 8_u64;
    if size == 1 {
        let mut extended = [0_u8; 8];
        reader.read_exact(&mut extended).map_err(|_| {
            ProbeServiceError::Inspection("incomplete ISO-BMFF extended box header".into())
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
        .map_err(|_| ProbeServiceError::Inspection("incomplete ISO-BMFF box body".into()))?;
    reader.position = header.end;
    Ok(body)
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
    pub async fn execute(&self, claimed: &ClaimedWorkJob) -> Result<i64, ProbeServiceError> {
        let repository = ProbeRepository::new(&self.database);
        let candidate = repository
            .candidate(claimed)
            .await?
            .ok_or(ProbeServiceError::CandidateUnavailable)?;
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
        let input = read_probe_input(
            &self.database,
            backend.as_ref(),
            candidate.storage_object_id(),
            &object_id,
            candidate.size(),
            &repository,
            claimed,
            &candidate,
        )
        .await?;
        let result = match self.inspector.inspect(input) {
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
        repository
            .commit_success(claimed, &candidate, &result)
            .await
            .map_err(Into::into)
    }
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
}

#[allow(clippy::too_many_arguments)] // Each backend read is fenced by the durable claim and exact SQL candidate.
async fn read_probe_input(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: tjxy_common::StorageObjectRecordId,
    object_id: &StorageObjectId,
    size: u64,
    repository: &ProbeRepository<'_>,
    claimed: &ClaimedWorkJob,
    candidate: &ProbeCandidate,
) -> Result<ProbeInput, ProbeServiceError> {
    let segments = if size <= RANGE_BUDGET * 2 {
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
                    RANGE_BUDGET,
                    repository,
                    claimed,
                    candidate,
                )
                .await?,
            },
            ProbeSegment {
                start: size - RANGE_BUDGET,
                bytes: read_exact_range(
                    database,
                    backend,
                    record_id,
                    object_id,
                    size - RANGE_BUDGET,
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

#[allow(clippy::too_many_arguments)] // Range reads repeat the same claim, candidate, and object authorization fence.
async fn read_exact_range(
    database: &sea_orm::DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: tjxy_common::StorageObjectRecordId,
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
    let mut stream = storage_read::open_range(database, backend, record_id, object_id, range)
        .await
        .map_err(probe_storage_read_error)?;
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
        "V_VP9" => "vp9".to_owned(),
        "V_AV1" => "av1".to_owned(),
        "A_AAC" => "aac".to_owned(),
        "A_OPUS" => "opus".to_owned(),
        "A_VORBIS" => "vorbis".to_owned(),
        "S_TEXT/UTF8" => "subrip".to_owned(),
        "S_TEXT/WEBVTT" => "webvtt".to_owned(),
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Probe byte budget gap"))?;
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
        assert_eq!(
            reader.read(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn codec_ids_are_normalized_for_jellyfin_fields() {
        assert_eq!(normalize_codec("V_MPEG4/ISO/AVC"), "h264");
        assert_eq!(normalize_codec("A_OPUS"), "opus");
        assert_eq!(normalize_codec("X CUSTOM/CODEC"), "x_custom_codec");
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
