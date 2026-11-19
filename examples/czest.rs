//! Ustawienie częstotliwości radia tą samą funkcją co program (port CAT otwierany na chwilę).
//!
//!     cargo run --release --example czest -- <icom|yaesu> <port> <bit/s> <MHz> [adres CI-V hex]

use pogsag_koder::ptt::{self, RodzajCat, UstawieniaCat};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        println!("użycie: czest <icom|yaesu> <port> <bit/s> <MHz> [adres CI-V hex]");
        println!("porty: {:?}", ptt::porty());
        return;
    }
    let cat = UstawieniaCat {
        rodzaj: RodzajCat::z_klucza(&a[1]).expect("icom albo yaesu"),
        port: a[2].clone(),
        predkosc: a[3].parse().expect("prędkość"),
        adres_civ: a.get(5).map_or(0x88, |s| u8::from_str_radix(s, 16).expect("adres hex")),
    };
    let hz = ptt::czest_z_tekstu(&a[4]).expect("MHz");
    println!("ramka: {:02X?}", ptt::ramka_czestotliwosci(&cat, hz));
    let t = Instant::now();
    let w = ptt::czestotliwosc_osobno(&cat, hz);
    println!("{} MHz -> {w:?} ({} ms)", ptt::tekst_czest(hz), t.elapsed().as_millis());
}
