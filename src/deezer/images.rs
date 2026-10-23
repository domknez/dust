//! Pictures on Deezer's image CDN. Deezer names a picture by its family there
//! (`cover`, `artist`, `playlist`, `talk` for podcasts, ...) and an md5; dust's
//! [`Art`] for it is the path `<family>/<md5>`.

use crate::model::Art;

/// The picture `md5` of a `family` on the CDN; None when either is missing.
pub(super) fn art(family: &str, md5: &str) -> Option<Art> {
    if family.is_empty() || md5.is_empty() {
        return None;
    }
    Art::new(format!("{family}/{md5}"))
}

/// The URL of a picture, `size` px square.
pub fn art_url(art: &Art, size: u32) -> String {
    format!("https://cdn-images.dzcdn.net/images/{}/{size}x{size}-000000-80-0-0.jpg", art.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_cdn_paths() {
        let cover = art("cover", "2e018122cb56986277102d2041a592c8").unwrap();
        assert_eq!(
            art_url(&cover, 120),
            "https://cdn-images.dzcdn.net/images/cover/2e018122cb56986277102d2041a592c8/120x120-000000-80-0-0.jpg"
        );
        assert_eq!(art("cover", ""), None);
        assert_eq!(art("", "abc"), None);
    }
}
