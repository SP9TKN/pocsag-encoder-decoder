//! Częstotliwość przez działający silnik (zlecenie w wątku nadajnika), PTT i CAT na jednym porcie.
//!
//!     cargo run --release --example czest_silnik -- <wyjście audio> <port> <MHz> [MHz...]

use pogsag_koder::ptt::{self, RodzajCat, RodzajPtt, UstawieniaCat, UstawieniaPtt};
use pogsag_koder::silnik::{BEZ_NASLUCHU, Silnik, Wspolne};
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        println!("użycie: czest_silnik <wyjście audio> <port> <MHz> [MHz...]");
        return;
    }
    let ust = UstawieniaPtt { rodzaj: RodzajPtt::CatIcom, port: a[2].clone(), predkosc: 9600, adres_civ: 0x88, odwroc: false };
    let cat = UstawieniaCat { rodzaj: RodzajCat::Icom, port: a[2].clone(), predkosc: 9600, adres_civ: 0x88 };
    let wspolne = Wspolne::new();
    let silnik = Silnik::start(BEZ_NASLUCHU, &a[1], &ust, &cat, wspolne.clone()).expect("start");
    for m in &a[3..] {
        let hz = ptt::czest_z_tekstu(m).expect("MHz");
        let t = Instant::now();
        wspolne.ustaw_czestotliwosc(true, &cat, hz);
        loop {
            let s = wspolne.radio.lock().unwrap().clone();
            if let Some((tekst, blad)) = s.filter(|(t, _)| !t.contains("ustawiam")) {
                println!("{m}: {tekst} (błąd: {blad}, {} ms)", t.elapsed().as_millis());
                break;
            }
            if t.elapsed() > Duration::from_secs(5) {
                println!("{m}: brak wyniku po 5 s");
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    drop(silnik);
}
