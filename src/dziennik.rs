//! Historia wiadomości: wpisy w oknie i plik dnia `historia_POCSAG_RRRR-MM-DD.txt` obok programu.
//!
//! Linia pliku: `godzina | kierunek | RIC | funkcja | prędkość | szczegóły | drugi odczyt | treść`,
//! UTF-8 + CRLF. Treść jest ostatnia, a znak „|” w polach tekstowych zamieniany na „¦”,
//! więc plik da się wczytać z powrotem: po restarcie okno pokazuje historię dnia.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub const NAGLOWEK: &str = "godzina | kierunek | RIC | funkcja | prędkość | szczegóły | drugi odczyt | treść";

#[derive(Clone, Debug)]
pub struct Wpis {
    pub godzina: String,
    pub nadane: bool,
    pub ric: u32,
    pub funkcja: u8,
    pub predkosc: u32,
    pub tresc: String,
    /// Przy odbiorze drugi odczyt (cyfry albo tekst), przy nadaniu np. tekst przed zamianą ogonków.
    pub druga: String,
    /// Jakość odbioru albo wynik nadania.
    pub szczegoly: String,
    /// Coś poszło nie tak: błąd nadania, przerwane, słowa z błędem, urwana.
    pub uwaga: bool,
    /// Puste przy pracy na żywo; nazwa pliku przy dekodowaniu z pliku.
    pub zrodlo: String,
    /// Numer wpisu nadania w tej sesji (wpis powstaje na starcie nadania, wynik dochodzi później);
    /// 0 dla pozostałych.
    pub id: u64,
}

impl Wpis {
    pub fn kierunek(&self) -> &'static str {
        if self.nadane { "NADANE" } else { "ODEBRANE" }
    }
}

fn bez_kreski(s: &str) -> String {
    s.replace('|', "¦").replace(['\r', '\n'], " ")
}

/// Lokalna data RRRR-MM-DD i godzina GG:MM:SS sprzed `temu_s` sekund.
pub fn data_i_godzina(temu_s: f32) -> (String, String) {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let t = utc + przesuniecie_strefy(utc) - temu_s.round() as i64;
    let (r, m, d) = data_z_dni(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    (format!("{r:04}-{m:02}-{d:02}"), format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60))
}

/// Czas lokalny minus UTC w sekundach, z zegara Windows (z czasem letnim).
fn przesuniecie_strefy(utc: i64) -> i64 {
    #[cfg(windows)]
    {
        // SYSTEMTIME: rok, miesiąc, dzień tygodnia, dzień, godzina, minuta, sekunda, ms.
        unsafe extern "system" {
            fn GetLocalTime(czas: *mut u16);
        }
        let mut t = [0u16; 8];
        unsafe { GetLocalTime(t.as_mut_ptr()) };
        let lokalna = t[4] as i64 * 3600 + t[5] as i64 * 60 + t[6] as i64;
        let r = (lokalna - utc.rem_euclid(86400)).rem_euclid(86400);
        // Strefy idą od -12 do +14 h co 15 min; zaokrąglenie gubi sekundę między odczytami zegarów.
        let r = if r > 14 * 3600 { r - 86400 } else { r };
        (r + 450).div_euclid(900) * 900
    }
    #[cfg(not(windows))]
    {
        let _ = utc;
        0
    }
}

/// Dni od 1970-01-01 na datę kalendarza gregoriańskiego (algorytm H. Hinnanta).
fn data_z_dni(dni: i64) -> (i64, u32, u32) {
    let z = dni + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

pub fn sciezka_dnia(data: &str) -> std::io::Result<PathBuf> {
    let mut p = std::env::current_exe()?;
    p.set_file_name(format!("historia_POCSAG_{data}.txt"));
    Ok(p)
}

pub fn linia(w: &Wpis) -> String {
    format!(
        "{} | {} | RIC {} | funkcja {} | {} bit/s | {} | {} | {}",
        w.godzina,
        w.kierunek(),
        w.ric,
        w.funkcja,
        w.predkosc,
        bez_kreski(&w.szczegoly),
        bez_kreski(&w.druga),
        bez_kreski(&w.tresc)
    )
}

/// Dopisuje wpis do pliku dnia; nagłówek przy nowym pliku.
pub fn zapisz(data: &str, w: &Wpis) -> std::io::Result<()> {
    let sciezka = sciezka_dnia(data)?;
    let nowy = !sciezka.exists();
    let mut f = OpenOptions::new().create(true).append(true).open(&sciezka)?;
    if nowy {
        write!(f, "{NAGLOWEK}\r\n")?;
    }
    write!(f, "{}\r\n", linia(w))
}

pub fn odczytaj_linie(l: &str) -> Option<Wpis> {
    let p: Vec<&str> = l.splitn(8, " | ").collect();
    if p.len() != 8 {
        return None;
    }
    let nadane = match p[1] {
        "NADANE" => true,
        "ODEBRANE" => false,
        _ => return None,
    };
    let szczegoly = p[5].to_string();
    let male = szczegoly.to_lowercase();
    let uwaga = ["błąd", "przerwane", "słowa z błędem", "urwana"].iter().any(|s| male.contains(s));
    Some(Wpis {
        godzina: p[0].to_string(),
        nadane,
        ric: p[2].strip_prefix("RIC ")?.parse().ok()?,
        funkcja: p[3].strip_prefix("funkcja ")?.parse().ok()?,
        predkosc: p[4].strip_suffix(" bit/s")?.parse().ok()?,
        tresc: p[7].to_string(),
        druga: p[6].to_string(),
        szczegoly,
        uwaga,
        zrodlo: String::new(),
        id: 0,
    })
}

/// Historia z dzisiejszego pliku, najstarsze pierwsze. Nadanie trafia do pliku po swoim końcu,
/// czyli po własnym odbiorze, więc kolejność idzie z godziny (sortowanie stabilne).
pub fn wczytaj_dzis() -> Vec<Wpis> {
    let (data, _) = data_i_godzina(0.0);
    let Ok(sciezka) = sciezka_dnia(&data) else { return Vec::new() };
    let mut w: Vec<Wpis> = std::fs::read_to_string(sciezka)
        .map(|t| t.lines().filter_map(odczytaj_linie).collect())
        .unwrap_or_default();
    w.sort_by(|a, b| a.godzina.cmp(&b.godzina));
    w
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn data_z_dni_kalendarz() {
        assert_eq!(data_z_dni(0), (1970, 1, 1));
        assert_eq!(data_z_dni(19_723 + 59), (2024, 2, 29));
        assert_eq!(data_z_dni(-1), (1969, 12, 31));
    }

    #[test]
    fn linia_tam_i_z_powrotem() {
        let w = Wpis {
            godzina: "12:34:56".into(),
            nadane: true,
            ric: 1234567,
            funkcja: 3,
            predkosc: 1200,
            tresc: "A | B".into(),
            druga: "wpisane: Ą | Ę".into(),
            szczegoly: "BŁĄD: port zajęty".into(),
            uwaga: true,
            zrodlo: String::new(),
            id: 0,
        };
        let l = linia(&w);
        assert_eq!(l, "12:34:56 | NADANE | RIC 1234567 | funkcja 3 | 1200 bit/s | BŁĄD: port zajęty | wpisane: Ą ¦ Ę | A ¦ B");
        let o = odczytaj_linie(&l).unwrap();
        assert_eq!((o.ric, o.funkcja, o.predkosc, o.nadane, o.uwaga), (1234567, 3, 1200, true, true));
        assert_eq!(o.tresc, "A ¦ B");
        assert!(odczytaj_linie(NAGLOWEK).is_none());
    }
}
