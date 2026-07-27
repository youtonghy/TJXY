use std::{fmt, str::FromStr};

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageType {
    Art,
    Backdrop,
    Banner,
    Box,
    BoxRear,
    Chapter,
    Disc,
    Logo,
    Menu,
    Primary,
    Profile,
    Screenshot,
    Thumb,
}

impl ImageType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Art => "Art",
            Self::Backdrop => "Backdrop",
            Self::Banner => "Banner",
            Self::Box => "Box",
            Self::BoxRear => "BoxRear",
            Self::Chapter => "Chapter",
            Self::Disc => "Disc",
            Self::Logo => "Logo",
            Self::Menu => "Menu",
            Self::Primary => "Primary",
            Self::Profile => "Profile",
            Self::Screenshot => "Screenshot",
            Self::Thumb => "Thumb",
        }
    }
}

impl fmt::Display for ImageType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ImageType {
    type Err = InvalidImageType;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "Art" => Ok(Self::Art),
            "Backdrop" => Ok(Self::Backdrop),
            "Banner" => Ok(Self::Banner),
            "Box" => Ok(Self::Box),
            "BoxRear" => Ok(Self::BoxRear),
            "Chapter" => Ok(Self::Chapter),
            "Disc" => Ok(Self::Disc),
            "Logo" => Ok(Self::Logo),
            "Menu" => Ok(Self::Menu),
            "Primary" => Ok(Self::Primary),
            "Profile" => Ok(Self::Profile),
            "Screenshot" => Ok(Self::Screenshot),
            "Thumb" => Ok(Self::Thumb),
            _ => Err(InvalidImageType),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("image type is not part of the pinned Jellyfin contract")]
pub struct InvalidImageType;
