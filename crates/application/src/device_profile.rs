use serde::Deserialize;

use crate::{PlaybackSource, PlaybackStream};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DeviceProfile {
    #[serde(default, rename = "DirectPlayProfiles", alias = "directPlayProfiles")]
    direct_play_profiles: Vec<DirectPlayProfile>,
    #[serde(default, rename = "CodecProfiles", alias = "codecProfiles")]
    codec_profiles: Vec<CodecProfile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DirectPlayProfile {
    #[serde(default, rename = "Container", alias = "container")]
    container: String,
    #[serde(default, rename = "Type", alias = "type")]
    profile_type: String,
    #[serde(default, rename = "VideoCodec", alias = "videoCodec")]
    video_codec: String,
    #[serde(default, rename = "AudioCodec", alias = "audioCodec")]
    audio_codec: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CodecProfile {
    #[serde(default, rename = "Type", alias = "type")]
    profile_type: String,
    #[serde(default, rename = "Codec", alias = "codec")]
    codec: String,
    #[serde(default, rename = "Container", alias = "container")]
    container: String,
    #[serde(default, rename = "Conditions", alias = "conditions")]
    conditions: Vec<ProfileCondition>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProfileCondition {
    #[serde(default, rename = "Condition", alias = "condition")]
    operator: String,
    #[serde(default, rename = "Property", alias = "property")]
    property: String,
    #[serde(default, rename = "Value", alias = "value")]
    value: String,
    #[serde(default, rename = "IsRequired", alias = "isRequired")]
    is_required: bool,
}

impl DeviceProfile {
    #[must_use]
    pub fn supports_direct_play(&self, source: &PlaybackSource) -> bool {
        self.direct_play_profiles.iter().any(|direct| {
            direct.matches_codecs(source)
                && self
                    .codec_profiles
                    .iter()
                    .all(|codec| codec.supports(source, direct))
        })
    }

    /// Ranks container and codec compatibility independently from profile limits.
    ///
    /// This is a secondary preference for sources which cannot Direct Play due
    /// to resolution, level, or another codec-profile condition. It deliberately
    /// does not invent a server-side codec preference.
    #[must_use]
    pub fn codec_compatibility_rank(&self, source: &PlaybackSource) -> u8 {
        u8::from(
            self.direct_play_profiles
                .iter()
                .any(|direct| direct.matches_codecs(source)),
        )
    }
}

impl DirectPlayProfile {
    fn matches_codecs(&self, source: &PlaybackSource) -> bool {
        self.profile_type.eq_ignore_ascii_case("Video")
            && matches_list(&self.container, source.container())
            && matches_stream_codecs(&self.video_codec, source.streams(), "Video")
            && matches_stream_codecs(&self.audio_codec, source.streams(), "Audio")
    }
}

impl CodecProfile {
    fn supports(&self, source: &PlaybackSource, direct: &DirectPlayProfile) -> bool {
        if !self.container.is_empty() && !matches_list(&self.container, source.container()) {
            return true;
        }
        source
            .streams()
            .iter()
            .filter(|stream| self.applies_to(stream, direct))
            .all(|stream| {
                self.conditions
                    .iter()
                    .all(|condition| condition.matches(stream))
            })
    }

    fn applies_to(&self, stream: &PlaybackStream, direct: &DirectPlayProfile) -> bool {
        let type_matches = self.profile_type.eq_ignore_ascii_case(stream.stream_type())
            || (self.profile_type.eq_ignore_ascii_case("VideoAudio")
                && matches!(stream.stream_type(), "Video" | "Audio"));
        if !type_matches {
            return false;
        }
        if !self.codec.is_empty() {
            return stream
                .codec()
                .is_some_and(|codec| matches_list(&self.codec, codec));
        }
        let direct_codecs = if stream.stream_type() == "Video" {
            &direct.video_codec
        } else {
            &direct.audio_codec
        };
        direct_codecs.is_empty()
            || stream
                .codec()
                .is_some_and(|codec| matches_list(direct_codecs, codec))
    }
}

impl ProfileCondition {
    fn matches(&self, stream: &PlaybackStream) -> bool {
        let actual = match self.property.as_str() {
            "Width" => stream.width().map(|value| value.to_string()),
            "Height" => stream.height().map(|value| value.to_string()),
            "AudioChannels" => stream.channels().map(|value| value.to_string()),
            "VideoProfile" => stream.profile().map(str::to_owned),
            "VideoLevel" => stream.level().map(|value| value.to_string()),
            _ => return !self.is_required,
        };
        actual.as_deref().map_or(!self.is_required, |actual| {
            compare(actual, &self.value, &self.operator)
        })
    }
}

fn matches_stream_codecs(allowed: &str, streams: &[PlaybackStream], stream_type: &str) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let relevant = streams
        .iter()
        .filter(|stream| stream.stream_type() == stream_type)
        .collect::<Vec<_>>();
    !relevant.is_empty()
        && relevant.iter().all(|stream| {
            stream
                .codec()
                .is_some_and(|codec| matches_list(allowed, codec))
        })
}

fn matches_list(list: &str, actual: &str) -> bool {
    list.split(',')
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .any(|candidate| candidate.eq_ignore_ascii_case(actual))
}

fn compare(actual: &str, expected: &str, operator: &str) -> bool {
    match operator {
        "Equals" => actual.eq_ignore_ascii_case(expected),
        "NotEquals" => !actual.eq_ignore_ascii_case(expected),
        "EqualsAny" => expected
            .split('|')
            .any(|value| actual.eq_ignore_ascii_case(value.trim())),
        "LessThan" => compare_number(actual, expected, |left, right| left < right),
        "LessThanEqual" => compare_number(actual, expected, |left, right| left <= right),
        "GreaterThan" => compare_number(actual, expected, |left, right| left > right),
        "GreaterThanEqual" => compare_number(actual, expected, |left, right| left >= right),
        _ => false,
    }
}

fn compare_number(actual: &str, expected: &str, predicate: impl FnOnce(i64, i64) -> bool) -> bool {
    actual
        .parse::<i64>()
        .ok()
        .zip(expected.parse::<i64>().ok())
        .is_some_and(|(actual, expected)| predicate(actual, expected))
}
