//! Koder POCSAG: wiadomości -> słowa kodowe -> bity -> audio NRZ na wejście nadajnika FM.
//!
//! Nadanie: preambuła 576 bitów 1010..., potem paczki: sync i 8 ramek po 2 słowa. Adres musi
//! leżeć w ramce równej 3 najmłodszym bitom RIC, a treść idzie dalej, także przez kolejne paczki.

use crate::pocsag::{IDLE, SLOW_W_PACZCE, SYNC, syndrom};

/// RIC ma 21 bitów: 18 w słowie adresu i 3 w numerze ramki.
pub const MAX_RIC: u32 = (1 << 21) - 1;
pub const PREAMBULA_BITOW: usize = 576;
/// Dłuższych treści wiele pagerów i tak nie pokaże.
pub const MAX_ZNAKOW: usize = 240;
/// Zbocza bitów jako połówka kosinusa o tej szerokości (w bitach): węższe pasmo niż prostokąt.
const ZBOCZE_BITU: f64 = 0.4;
/// Łagodny początek i koniec nośnej danych, bez trzasku.
const NARASTANIE_S: f64 = 0.003;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rodzaj {
    Tekst,
    Cyfry,
    /// Samo wywołanie: słowo adresu bez treści (pager tylko piszczy).
    Sygnal,
}

impl Rodzaj {
    pub const WSZYSTKIE: [Rodzaj; 3] = [Rodzaj::Tekst, Rodzaj::Cyfry, Rodzaj::Sygnal];

    pub fn nazwa(self) -> &'static str {
        match self {
            Rodzaj::Tekst => "tekst",
            Rodzaj::Cyfry => "cyfry",
            Rodzaj::Sygnal => "sam sygnał",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DoNadania {
    pub ric: u32,
    pub funkcja: u8,
    pub rodzaj: Rodzaj,
    /// Treść już po `przygotuj`: tylko znaki, które da się zakodować.
    pub tresc: String,
}

/// Słowo kodowe: 21 bitów danych, BCH(31,21) i bit parzystości.
pub fn bch(d21: u32) -> u32 {
    let cw = ((d21 << 10) | syndrom(d21 << 10)) << 1;
    cw | (cw.count_ones() & 1)
}

/// Polskie litery bez ogonków (pagery znają tylko ASCII), inne znaki spoza ASCII jako „?”.
pub fn do_ascii(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            'Ą' => 'A',
            'Ć' => 'C',
            'Ę' => 'E',
            'Ł' => 'L',
            'Ń' => 'N',
            'Ó' => 'O',
            'Ś' => 'S',
            'Ź' | 'Ż' => 'Z',
            ' '..='~' => c,
            _ => '?',
        })
        .collect()
}

/// Kod 4-bitowy znaku wiadomości cyfrowej; ten sam zestaw co w dekoderze: "0123456789*U -)(".
fn kod_cyfry(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c as u32 - '0' as u32,
        '*' => 0xA,
        'U' | 'u' => 0xB,
        ' ' => 0xC,
        '-' => 0xD,
        ')' | ']' => 0xE,
        '(' | '[' => 0xF,
        _ => return None,
    })
}

/// Treść, która naprawdę pójdzie w eter, albo opis, czego nie da się nadać.
pub fn przygotuj(rodzaj: Rodzaj, tresc: &str) -> Result<String, String> {
    match rodzaj {
        Rodzaj::Sygnal => Ok(String::new()),
        Rodzaj::Tekst => {
            let t = do_ascii(tresc.trim_end());
            if t.is_empty() {
                Err("Pusta wiadomość: wpisz tekst albo wybierz typ „sam sygnał”".into())
            } else {
                Ok(t.chars().take(MAX_ZNAKOW).collect())
            }
        }
        Rodzaj::Cyfry => {
            let t = tresc.trim();
            if t.is_empty() {
                return Err("Pusta wiadomość: wpisz cyfry albo wybierz typ „sam sygnał”".into());
            }
            if let Some(c) = t.chars().find(|&c| kod_cyfry(c).is_none()) {
                return Err(format!("Znak „{c}” nie istnieje w wiadomości cyfrowej (dozwolone: 0-9 * U spacja - ( ) [ ])"));
            }
            Ok(t.chars().take(MAX_ZNAKOW).map(|c| c.to_ascii_uppercase()).collect())
        }
    }
}

/// Słowa treści: bity znaków najmłodszy pierwszy, po 20 w słowie z bitem 31 = 1.
fn slowa_tresci(rodzaj: Rodzaj, tresc: &str) -> Vec<u32> {
    let mut bity: Vec<u32> = Vec::new();
    match rodzaj {
        Rodzaj::Sygnal => return Vec::new(),
        Rodzaj::Tekst => {
            // EOT na końcu: pager i dekoder wiedzą, że dalsze bity to wypełnienie.
            for c in tresc.bytes().chain([4u8]) {
                bity.extend((0..7).map(|j| (c as u32 >> j) & 1));
            }
            bity.resize(bity.len().div_ceil(20) * 20, 0);
        }
        Rodzaj::Cyfry => {
            for c in tresc.chars() {
                let k = kod_cyfry(c).unwrap_or(0xC);
                bity.extend((0..4).map(|j| (k >> j) & 1));
            }
            // Wypełnienie spacjami (0xC), jak robią nadajniki pagerów.
            while bity.len() % 20 != 0 {
                let j = (bity.len() % 4) as u32;
                bity.push((0xC >> j) & 1);
            }
        }
    }
    bity.chunks(20).map(|z| bch((1 << 20) | z.iter().fold(0, |a, &b| (a << 1) | b))).collect()
}

/// Słowa kodowe całego nadania, w paczkach po 16 (bez syncu).
pub fn slowa(wiad: &[DoNadania]) -> Vec<u32> {
    let mut s = Vec::new();
    for w in wiad {
        // Adres czeka na swoją ramkę; poprzednia wiadomość kończy się na nim albo na idle.
        while (s.len() % SLOW_W_PACZCE) / 2 != (w.ric & 7) as usize {
            s.push(IDLE);
        }
        s.push(bch(((w.ric >> 3) << 2) | (w.funkcja & 3) as u32));
        s.extend(slowa_tresci(w.rodzaj, &w.tresc));
    }
    s.push(IDLE);
    while s.len() % SLOW_W_PACZCE != 0 {
        s.push(IDLE);
    }
    s
}

/// Bity nadania: preambuła 1010... i paczki z syncem.
pub fn bity(wiad: &[DoNadania]) -> Vec<u8> {
    let mut b: Vec<u8> = (0..PREAMBULA_BITOW).map(|i| (i % 2 == 0) as u8).collect();
    for paczka in slowa(wiad).chunks(SLOW_W_PACZCE) {
        for &w in std::iter::once(&SYNC).chain(paczka) {
            b.extend((0..32).map(|i| ((w >> (31 - i)) & 1) as u8));
        }
    }
    b
}

/// Sama preambuła przez zadany czas: prostokąt o połowie prędkości bitowej, do strojenia poziomu.
pub fn bity_probne(predkosc: u32, sekundy: f32) -> Vec<u8> {
    let n = (predkosc as f32 * sekundy) as usize;
    (0..n).map(|i| (i % 2 == 0) as u8).collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Modulacja {
    pub predkosc: u32,
    pub fs: f32,
    /// Amplituda 0..1.
    pub poziom: f32,
    /// POCSAG: bit 1 = niższa częstotliwość, więc domyślnie bit 1 = ujemne napięcie.
    pub odwroc: bool,
}

/// NRZ z zaokrąglonymi zboczami. Zegar ułamkowy, więc prędkość jest dokładna przy każdym fs.
pub fn audio(bity: &[u8], m: &Modulacja) -> Vec<f32> {
    if bity.is_empty() {
        return Vec::new();
    }
    let poziom = |b: u8| if (b == 1) != m.odwroc { -1.0 } else { 1.0 };
    let na_probke = m.predkosc as f64 / m.fs as f64;
    let n = (bity.len() as f64 / na_probke).ceil() as usize;
    let pol = ZBOCZE_BITU / 2.0;
    let narastanie = (NARASTANIE_S * m.fs as f64).max(1.0);
    (0..n)
        .map(|i| {
            let t = i as f64 * na_probke;
            let k = (t as usize).min(bity.len() - 1);
            let u = t - k as f64;
            let biezacy = poziom(bity[k]);
            let mut v = biezacy;
            if u < pol && k > 0 {
                // Druga połowa zbocza od poprzedniego bitu.
                let s = 0.5 - 0.5 * (std::f64::consts::PI * (u + pol) / ZBOCZE_BITU).cos();
                v = poziom(bity[k - 1]) + (biezacy - poziom(bity[k - 1])) * s;
            } else if u > 1.0 - pol && k + 1 < bity.len() {
                let s = 0.5 - 0.5 * (std::f64::consts::PI * (u - 1.0 + pol) / ZBOCZE_BITU).cos();
                v = biezacy + (poziom(bity[k + 1]) - biezacy) * s;
            }
            let obwiednia = (i as f64 / narastanie).min((n - 1 - i) as f64 / narastanie).min(1.0);
            (v * obwiednia * m.poziom as f64) as f32
        })
        .collect()
}

/// Czas nadania w sekundach (bez opóźnienia PTT i ogona).
pub fn czas_s(bitow: usize, predkosc: u32) -> f32 {
    bitow as f32 / predkosc as f32
}

/// POCSAG nie przesyła nadawcy, więc jego RIC idzie na początku treści: „(1234567) treść”.
/// Nawiasy i cyfry są też w wiadomościach cyfrowych i czytelne na ekranie pagera.
pub fn z_nadawca(ric: u32, tresc: &str) -> String {
    format!("({ric}) {}", tresc.trim_start())
}

/// RIC z 1-7 cyfr (do 2097151).
fn ric_z_cyfr(numer: &str) -> Option<u32> {
    if (1..=7).contains(&numer.len()) && numer.bytes().all(|b| b.is_ascii_digit()) {
        numer.parse::<u32>().ok().filter(|r| *r <= MAX_RIC)
    } else {
        None
    }
}

/// Numer nadawcy z początku treści i reszta. Dwie postacie: ta programu, „(101) treść”,
/// i spotykana w innych nadajnikach, „101: treść”. „(pilne) ...” nie ma cyfr, a „12:30 ...”
/// ma cyfrę zaraz po dwukropku, więc żadne z nich nie jest brane za numer.
pub fn nadawca(tresc: &str) -> (Option<u32>, &str) {
    let t = tresc.trim_start();
    if let Some(reszta) = t.strip_prefix('(')
        && let Some(k) = reszta.find(')')
        && let Some(ric) = ric_z_cyfr(&reszta[..k])
    {
        return (Some(ric), reszta[k + 1..].trim_start());
    }
    if let Some(k) = t.find(':')
        && let Some(ric) = ric_z_cyfr(&t[..k])
        && !t[k + 1..].starts_with(|c: char| c.is_ascii_digit())
    {
        return (Some(ric), t[k + 1..].trim_start());
    }
    (None, tresc)
}

#[cfg(test)]
mod testy {
    use super::*;
    use crate::pocsag::{self, PREDKOSCI, popraw};

    fn wiad(ric: u32, funkcja: u8, rodzaj: Rodzaj, tresc: &str) -> DoNadania {
        DoNadania { ric, funkcja, rodzaj, tresc: przygotuj(rodzaj, tresc).unwrap() }
    }

    /// Tor radia jak w testach dekodera: jednobiegunowe filtry, górnoprzepustowy 250 Hz, szum.
    /// Płaski tor (wyjście dyskryminatora, SDR) to sam szum.
    fn tor_radia(x: &[f32], fs: f32, plaski: bool, deemfaza: bool) -> Vec<f32> {
        let mut y: Vec<f32> = vec![0.0; (0.4 * fs) as usize];
        y.extend_from_slice(x);
        y.extend(vec![0.0; (0.6 * fs) as usize]);
        let filtr = |y: &mut Vec<f32>, fc: f32, gorny: bool| {
            let a = (-2.0 * std::f32::consts::PI * fc / fs).exp();
            let mut lp = 0.0f32;
            for v in y.iter_mut() {
                lp = a * lp + (1.0 - a) * *v;
                *v = if gorny { *v - lp } else { lp };
            }
        };
        if deemfaza {
            filtr(&mut y, 300.0, false);
        }
        for _ in 0..4 {
            if !plaski {
                filtr(&mut y, 250.0, true);
            }
        }
        let mut st = 77u32;
        for v in y.iter_mut() {
            st = st.wrapping_mul(1664525).wrapping_add(1013904223);
            *v += ((st >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.01;
        }
        y
    }

    #[test]
    fn slowa_kodowe_poprawne() {
        for d in [0u32, 1, 0x1F_FFFF, (148036 >> 3) << 2 | 2] {
            let c = bch(d);
            assert_eq!(popraw(c), Some((c, 0)), "{d:x}");
            assert_eq!(c >> 11, d);
        }
    }

    #[test]
    fn adres_w_swojej_ramce() {
        for ric in [0u32, 1, 7, 8, 123454, MAX_RIC] {
            let s = slowa(&[wiad(ric, 3, Rodzaj::Sygnal, "")]);
            let poz = s.iter().position(|&w| w != IDLE).unwrap();
            assert_eq!(poz / 2, (ric & 7) as usize);
            assert_eq!(s.len() % 16, 0);
        }
    }

    #[test]
    fn polskie_znaki_i_cyfry() {
        assert_eq!(do_ascii("Zażółć gęślą jaźń"), "Zazolc gesla jazn");
        assert_eq!(przygotuj(Rodzaj::Cyfry, "12-34 u").unwrap(), "12-34 U");
        assert!(przygotuj(Rodzaj::Cyfry, "12a").is_err());
        assert!(przygotuj(Rodzaj::Tekst, "  ").is_err());
        assert_eq!(przygotuj(Rodzaj::Sygnal, "cokolwiek").unwrap(), "");
    }

    #[test]
    fn numer_nadawcy_na_poczatku() {
        assert_eq!(z_nadawca(123454, "  TEST ODBIORU"), "(123454) TEST ODBIORU");
        assert_eq!(nadawca("(123454) TEST ODBIORU"), (Some(123454), "TEST ODBIORU"));
        assert_eq!(nadawca("(123454)"), (Some(123454), ""));
        assert_eq!(nadawca("(pilne) spotkanie"), (None, "(pilne) spotkanie"));
        assert_eq!(nadawca("12:30 spotkanie"), (None, "12:30 spotkanie"));
        assert_eq!(nadawca("(12345678) za długi"), (None, "(12345678) za długi"));
        assert_eq!(nadawca("(2097152) ponad RIC"), (None, "(2097152) ponad RIC"));
        assert_eq!(nadawca("(5) (123) dalej"), (Some(5), "(123) dalej"));
        // Postać „101: treść” z innego nadajnika (wiadomości z 04.11).
        assert_eq!(nadawca("101: TAKOTAK"), (Some(101), "TAKOTAK"));
        assert_eq!(nadawca("101: JAKOH.A?"), (Some(101), "JAKOH.A?"));
        assert_eq!(nadawca("101:bez spacji"), (Some(101), "bez spacji"));
        assert_eq!(nadawca("101:"), (Some(101), ""));
        assert_eq!(nadawca("15:00 zebranie"), (None, "15:00 zebranie"), "godzina to nie numer");
        assert_eq!(nadawca("Uwaga: 101 osob"), (None, "Uwaga: 101 osob"));
        assert_eq!(nadawca("12345678: za długi"), (None, "12345678: za długi"));
        // Wiadomość cyfrowa z nadawcą przechodzi przez koder i dekoder.
        let tresc = przygotuj(Rodzaj::Cyfry, &z_nadawca(1234567, "555-123")).unwrap();
        let b = bity(&[DoNadania { ric: 1000200, funkcja: 0, rodzaj: Rodzaj::Cyfry, tresc }]);
        let mut x = vec![0.0f32; 20000];
        x.extend(audio(&b, &Modulacja { predkosc: 1200, fs: 48000.0, poziom: 0.5, odwroc: false }));
        x.extend(vec![0.0f32; 48000]);
        let w = crate::pocsag::dekoduj(&x, 48000.0);
        assert_eq!(nadawca(&w[0].cyfry()), (Some(1234567), "555-123"));
    }

    #[test]
    fn koder_dekoder_przez_tor_radia() {
        // Małe RIC mają w słowie adresu kilkanaście zer z rzędu, a tor radia (górnoprzepustowy
        // 250 Hz, deemfaza) zostawia z takich ciągów tylko zbocza; dekoder odbudowuje je
        // korektorem ze sprzężeniem od decyzji (przed nim te przypadki nie przechodziły).
        for (fs, plaski, deemfaza, ric3, ric4) in [
            (48000.0, true, false, 200, 9),
            (44100.0, true, false, 200, 9),
            (48000.0, false, false, 200, 9),
            (44100.0, false, true, 200, 9),
            (48000.0, false, true, 1000200, 987657),
        ] {
            let lista = vec![
                wiad(1234567, 3, Rodzaj::Tekst, "Próba kodera POCSAG, 73!"),
                wiad(123454, 2, Rodzaj::Tekst, "PROBA TEST"),
                wiad(ric3, 0, Rodzaj::Cyfry, "0123456789 U-()"),
                wiad(ric4, 1, Rodzaj::Sygnal, ""),
            ];
            let b = bity(&lista);
            for predkosc in PREDKOSCI {
                for odwroc in [false, true] {
                    let x = audio(&b, &Modulacja { predkosc, fs, poziom: 0.5, odwroc });
                    let w = pocsag::dekoduj(&tor_radia(&x, fs, plaski, deemfaza), fs);
                    let opis = format!("{predkosc} odwr={odwroc} fs={fs} płaski={plaski} deemfaza={deemfaza}");
                    assert_eq!(w.len(), 4, "{opis}: {w:?}");
                    assert_eq!((w[0].ric, w[0].funkcja, w[0].tekst().as_str()), (1234567, 3, "Proba kodera POCSAG, 73!"), "{opis}");
                    assert_eq!((w[1].ric, w[1].funkcja, w[1].tekst().as_str()), (123454, 2, "PROBA TEST"), "{opis}");
                    assert_eq!((w[2].ric, w[2].funkcja, w[2].cyfry().as_str()), (ric3, 0, "0123456789 U-()"), "{opis}");
                    assert_eq!((w[3].ric, w[3].funkcja, w[3].dane.len()), (ric4, 1, 0), "{opis}");
                    let jakosc: Vec<_> = w.iter().map(|w| (w.poprawki, w.bledne_slowa, w.zakonczona)).collect();
                    assert!(w.iter().all(|w| w.bledne_slowa == 0 && w.zakonczona), "{opis}: {jakosc:?}");
                    // Płaski tor: każde słowo kodera dochodzi bez jednego złego bitu.
                    if plaski {
                        assert!(w.iter().all(|w| w.poprawki == 0), "{opis}: {jakosc:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn dlugosc_i_poziom_audio() {
        let b = bity(&[wiad(8, 0, Rodzaj::Tekst, "A")]);
        // Preambuła + 1 paczka (adres w ramce 0, treść mieści się w niej).
        assert_eq!(b.len(), 576 + 544);
        let x = audio(&b, &Modulacja { predkosc: 512, fs: 48000.0, poziom: 0.3, odwroc: false });
        assert_eq!(x.len(), (b.len() as f64 * 48000.0 / 512.0).ceil() as usize);
        let szczyt = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!((szczyt - 0.3).abs() < 1e-4);
        assert_eq!(x[0], 0.0);
    }
}


