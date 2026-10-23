//! Artwork URLs.

/// Square image from Deezer's CDN, e.g. `image_url("cover", md5, 120)`. A
/// music server's artwork (`md5` = `sub:<cover id>`) comes from that server.
pub fn image_url(kind: &str, md5: &str, size: u32) -> String {
    if let Some(id) = md5.strip_prefix(crate::subsonic::COVER_PREFIX) {
        return crate::subsonic::cover_url(id, size);
    }
    format!("https://cdn-images.dzcdn.net/images/{kind}/{md5}/{size}x{size}-000000-80-0-0.jpg")
}
