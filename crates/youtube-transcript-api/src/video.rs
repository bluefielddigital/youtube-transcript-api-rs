use serde::{Deserialize, Serialize};

use crate::innertube;

/// Details of a video, from [`TranscriptList::video_details`].
///
/// They come from the responses that listing transcripts downloads anyway,
/// so they cost no extra request. Every field YouTube may leave out is
/// optional.
///
/// [`TranscriptList::video_details`]: crate::TranscriptList::video_details
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VideoDetails {
    /// The video ID.
    pub video_id: String,
    /// The title.
    pub title: Option<String>,
    /// The channel's name.
    pub author: Option<String>,
    /// The channel ID.
    pub channel_id: Option<String>,
    /// The channel's URL.
    pub channel_url: Option<String>,
    /// The length in seconds.
    pub length_seconds: Option<u64>,
    /// The view count.
    pub view_count: Option<u64>,
    /// The like count.
    pub like_count: Option<u64>,
    /// The description.
    pub description: Option<String>,
    /// The tags.
    pub keywords: Vec<String>,
    /// The category, for example `Music`.
    pub category: Option<String>,
    /// The publish date, as YouTube gives it (ISO 8601, for example
    /// `2009-10-24T23:57:33-07:00`).
    pub publish_date: Option<String>,
    /// The upload date, in the same format.
    pub upload_date: Option<String>,
    /// Thumbnails, smallest first.
    pub thumbnails: Vec<Thumbnail>,
    /// The countries the video is available in (ISO 3166 codes).
    pub available_countries: Vec<String>,
    /// Whether it's a live stream, now or in the past.
    pub is_live_content: bool,
    /// Whether it's private.
    pub is_private: bool,
    /// Whether only people with the link can find it.
    pub is_unlisted: Option<bool>,
    /// Whether YouTube considers it family safe.
    pub is_family_safe: Option<bool>,
}

/// A thumbnail image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Thumbnail {
    /// The image URL.
    pub url: String,
    /// The width in pixels.
    pub width: Option<u32>,
    /// The height in pixels.
    pub height: Option<u32>,
}

impl VideoDetails {
    /// Merges the two sources; `None` if YouTube sent neither.
    pub(crate) fn from_responses(
        video_id: &str,
        details: Option<innertube::VideoDetails>,
        microformat: Option<innertube::Microformat>,
    ) -> Option<Self> {
        if details.is_none() && microformat.is_none() {
            return None;
        }
        let d = details.unwrap_or_default();
        let m = microformat.unwrap_or_default();
        Some(Self {
            video_id: d.video_id.unwrap_or_else(|| video_id.to_owned()),
            title: d.title.or_else(|| m.title.map(|t| t.plain())),
            author: d.author.or(m.owner_channel_name),
            channel_id: d.channel_id.or(m.external_channel_id),
            channel_url: m.owner_profile_url,
            length_seconds: d.length_seconds,
            view_count: d.view_count,
            like_count: m.like_count,
            description: d
                .short_description
                .or_else(|| m.description.map(|t| t.plain())),
            keywords: d.keywords.unwrap_or_default(),
            category: m.category,
            publish_date: m.publish_date,
            upload_date: m.upload_date,
            thumbnails: d
                .thumbnail
                .and_then(|t| t.thumbnails)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| {
                    Some(Thumbnail {
                        url: t.url?,
                        width: t.width,
                        height: t.height,
                    })
                })
                .collect(),
            available_countries: m.available_countries.unwrap_or_default(),
            is_live_content: d.is_live_content,
            is_private: d.is_private,
            is_unlisted: m.is_unlisted,
            is_family_safe: m.is_family_safe,
        })
    }
}
