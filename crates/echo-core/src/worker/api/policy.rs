#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum AuthRoute {
    ThirdParty,
    FirstParty,
    FirstPartyFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ApiEndpoint {
    Library,
    TopTracks,
    RecentlyPlayed,
    FollowedArtists,
    ArtistPage,
    ArtistTopTracks,
}

impl ApiEndpoint {
    pub fn route(self) -> AuthRoute {
        match self {
            // Artist top tracks are approximated with a third-party search: the real
            // endpoint is gone from the dev-mode API and the first-party route's shared
            // client id is quota-throttled.
            Self::Library | Self::ArtistTopTracks => AuthRoute::ThirdParty,
            Self::ArtistPage => AuthRoute::FirstParty,
            Self::TopTracks | Self::RecentlyPlayed | Self::FollowedArtists => {
                AuthRoute::FirstPartyFallback
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::TopTracks => "top_tracks",
            Self::RecentlyPlayed => "recently_played",
            Self::FollowedArtists => "followed_artists",
            Self::ArtistPage => "artist_page",
            Self::ArtistTopTracks => "artist_top_tracks",
        }
    }
}
