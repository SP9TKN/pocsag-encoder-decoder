//! POCSAG z plików, linia na wiadomość; do porównania silnika ze starym dekoderem.
//!     cargo run --release --example dekoduj -- <plik>...
use pogsag_koder::{plik, pocsag};

fn main() {
    for a in std::env::args().skip(1) {
        let Ok((x, fs)) = plik::wczytaj(std::path::Path::new(&a)) else {
            println!("{a}: nie da się wczytać");
            continue;
        };
        let w = pocsag::dekoduj_plik(&x, fs);
        println!("{a}: {} wiadomości", w.len());
        for w in w {
            println!("  {:.1} s | RIC {} | f{} | {} | {} | {}", w.probka as f32 / fs, w.ric, w.funkcja, w.predkosc, w.odczyty().0, w.jakosc());
        }
    }
}
