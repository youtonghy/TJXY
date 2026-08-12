use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_NAME_CHARS: usize = 512;

/// Incremented whenever persisted Naming evidence must be rebuilt.
pub const MEDIA_NAME_PARSER_VERSION: i32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NumberRange {
    start: u32,
    end: u32,
}

impl NumberRange {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Option<Self> {
        if end < start {
            return None;
        }
        Some(Self { start, end })
    }

    #[must_use]
    pub const fn single(value: u32) -> Option<Self> {
        Self::new(value, value)
    }

    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaNameWarning {
    AmbiguousYear,
    DiscardedLeadingTag,
    MissingTitle,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParsedMediaName {
    title: Option<String>,
    year: Option<i32>,
    season: Option<NumberRange>,
    episode: Option<NumberRange>,
    part: Option<u32>,
    resolution: Option<String>,
    sources: Vec<String>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
    bit_depth: Option<u32>,
    frame_rate: Option<u32>,
    release_group: Option<String>,
    warnings: Vec<MediaNameWarning>,
}

impl ParsedMediaName {
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub const fn year(&self) -> Option<i32> {
        self.year
    }

    #[must_use]
    pub const fn season(&self) -> Option<NumberRange> {
        self.season
    }

    #[must_use]
    pub const fn episode(&self) -> Option<NumberRange> {
        self.episode
    }

    #[must_use]
    pub const fn part(&self) -> Option<u32> {
        self.part
    }

    #[must_use]
    pub fn resolution(&self) -> Option<&str> {
        self.resolution.as_deref()
    }

    #[must_use]
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    #[must_use]
    pub fn video_codec(&self) -> Option<&str> {
        self.video_codec.as_deref()
    }

    #[must_use]
    pub fn audio_codec(&self) -> Option<&str> {
        self.audio_codec.as_deref()
    }

    #[must_use]
    pub const fn bit_depth(&self) -> Option<u32> {
        self.bit_depth
    }

    #[must_use]
    pub const fn frame_rate(&self) -> Option<u32> {
        self.frame_rate
    }

    #[must_use]
    pub fn release_group(&self) -> Option<&str> {
        self.release_group.as_deref()
    }

    #[must_use]
    pub fn warnings(&self) -> &[MediaNameWarning] {
        &self.warnings
    }

    /// Fills missing identity fields from a more distant path component.
    pub fn merge_path_context(&mut self, parent: &Self) {
        if self.title.is_none() {
            self.title.clone_from(&parent.title);
        }
        if self.year.is_none() {
            self.year = parent.year;
        }
        if self.season.is_none() {
            self.season = parent.season;
        }
        if self.episode.is_none() {
            self.episode = parent.episode;
        }
    }

    #[must_use]
    pub fn naming_hint(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(resolution) = &self.resolution {
            parts.push(resolution.clone());
        }
        parts.extend(self.sources.iter().cloned());
        if let Some(codec) = &self.video_codec {
            parts.push(codec.clone());
        }
        if let Some(bit_depth) = self.bit_depth {
            parts.push(format!("{bit_depth}bit"));
        }
        if let Some(frame_rate) = self.frame_rate {
            parts.push(format!("{frame_rate}FPS"));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MediaNameError {
    #[error("media name is empty")]
    Empty,
    #[error("media name exceeds {MAX_NAME_CHARS} characters")]
    TooLong,
    #[error("media name contains control characters")]
    ControlCharacter,
}

/// Parses naming evidence without consulting storage, configuration, or remote providers.
///
/// # Errors
///
/// Returns [`MediaNameError`] for empty, unbounded, or control-containing input.
#[allow(clippy::too_many_lines)] // Keeps token classification in one deterministic precedence order.
pub fn parse_media_name(value: &str) -> Result<ParsedMediaName, MediaNameError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MediaNameError::Empty);
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(MediaNameError::TooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(MediaNameError::ControlCharacter);
    }

    let stem = strip_known_extension(trimmed);
    let (candidate, discarded_tag) = strip_leading_tag(stem);
    let tokens = tokenize(candidate);
    let year_indexes = plausible_year_indexes(&tokens);
    let selected_year = year_indexes.iter().rev().copied().find(|index| {
        year_has_explicit_delimiters(candidate, &tokens[*index])
            || (*index > 0
                && tokens
                    .iter()
                    .skip(index + 1)
                    .any(|token| is_boundary_token(token)))
    });
    let first_boundary = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| is_boundary_token(token).then_some(index));
    let mut parsed = ParsedMediaName {
        title: None,
        year: selected_year.and_then(|index| parse_year(&tokens[index])),
        season: None,
        episode: None,
        part: None,
        resolution: None,
        sources: Vec::new(),
        video_codec: None,
        audio_codec: None,
        bit_depth: None,
        frame_rate: None,
        release_group: None,
        warnings: Vec::new(),
    };
    if discarded_tag {
        parsed.warnings.push(MediaNameWarning::DiscardedLeadingTag);
    }
    if year_indexes.len() > 1 {
        parsed.warnings.push(MediaNameWarning::AmbiguousYear);
    }

    for (index, token) in tokens.iter().enumerate() {
        if Some(index) == selected_year {
            continue;
        }
        if let Some((season, episode)) = parse_season_episode(token) {
            parsed.season = parsed.season.or(season);
            parsed.episode = parsed.episode.or(episode);
            continue;
        }
        if parsed.season.is_none()
            && token.eq_ignore_ascii_case("season")
            && let Some(value) = tokens
                .get(index + 1)
                .and_then(|next| parse_nonnegative(next))
        {
            parsed.season = NumberRange::single(value);
            continue;
        }
        if parsed.season.is_none()
            && let Some(value) = parse_chinese_ordinal(token, '季')
        {
            parsed.season = NumberRange::single(value);
            continue;
        }
        if parsed.episode.is_none()
            && let Some(value) = parse_chinese_ordinal(token, '集')
        {
            parsed.episode = NumberRange::single(value);
            continue;
        }
        if parsed.part.is_none() {
            parsed.part = parse_prefixed_number(token, &["part", "pt", "disc", "disk", "cd"]);
        }
        if parsed.resolution.is_none() {
            parsed.resolution = normalize_resolution(token).map(str::to_owned);
        }
        if let Some(source) = normalize_source(token)
            && !parsed.sources.iter().any(|existing| existing == source)
        {
            parsed.sources.push(source.to_owned());
        }
        if let Some(source) = tokens
            .get(index + 1)
            .and_then(|next| normalize_source_pair(token, next))
            && !parsed.sources.iter().any(|existing| existing == source)
        {
            parsed.sources.push(source.to_owned());
        }
        if parsed.video_codec.is_none() {
            parsed.video_codec = normalize_video_codec(token)
                .or_else(|| {
                    tokens
                        .get(index + 1)
                        .and_then(|next| normalize_video_codec_pair(token, next))
                })
                .or_else(|| {
                    token
                        .split_once('-')
                        .and_then(|(prefix, _)| normalize_video_codec(prefix))
                })
                .map(str::to_owned);
        }
        if parsed.audio_codec.is_none() {
            parsed.audio_codec = normalize_audio_codec(token).map(str::to_owned);
        }
        if parsed.bit_depth.is_none() {
            parsed.bit_depth = parse_suffix_number(token, "bit");
        }
        if parsed.frame_rate.is_none() {
            parsed.frame_rate = parse_suffix_number(token, "fps");
        }
    }

    if tokens.len() == 1 && parsed.episode.is_none() {
        parsed.episode = parse_positive(&tokens[0])
            .filter(|episode| *episode <= 999)
            .and_then(NumberRange::single);
    }

    let title_end = [first_boundary, selected_year]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(tokens.len());
    let title_tokens = tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            *index < title_end
                && Some(*index) != selected_year
                && parse_year(token).is_none_or(|_| Some(*index) != selected_year)
        })
        .map(|(_, token)| token.as_str())
        .collect::<Vec<_>>();
    let title = normalize_title(&title_tokens.join(" "));
    if !title.is_empty()
        && (parse_positive(&title).is_none() || (tokens.len() == 1 && parsed.episode.is_none()))
    {
        parsed.title = Some(title);
    } else {
        parsed.warnings.push(MediaNameWarning::MissingTitle);
    }
    parsed.release_group = release_group(candidate, &parsed);
    Ok(parsed)
}

fn strip_known_extension(value: &str) -> &str {
    let path = Path::new(value);
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return value;
    };
    if matches!(
        extension.to_ascii_lowercase().as_str(),
        "mkv" | "mp4" | "m4v" | "webm" | "avi" | "mov" | "ts" | "m2ts"
    ) {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(value)
    } else {
        value
    }
}

fn strip_leading_tag(value: &str) -> (&str, bool) {
    let Some((close, offset)) = (match value.chars().next() {
        Some('[') => Some((']', 1)),
        Some('【') => Some(('】', '【'.len_utf8())),
        _ => None,
    }) else {
        return (value, false);
    };
    let Some(end) = value.find(close) else {
        return (value, false);
    };
    let content = &value[offset..end];
    let remainder = value[end + close.len_utf8()..].trim();
    if remainder.is_empty() || !known_leading_tag(content) {
        return (value, false);
    }
    (remainder, true)
}

fn known_leading_tag(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '.')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        compact.as_str(),
        "yts" | "ytsmx" | "rarbg" | "中文字幕" | "中英字幕" | "chs" | "cht" | "group"
    )
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '.' | '(' | ')' | '[' | ']' | '【' | '】' | '/' | '\\' | '_' | '「' | '」'
                )
        })
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn plausible_year_indexes(tokens: &[String]) -> Vec<usize> {
    let boundary = tokens
        .iter()
        .position(|token| is_strong_technical_token(token))
        .unwrap_or(tokens.len());
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            (index <= boundary && parse_year(token).is_some()).then_some(index)
        })
        .collect()
}

fn year_has_explicit_delimiters(candidate: &str, year: &str) -> bool {
    [
        format!("({year})"),
        format!("[{year}]"),
        format!("【{year}】"),
        format!("「{year}」"),
    ]
    .iter()
    .any(|pattern| candidate.contains(pattern))
}

fn parse_year(token: &str) -> Option<i32> {
    if token.len() != 4 || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = token.parse::<i32>().ok()?;
    (1888..=2199).contains(&year).then_some(year)
}

fn is_boundary_token(token: &str) -> bool {
    parse_season_episode(token).is_some()
        || token.eq_ignore_ascii_case("season")
        || parse_chinese_ordinal(token, '季').is_some()
        || parse_chinese_ordinal(token, '集').is_some()
        || is_strong_technical_token(token)
}

fn is_strong_technical_token(token: &str) -> bool {
    normalize_resolution(token).is_some()
        || normalize_source(token).is_some()
        || normalize_video_codec(token).is_some()
        || normalize_audio_codec(token).is_some()
        || parse_suffix_number(token, "bit").is_some()
        || parse_suffix_number(token, "fps").is_some()
}

fn parse_season_episode(token: &str) -> Option<(Option<NumberRange>, Option<NumberRange>)> {
    let lower = token.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if bytes.first() == Some(&b's') {
        if let Some(e_position) = lower.find('e') {
            let season = parse_nonnegative(&lower[1..e_position]).and_then(NumberRange::single);
            let episode = parse_number_range(&lower[e_position + 1..], Some('e'));
            if season.is_some() && episode.is_some() {
                return Some((season, episode));
            }
        }
        let digits = &lower[1..];
        if digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit()) {
            let season = parse_positive(&digits[..1]).and_then(NumberRange::single);
            let episode = parse_positive(&digits[1..]).and_then(NumberRange::single);
            return Some((season, episode));
        }
        if let Some(season) = parse_nonnegative(digits).and_then(NumberRange::single) {
            return Some((Some(season), None));
        }
    }
    for prefix in ["episode", "ep", "e"] {
        if let Some(value) = lower.strip_prefix(prefix)
            && let Some(episode) = parse_number_range(value, Some('e'))
        {
            return Some((None, Some(episode)));
        }
    }
    None
}

fn parse_number_range(value: &str, repeated_prefix: Option<char>) -> Option<NumberRange> {
    let (start, end) = value.split_once('-').map_or((value, None), |(start, end)| {
        (
            start,
            Some(end.trim_start_matches(repeated_prefix.unwrap_or_default())),
        )
    });
    let start = parse_positive(start)?;
    let end = end.map_or(Some(start), parse_positive)?;
    NumberRange::new(start, end)
}

fn parse_positive(value: &str) -> Option<u32> {
    parse_nonnegative(value).filter(|value| *value > 0)
}

fn parse_nonnegative(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<u32>().ok())
        .flatten()
}

fn parse_prefixed_number(token: &str, prefixes: &[&str]) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    prefixes
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix).and_then(parse_positive))
}

fn parse_suffix_number(token: &str, suffix: &str) -> Option<u32> {
    token
        .to_ascii_lowercase()
        .strip_suffix(suffix)
        .and_then(parse_positive)
}

fn parse_chinese_ordinal(token: &str, suffix: char) -> Option<u32> {
    let value = token.strip_prefix('第')?.strip_suffix(suffix)?;
    parse_positive(value).or_else(|| parse_chinese_number(value))
}

fn parse_chinese_number(value: &str) -> Option<u32> {
    let digit = |character| match character {
        '零' | '〇' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    };
    if let Some((left, right)) = value.split_once('十') {
        let tens = if left.is_empty() {
            1
        } else {
            left.chars().next().and_then(digit)?
        };
        let ones = if right.is_empty() {
            0
        } else {
            right.chars().next().and_then(digit)?
        };
        return Some(tens * 10 + ones).filter(|number| *number > 0);
    }
    let mut number = 0_u32;
    for character in value.chars() {
        number = number.checked_mul(10)?.checked_add(digit(character)?)?;
    }
    (number > 0).then_some(number)
}

fn normalize_resolution(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "4k" | "uhd" | "2160" | "2160p" => Some("2160p"),
        "1080" | "1080p" | "1080i" => Some("1080p"),
        "720" | "720p" => Some("720p"),
        "480" | "480p" | "576" | "576p" => Some("SD"),
        _ => None,
    }
}

fn normalize_source(token: &str) -> Option<&'static str> {
    let compact = token
        .chars()
        .filter(|character| !matches!(*character, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match compact.as_str() {
        "webdl" => Some("WEB-DL"),
        "webrip" => Some("WEBRip"),
        "bluray" | "bdrip" => Some("BluRay"),
        "hdtv" => Some("HDTV"),
        "remux" => Some("REMUX"),
        "dvd" | "dvdrip" => Some("DVD"),
        _ => None,
    }
}

fn normalize_source_pair(first: &str, second: &str) -> Option<&'static str> {
    match (
        first.to_ascii_lowercase().as_str(),
        second.to_ascii_lowercase().as_str(),
    ) {
        ("web", "dl") => Some("WEB-DL"),
        ("blu" | "blue", "ray") => Some("BluRay"),
        _ => None,
    }
}

fn normalize_video_codec(token: &str) -> Option<&'static str> {
    let compact = token
        .chars()
        .filter(|character| !matches!(*character, '.' | '-'))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match compact.as_str() {
        "h264" | "x264" | "avc" => Some("H264"),
        "h265" | "x265" | "hevc" => Some("H265"),
        "av1" => Some("AV1"),
        "vp9" => Some("VP9"),
        _ => None,
    }
}

fn normalize_video_codec_pair(first: &str, second: &str) -> Option<&'static str> {
    match (
        first.to_ascii_lowercase().as_str(),
        second.to_ascii_lowercase().as_str(),
    ) {
        ("h", "264") => Some("H264"),
        ("h", "265") => Some("H265"),
        _ => None,
    }
}

fn normalize_audio_codec(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "aac" => Some("AAC"),
        "ac3" | "dd" => Some("AC3"),
        "eac3" | "ddp" | "dd+" => Some("EAC3"),
        "dts" => Some("DTS"),
        "truehd" => Some("TrueHD"),
        "flac" => Some("FLAC"),
        _ => None,
    }
}

fn normalize_title(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character: char| matches!(character, '-' | '–' | '—'))
        .trim()
        .to_owned()
}

fn release_group(candidate: &str, parsed: &ParsedMediaName) -> Option<String> {
    let last_token = candidate
        .rsplit(|character: char| character.is_whitespace() || matches!(character, '.' | '_'))
        .find(|token| !token.is_empty())?;
    if normalize_source(last_token).is_some() {
        return None;
    }
    let (prefix, suffix) = last_token.rsplit_once('-')?;
    let suffix = suffix.trim();
    if !is_strong_technical_token(prefix)
        || suffix.is_empty()
        || suffix.chars().count() > 32
        || suffix.chars().any(char::is_whitespace)
        || parsed.title.as_deref() == Some(suffix)
    {
        return None;
    }
    Some(suffix.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{MediaNameWarning, NumberRange, parse_media_name};

    #[test]
    fn parses_movie_release_names_without_losing_title_punctuation() {
        let parsed =
            parse_media_name("The.Shawshank.Redemption.1994.1080p.BluRay.x264-GROUP.mkv").unwrap();
        assert_eq!(parsed.title(), Some("The Shawshank Redemption"));
        assert_eq!(parsed.year(), Some(1994));
        assert_eq!(parsed.resolution(), Some("1080p"));
        assert_eq!(parsed.sources(), ["BluRay"]);
        assert_eq!(parsed.video_codec(), Some("H264"));
        assert_eq!(parsed.release_group(), Some("GROUP"));

        let hyphenated = parse_media_name("Spider-Man.No.Way.Home.2021.WEB-DL.mkv").unwrap();
        assert_eq!(hyphenated.title(), Some("Spider-Man No Way Home"));
    }

    #[test]
    fn keeps_title_numbers_and_uses_the_last_plausible_year() {
        let parsed = parse_media_name("Wonder.Woman.1984.2020.2160p.WEB-DL.mkv").unwrap();
        assert_eq!(parsed.title(), Some("Wonder Woman 1984"));
        assert_eq!(parsed.year(), Some(2020));
        assert!(parsed.warnings().contains(&MediaNameWarning::AmbiguousYear));

        let parsed = parse_media_name("玩具总动员5(2026)").unwrap();
        assert_eq!(parsed.title(), Some("玩具总动员5"));
        assert_eq!(parsed.year(), Some(2026));

        let numeric_title = parse_media_name("1917.mkv").unwrap();
        assert_eq!(numeric_title.title(), Some("1917"));
        assert_eq!(numeric_title.year(), None);

        let ambiguous_suffix = parse_media_name("Blade.Runner.2049.mkv").unwrap();
        assert_eq!(ambiguous_suffix.title(), Some("Blade Runner 2049"));
        assert_eq!(ambiguous_suffix.year(), None);
    }

    #[test]
    fn parses_seasons_episodes_ranges_and_chinese_ordinals() {
        let parsed = parse_media_name("Show.S01E18-E19.1080p.mkv").unwrap();
        assert_eq!(parsed.season(), NumberRange::single(1));
        assert_eq!(parsed.episode(), NumberRange::new(18, 19));

        let compact = parse_media_name("S101.mp4").unwrap();
        assert_eq!(compact.season(), NumberRange::single(1));
        assert_eq!(compact.episode(), NumberRange::single(1));
        assert_eq!(compact.title(), None);

        let special = parse_media_name("Show.S00E01.mkv").unwrap();
        assert_eq!(special.season(), NumberRange::single(0));
        assert_eq!(special.episode(), NumberRange::single(1));

        let chinese = parse_media_name("第十二集.mkv").unwrap();
        assert_eq!(chinese.episode(), NumberRange::single(12));
    }

    #[test]
    fn merges_near_path_context_only_into_missing_fields() {
        let mut file = parse_media_name("5.mkv").unwrap();
        let season = parse_media_name("第二季").unwrap();
        let series = parse_media_name("西部世界 (2016)").unwrap();
        file.merge_path_context(&season);
        file.merge_path_context(&series);
        assert_eq!(file.title(), Some("西部世界"));
        assert_eq!(file.year(), Some(2016));
        assert_eq!(file.season(), NumberRange::single(2));
        assert_eq!(file.episode(), NumberRange::single(5));
    }

    #[test]
    fn strips_only_known_leading_tags_and_deduplicates_sources() {
        let parsed = parse_media_name(
            "[YTS.MX] Movie.Name.2024.UHD.Blu-ray.Remux.BluRay.REMUX.H265.10bit.60FPS",
        )
        .unwrap();
        assert_eq!(parsed.title(), Some("Movie Name"));
        assert_eq!(parsed.resolution(), Some("2160p"));
        assert_eq!(parsed.sources(), ["BluRay", "REMUX"]);
        assert_eq!(parsed.video_codec(), Some("H265"));
        assert_eq!(parsed.bit_depth(), Some(10));
        assert_eq!(parsed.frame_rate(), Some(60));
        assert!(
            parsed
                .warnings()
                .contains(&MediaNameWarning::DiscardedLeadingTag)
        );

        let unknown = parse_media_name("[Director Cut] Movie Name (2024)").unwrap();
        assert_eq!(unknown.title(), Some("Director Cut Movie Name"));

        let dotted = parse_media_name("Movie.2024.1080p.WEB.DL.Blu.Ray.H.264.mkv").unwrap();
        assert_eq!(dotted.sources(), ["WEB-DL", "BluRay"]);
        assert_eq!(dotted.video_codec(), Some("H264"));
        assert_eq!(dotted.release_group(), None);

        let web_dl = parse_media_name("Movie.2024.WEB-DL.mkv").unwrap();
        assert_eq!(web_dl.release_group(), None);
    }
}
