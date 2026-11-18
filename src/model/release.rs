//! What an artist's releases are, and how an artist page groups them: the same
//! for every source, which each say only what kind a release is and when it came
//! out.

use super::{Item, Section};

/// What kind of release an album is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseKind {
    Album,
    Single,
    Ep,
    Live,
    Compilation,
}

impl ReleaseKind {
    /// As shown under a release: "Album", "EP".
    pub fn label(self) -> &'static str {
        match self {
            ReleaseKind::Album => "Album",
            ReleaseKind::Single => "Single",
            ReleaseKind::Ep => "EP",
            ReleaseKind::Live => "Live",
            ReleaseKind::Compilation => "Compilation",
        }
    }

    pub fn group(self) -> ReleaseGroup {
        match self {
            ReleaseKind::Album => ReleaseGroup::Albums,
            ReleaseKind::Single | ReleaseKind::Ep => ReleaseGroup::SinglesAndEps,
            ReleaseKind::Live | ReleaseKind::Compilation => ReleaseGroup::LiveAndCompilations,
        }
    }
}

/// A section of an artist's discography, in the order an artist page shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReleaseGroup {
    Albums,
    SinglesAndEps,
    LiveAndCompilations,
}

impl ReleaseGroup {
    pub const ALL: [ReleaseGroup; 3] = [ReleaseGroup::Albums, ReleaseGroup::SinglesAndEps, ReleaseGroup::LiveAndCompilations];

    pub fn title(self) -> &'static str {
        match self {
            ReleaseGroup::Albums => "Albums",
            ReleaseGroup::SinglesAndEps => "Singles & EPs",
            ReleaseGroup::LiveAndCompilations => "Live & compilations",
        }
    }
}

/// What a discography card knows of its release.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    /// "2013"; None when the source doesn't say.
    pub year: Option<String>,
    pub kind: ReleaseKind,
}

impl ReleaseInfo {
    /// A release that came out on `date` ("2013-05-17", or just "2013"; empty when
    /// unknown).
    pub fn new(kind: ReleaseKind, date: &str) -> Self {
        let year = date.get(..4).filter(|y| y.chars().all(|c| c.is_ascii_digit())).map(str::to_string);
        ReleaseInfo { year, kind }
    }

    /// The line under its card: "2013 · Album", or "Album" without a year.
    pub fn subtitle(&self) -> String {
        match &self.year {
            Some(year) => format!("{year} · {}", self.kind.label()),
            None => self.kind.label().to_string(),
        }
    }
}

/// An artist's releases in their sections, each newest first by `date` (ISO, so
/// it sorts as text; ties keep the order given). Releases need
/// [`Item::release`]; empty sections are left out.
pub fn discography(releases: Vec<(String, Item)>) -> Vec<Section> {
    ReleaseGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let mut dated: Vec<&(String, Item)> =
                releases.iter().filter(|(_, item)| item.release.as_ref().is_some_and(|r| r.kind.group() == group)).collect();
            dated.sort_by(|a, b| b.0.cmp(&a.0));
            let items: Vec<Item> = dated.into_iter().map(|(_, item)| item.clone()).collect();
            (!items.is_empty()).then(|| Section::new(group.title(), items))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ItemKind;

    fn release(id: &str, kind: ReleaseKind, date: &str) -> (String, Item) {
        let release = ReleaseInfo::new(kind, date);
        let item = Item {
            kind: ItemKind::Album,
            id: id.into(),
            title: id.into(),
            subtitle: release.subtitle(),
            picture: None,
            release: Some(release),
        };
        (date.to_string(), item)
    }

    #[test]
    fn releases_go_in_their_sections_newest_first() {
        let sections = discography(vec![
            release("a", ReleaseKind::Album, "2001-03-12"),
            release("b", ReleaseKind::Ep, "1997"),
            release("c", ReleaseKind::Album, "2013-05-17"),
            release("d", ReleaseKind::Live, ""),
            release("e", ReleaseKind::Single, "2013-01-01"),
        ]);
        let shape: Vec<(&str, Vec<&str>)> =
            sections.iter().map(|s| (s.title.as_str(), s.items.iter().map(|i| i.id.as_str()).collect())).collect();
        assert_eq!(shape, [("Albums", vec!["c", "a"]), ("Singles & EPs", vec!["e", "b"]), ("Live & compilations", vec!["d"])]);
        assert_eq!(sections[0].items[0].subtitle, "2013 · Album");
        assert_eq!(sections[2].items[0].subtitle, "Live", "no year");
    }
}
