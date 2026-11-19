//! Czas dekodowania okna 8 s (tyle co sekundę liczy dekoder na żywo) dla wszystkich prędkości.
//!     cargo run --release --example czas_okna
use pogsag_koder::koder::{self, DoNadania, Modulacja, Rodzaj};
use pogsag_koder::pocsag;
use std::time::Instant;

fn main() {
    let fs = 48000.0f32;
    let w = DoNadania { ric: 1000100, funkcja: 3, rodzaj: Rodzaj::Tekst, tresc: "101: TAKOTAK JAKO".into() };
    let b = koder::bity(&[w]);
    for predkosc in pocsag::PREDKOSCI {
        let mut x = vec![0.0f32; (1.0 * fs) as usize];
        x.extend(koder::audio(&b, &Modulacja { predkosc, fs, poziom: 0.5, odwroc: false }));
        x.resize((8.0 * fs) as usize, 0.0);
        let mut st = 1u32;
        for v in x.iter_mut() {
            st = st.wrapping_mul(1664525).wrapping_add(1013904223);
            *v += ((st >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.02;
        }
        let t = Instant::now();
        let n = 5;
        let mut ile = 0;
        for _ in 0..n {
            ile = pocsag::dekoduj(&x, fs).len();
        }
        println!("{predkosc} bit/s: okno 8 s w {:.0} ms, wiadomości {ile}", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    }
}
