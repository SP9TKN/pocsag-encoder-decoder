//! Korekcja toru nadawczego radia: FIR odwracający charakterystykę wejścia audio nadajnika.
//!
//! IC-7100 przez USB (także w FM-D) podbija wysokie tony (preemfaza ~6 dB/okt.) i obcina dół
//! poniżej ~80 Hz. Prostokąt NRZ robi się wtedy szpilkami na zboczach (dobijają do ogranicznika
//! dewiacji), a środek bitu ma tylko ~±0,8 kHz. Pomiar 06.11.2026 (RSP1B, 435 MHz, 512 bit/s):
//! bez korekcji środek bitu ±0,76 kHz, z tym filtrem ±3,5 kHz, zapas od nośnej ~60 Hz -> ~1 kHz.
//! Filtr = `korekcja_tx_usb_ic7100_v2.txt` z pomiaru toru USB IC-7100 dla C4FM (12.10.2026).

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Korekcja {
    Brak,
    Ic7100Usb,
}

impl Korekcja {
    pub const WSZYSTKIE: [Korekcja; 2] = [Korekcja::Brak, Korekcja::Ic7100Usb];

    pub fn klucz(self) -> &'static str {
        match self {
            Korekcja::Brak => "brak",
            Korekcja::Ic7100Usb => "ic7100_usb",
        }
    }

    pub fn z_klucza(s: &str) -> Option<Self> {
        Self::WSZYSTKIE.into_iter().find(|k| k.klucz() == s)
    }

    pub fn nazwa(self) -> &'static str {
        match self {
            Korekcja::Brak => "bez korekty",
            Korekcja::Ic7100Usb => "korekta 7100",
        }
    }
}

const FIR_IC7100_USB: &str = include_str!("../assets/korekcja_tx_usb_ic7100_v2.txt");
/// Częstotliwość, dla której policzono współczynniki.
const FS_FIR: f32 = 48_000.0;

fn wspolczynniki(tekst: &str) -> Vec<f32> {
    tekst
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.parse::<f32>().ok())
        .collect()
}

fn fir(k: Korekcja) -> &'static [f32] {
    static IC7100: OnceLock<Vec<f32>> = OnceLock::new();
    match k {
        Korekcja::Brak => &[],
        Korekcja::Ic7100Usb => IC7100.get_or_init(|| wspolczynniki(FIR_IC7100_USB)),
    }
}

/// Współczynniki dla innej częstotliwości karty: odpowiedź impulsowa próbkowana na nowo (liniowo),
/// z zachowanym wzmocnieniem. Wystarcza, bo korekcja działa w paśmie danych (< 3 kHz).
fn dla_fs(h: &[f32], fs: f32) -> Vec<f32> {
    if (fs - FS_FIR).abs() < 0.5 {
        return h.to_vec();
    }
    let r = FS_FIR / fs;
    let n = ((h.len() - 1) as f32 / r).floor() as usize + 1;
    (0..n)
        .map(|i| {
            let t = i as f32 * r;
            let k = t as usize;
            let u = t - k as f32;
            let a = h[k];
            let b = h.get(k + 1).copied().unwrap_or(0.0);
            (a + (b - a) * u) * r
        })
        .collect()
}

/// Pełny splot (długość x + h - 1); osiem sum naraz, żeby kompilator użył SIMD.
fn splot(x: &[f32], h: &[f32]) -> Vec<f32> {
    let l = h.len();
    let hr: Vec<f32> = h.iter().rev().copied().collect();
    let mut xp = vec![0.0f32; l - 1];
    xp.extend_from_slice(x);
    xp.extend(std::iter::repeat_n(0.0, l - 1));
    (0..x.len() + l - 1)
        .map(|n| {
            let okno = &xp[n..n + l];
            let mut acc = [0.0f32; 8];
            let (a, b) = (okno.chunks_exact(8), hr.chunks_exact(8));
            let (ra, rb) = (a.remainder(), b.remainder());
            for (ca, cb) in a.zip(b) {
                for j in 0..8 {
                    acc[j] += ca[j] * cb[j];
                }
            }
            acc.iter().sum::<f32>() + ra.iter().zip(rb).map(|(p, q)| p * q).sum::<f32>()
        })
        .collect()
}

/// Sygnał po korekcji toru o tym samym szczycie co wejście (poziom wyjścia znaczy to samo:
/// szczyt próbek). Bez korekcji zwraca wejście bez zmian. Wynik jest dłuższy o długość filtra
/// (opóźnienie ~43 ms i wybrzmienie), więc dane nie są ucięte.
pub fn koryguj(x: &[f32], fs: f32, k: Korekcja) -> Vec<f32> {
    let h = fir(k);
    if h.is_empty() || x.is_empty() {
        return x.to_vec();
    }
    let h = dla_fs(h, fs);
    let mut y = splot(x, &h);
    let szczyt_we = x.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    let szczyt_wy = y.iter().fold(0.0f32, |a, &v| a.max(v.abs())).max(1e-9);
    let s = szczyt_we / szczyt_wy;
    y.iter_mut().for_each(|v| *v *= s);
    y
}

#[cfg(test)]
mod testy {
    use super::*;
    use crate::koder::{self, DoNadania, Modulacja, Rodzaj};

    #[test]
    fn filtr_wczytany() {
        let h = fir(Korekcja::Ic7100Usb);
        assert_eq!(h.len(), 4096);
        assert!(h.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn klucze_tam_i_z_powrotem() {
        for k in Korekcja::WSZYSTKIE {
            assert_eq!(Korekcja::z_klucza(k.klucz()), Some(k));
        }
        assert_eq!(Korekcja::z_klucza("cos"), None);
    }

    #[test]
    fn brak_nie_zmienia() {
        let x: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        assert_eq!(koryguj(&x, 48_000.0, Korekcja::Brak), x);
    }

    #[test]
    fn splot_jak_wprost() {
        let x: Vec<f32> = (0..300).map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0).collect();
        let h: Vec<f32> = (0..37).map(|i| ((i * 31) % 13) as f32 / 6.0 - 1.0).collect();
        let y = splot(&x, &h);
        assert_eq!(y.len(), x.len() + h.len() - 1);
        for (n, &v) in y.iter().enumerate() {
            let mut w = 0.0f32;
            for (k, &hk) in h.iter().enumerate() {
                if n >= k && n - k < x.len() {
                    w += hk * x[n - k];
                }
            }
            assert!((v - w).abs() < 1e-4, "n={n}: {v} vs {w}");
        }
    }

    /// |H(f)| filtra w dB (suma wprost po współczynnikach).
    fn db(h: &[f32], fs: f32, f: f32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, &v) in h.iter().enumerate() {
            let a = -2.0 * std::f64::consts::PI * f as f64 * n as f64 / fs as f64;
            re += v as f64 * a.cos();
            im += v as f64 * a.sin();
        }
        (20.0 * (re * re + im * im).sqrt().log10()) as f32
    }

    /// Filtr odwraca zmierzony tor IC-7100 przez USB: podbija dół (tor obcina poniżej ~80 Hz)
    /// i opada ~6 dB/okt. od ~130 Hz (tor ma tam preemfazę). Wartości z pliku: 40 Hz +10,7 dB,
    /// 256 Hz -0,5 dB, 2 kHz -16,6 dB.
    #[test]
    fn charakterystyka_odwraca_tor_7100() {
        let h = fir(Korekcja::Ic7100Usb);
        let (d40, d256, d1k, d2k) = (db(h, 48e3, 40.0), db(h, 48e3, 256.0), db(h, 48e3, 1000.0), db(h, 48e3, 2000.0));
        assert!(d40 - d256 > 8.0, "dol: 40 Hz {d40} vs 256 Hz {d256}");
        assert!(d256 - d1k > 9.0 && d1k - d2k > 3.0, "spadek: 256 {d256}, 1k {d1k}, 2k {d2k}");
    }

    /// Karta 44,1 kHz: przeliczone współczynniki mają tę samą charakterystykę w paśmie danych.
    #[test]
    fn inna_czestotliwosc_karty() {
        let h = fir(Korekcja::Ic7100Usb);
        let h44 = dla_fs(h, 44_100.0);
        for f in [30.0, 128.0, 256.0, 640.0, 1280.0, 2400.0] {
            let (a, b) = (db(h, 48e3, f), db(&h44, 44_100.0, f));
            assert!((a - b).abs() < 0.5, "{f} Hz: 48k {a} dB, 44,1k {b} dB");
        }
    }

    /// Nadanie z korektą: szczyt jak na wejściu (poziom znaczy to samo), dane całe (dłuższe o filtr).
    #[test]
    fn poziom_i_dlugosc() {
        let w = DoNadania { ric: 1234567, funkcja: 3, rodzaj: Rodzaj::Tekst, tresc: "TEST".into() };
        let x = koder::audio(&koder::bity(&[w]), &Modulacja { predkosc: 512, fs: 48_000.0, poziom: 0.8, odwroc: true });
        let y = koryguj(&x, 48_000.0, Korekcja::Ic7100Usb);
        assert_eq!(y.len(), x.len() + 4095);
        let s = y.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!((s - 0.8).abs() < 1e-4, "szczyt {s}");
    }
}
