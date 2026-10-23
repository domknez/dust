//! Checks against a real server, skipped by default. Point them at a throwaway
//! Navidrome (see `scripts/navidrome-test-server.sh`) and run:
//!
//! ```sh
//! DUST_TEST_SUBSONIC="http://127.0.0.1:4533|dust|dust-test-pw" cargo test subsonic::live -- --ignored
//! ```

use super::{Server, Subsonic};
use crate::media::{Decoded, Playback};
use crate::model::{Likeable, Origin, Track};

fn server() -> Subsonic {
    let spec = std::env::var("DUST_TEST_SUBSONIC").expect("DUST_TEST_SUBSONIC=url|user|password");
    let mut parts = spec.splitn(3, '|');
    let mut next = || parts.next().expect("url|user|password").to_string();
    let server = Server { url: next(), user: next(), password: next() };
    Subsonic::connect(&server).expect("connect")
}

/// Every song in the library, through its albums.
fn all_songs(client: &Subsonic) -> Vec<Track> {
    client.favorite_albums().unwrap().iter().flat_map(|a| client.album(&a.id).unwrap()).collect()
}

/// Decode about a second of audio from `stream`; the track time reached.
fn play_a_second(stream: &mut dyn Playback) -> f64 {
    let mut out = Vec::new();
    let mut reached = stream.written();
    for _ in 0..2000 {
        match stream.decode_next(&mut out) {
            Decoded::Audio { ends_at } => reached = ends_at,
            Decoded::Nothing => {}
            Decoded::End => break,
        }
        if out.len() >= 2 * 44_100 {
            break;
        }
    }
    assert!(out.len() >= 2 * 44_100, "less than a second of audio at {reached:.1} s");
    reached
}

#[test]
#[ignore]
fn live_wrong_password_is_refused() {
    let spec = std::env::var("DUST_TEST_SUBSONIC").expect("DUST_TEST_SUBSONIC");
    let url = spec.split('|').next().unwrap().to_string();
    let err = Subsonic::connect(&Server { url, user: "dust".into(), password: "nope".into() }).err().expect("refused");
    assert!(err.to_string().contains("Wrong username or password"), "{err}");
}

#[test]
#[ignore]
fn live_browse() {
    let client = server();
    let home = client.home().unwrap();
    let titles: Vec<&str> = home.iter().map(|s| s.title.as_str()).collect();
    assert!(titles.contains(&"Recently added") && titles.contains(&"Mixes"), "{titles:?}");
    let albums = client.favorite_albums().unwrap();
    assert!(!albums.is_empty());
    let (tracks, info) = client.album_page(&albums[0].id).unwrap();
    assert!(!tracks.is_empty());
    assert!(!info.year.is_empty());
    let artists = client.favorite_artists().unwrap();
    let page = client.artist(&artists[0].id).unwrap();
    assert!(!page.sections.is_empty() && !page.top.is_empty(), "discography and songs");
    let found = client.search("lab").unwrap();
    assert!(!found.tracks.is_empty());
    let mix = client.mix("random").unwrap();
    assert!(!mix.is_empty());
    let artist_mix = client.mix(&format!("artist:{}", artists[0].id)).unwrap();
    assert!(!artist_mix.is_empty(), "falls back to the artist's own songs");
}

#[test]
#[ignore]
fn live_likes_and_playlists() {
    let client = server();
    let song = &all_songs(&client)[0];
    client.set_liked(&Likeable::Track(song.id.clone()), true).unwrap();
    assert!(client.likes().unwrap().tracks.contains(&song.id));
    client.set_liked(&Likeable::Track(song.id.clone()), false).unwrap();
    assert!(!client.likes().unwrap().tracks.contains(&song.id));

    let id = client.create_playlist("dust live test", std::slice::from_ref(&song.id)).unwrap().expect("the new playlist's id");
    assert_eq!(client.add_to_playlist(&id, std::slice::from_ref(&song.id)).unwrap(), 0, "already there");
    let other = &all_songs(&client)[1];
    assert_eq!(client.add_to_playlist(&id, std::slice::from_ref(&other.id)).unwrap(), 1);
    client.remove_from_playlist(&id, std::slice::from_ref(&song.id)).unwrap();
    assert_eq!(client.playlist(&id).unwrap().iter().map(|t| &t.id).collect::<Vec<_>>(), [&other.id]);
    let playlist = client.playlists().unwrap().into_iter().find(|p| p.id == id).unwrap();
    client.rename_playlist(&playlist, "dust live test 2").unwrap();
    client.delete_playlist(&id).unwrap();
    assert!(client.playlists().unwrap().iter().all(|p| p.id != id));
    // Nothing to read, but the calls must work.
    client.lyrics(&song.id).unwrap();
    client.scrobble(&song.id, false, 0).unwrap();
}

#[test]
#[ignore]
fn live_every_format_plays_and_seeks() {
    let client = server();
    let songs = all_songs(&client);
    assert!(songs.len() >= 5);
    for track in &songs {
        let Origin::Server(song) = &track.origin else { panic!("not a server song") };
        let suffix = &song.suffix;
        let mut stream = client.open_track(track, 0.0).unwrap_or_else(|e| panic!("{suffix}: {e}"));
        assert!(play_a_second(&mut stream) > 0.5, "{suffix}");
        // Jump into the middle: either reopened there or skipped to it.
        let mut later = client.open_track(track, 10.0).unwrap_or_else(|e| panic!("{suffix} at 10 s: {e}"));
        let reached = play_a_second(&mut later);
        assert!((10.0..13.0).contains(&reached), "{suffix}: reached {reached:.2} s");
    }
}
