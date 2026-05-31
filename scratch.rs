use rspotify::model::{PlaylistId, Id};

fn main() {
    let id = PlaylistId::from_id("37i9dQZF1DXcBWIGoYBM5M").unwrap();
    println!("from_id: {}", id.to_string());
    println!("uri: {}", id.uri());
    println!("id: {}", id.id());
}
