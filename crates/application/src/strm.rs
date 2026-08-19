use std::path::Path;

use thiserror::Error;

pub(crate) const MAX_STRM_BYTES: usize = 8 * 1024;

pub(crate) fn parse_strm(bytes: &[u8]) -> Result<&str, StrmError> {
    if bytes.len() > MAX_STRM_BYTES {
        return Err(StrmError::TooLarge);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| StrmError::InvalidUtf8)?;
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return Err(StrmError::InvalidCharacter);
    }
    let mut targets = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let target = targets.next().ok_or(StrmError::MissingTarget)?;
    if targets.next().is_some() {
        return Err(StrmError::MultipleTargets);
    }
    if looks_like_uri(target) {
        return Err(StrmError::RemoteTarget);
    }
    let path = Path::new(target);
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !supported_video_extension(&extension.to_ascii_lowercase()))
    {
        return Err(StrmError::UnsupportedTarget);
    }
    Ok(target)
}

fn looks_like_uri(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

fn supported_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mkv" | "mp4" | "m4v" | "webm" | "avi" | "mov" | "ts" | "m2ts"
    )
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StrmError {
    #[error("STRM descriptor exceeds 8 KiB")]
    TooLarge,
    #[error("STRM descriptor is not valid UTF-8")]
    InvalidUtf8,
    #[error("STRM descriptor contains a forbidden control character")]
    InvalidCharacter,
    #[error("STRM descriptor contains no target")]
    MissingTarget,
    #[error("STRM descriptor contains more than one target")]
    MultipleTargets,
    #[error("remote STRM targets are not enabled")]
    RemoteTarget,
    #[error("STRM target is not a supported video file")]
    UnsupportedTarget,
}

#[cfg(test)]
mod tests {
    use super::{MAX_STRM_BYTES, StrmError, parse_strm};

    #[test]
    fn accepts_bom_crlf_and_one_local_target() {
        assert_eq!(
            parse_strm(b"\xef\xbb\xbf  /media/show/S01E01.mkv\r\n").unwrap(),
            "/media/show/S01E01.mkv"
        );
    }

    #[test]
    fn rejects_remote_multiple_and_oversized_targets() {
        assert_eq!(
            parse_strm(b"https://example.test/video.mkv"),
            Err(StrmError::RemoteTarget)
        );
        assert_eq!(
            parse_strm(b"one.mkv\ntwo.mkv"),
            Err(StrmError::MultipleTargets)
        );
        assert_eq!(
            parse_strm(&vec![b'a'; MAX_STRM_BYTES + 1]),
            Err(StrmError::TooLarge)
        );
    }
}
