//! Test na żywo całego toru: nadanie przez kabel wirtualny, odbiór tym samym silnikiem, PTT na
//! jednym końcu pary com0com, a jego stan czytany z drugiego końca (DTR -> DSR, RTS -> CTS,
//! CAT jako bajty). Wiadomości trafiają do pliku historii obok exe przykładu.
//!
//!     cargo run --release --example petla -- <wyjście> <wejście> <port PTT> <port podglądu> <dtr|rts|dtr_rts|icom|yaesu>

use pogsag_koder::dziennik;
use pogsag_koder::koder::{self, DoNadania, Rodzaj};
use pogsag_koder::ptt::{RodzajPtt, UstawieniaPtt};
use pogsag_koder::silnik::{self, Parametry, Silnik, Wspolne, Zlecenie};
use std::io::Read;
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 {
        println!("użycie: petla <wyjście> <wejście> <port PTT> <port podglądu> <dtr|rts|dtr_rts|icom|yaesu>");
        println!("wyjścia: {:?}", silnik::urzadzenia_wyjsciowe());
        println!("wejścia: {:?}", silnik::urzadzenia_wejsciowe());
        println!("porty: {:?}", pogsag_koder::ptt::porty());
        return;
    }
    let rodzaj = RodzajPtt::z_klucza(&a[5]).expect("rodzaj PTT");
    let ust = UstawieniaPtt { rodzaj, port: a[3].clone(), predkosc: 9600, adres_civ: 0x88, odwroc: false };
    let mut pod = serialport::new(&a[4], 9600)
        .dtr_on_open(false)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(1))
        .open()
        .expect("port podglądu");
    let _ = pod.write_data_terminal_ready(false);
    let _ = pod.write_request_to_send(false);

    let wspolne = Wspolne::new();
    let n0 = wspolne.historia.lock().unwrap().len();
    let t0 = Instant::now();
    let cat = pogsag_koder::ptt::UstawieniaCat { rodzaj: pogsag_koder::ptt::RodzajCat::Brak, port: String::new(), predkosc: 9600, adres_civ: 0x88 };
    let silnik = Silnik::start(&a[2], &a[1], &ust, &cat, wspolne.clone()).expect("start");
    println!("{:7.3} s  start silnika", t0.elapsed().as_secs_f32());

    let par = |predkosc| Parametry { predkosc, poziom: 0.5, odwroc: false, powtorzenia: 1, opoznienie_ms: 300, ogon_ms: 100, korekcja: pogsag_koder::korekcja::Korekcja::Brak };
    let wiad = |ric, funkcja, rodzaj, tekst: &str| DoNadania { ric, funkcja, rodzaj, tresc: koder::przygotuj(rodzaj, tekst).unwrap() };
    let zlecenia = [
        (wiad(1234567, 3, Rodzaj::Tekst, "Test pętli 512 zażółć"), "Test pętli 512 zażółć", par(512)),
        (wiad(123454, 2, Rodzaj::Cyfry, "0123 456-789"), "0123 456-789", par(512)),
        (wiad(1000200, 3, Rodzaj::Tekst, "Petla 1200 bit/s"), "Petla 1200 bit/s", par(1200)),
        (wiad(987657, 1, Rodzaj::Sygnal, ""), "", par(2400)),
    ];
    let n = zlecenia.len();
    for (w, wpisane, p) in zlecenia {
        wspolne.zlec(Zlecenie::Wiadomosc { w, wpisane: wpisane.into(), par: p });
    }

    let mut ptt = false;
    let mut wlaczenie = t0;
    let mut okresy = Vec::new();
    let mut koniec_od: Option<Instant> = None;
    let mut bajty = Vec::new();
    // DTR+RTS: odczyty, w których tylko jedna linia była w górze (DTR -> DSR, RTS -> CTS).
    let mut jedna_linia = 0;
    while t0.elapsed() < Duration::from_secs(90) {
        let teraz = match rodzaj {
            RodzajPtt::Dtr => pod.read_data_set_ready().unwrap_or(false),
            RodzajPtt::Rts => pod.read_clear_to_send().unwrap_or(false),
            RodzajPtt::DtrRts => {
                let (dsr, cts) = (pod.read_data_set_ready().unwrap_or(false), pod.read_clear_to_send().unwrap_or(false));
                if dsr != cts {
                    jedna_linia += 1;
                }
                dsr && cts
            }
            _ => {
                let mut b = [0u8; 64];
                if let Ok(k) = pod.read(&mut b) {
                    bajty.extend_from_slice(&b[..k]);
                    println!("{:7.3} s  CAT: {:02X?}", t0.elapsed().as_secs_f32(), &b[..k]);
                }
                ptt
            }
        };
        if teraz != ptt {
            ptt = teraz;
            if ptt {
                wlaczenie = Instant::now();
            } else {
                okresy.push(wlaczenie.elapsed().as_secs_f32());
            }
            println!("{:7.3} s  PTT {}", t0.elapsed().as_secs_f32(), if ptt { "WŁ" } else { "wył" });
        }
        let h = wspolne.historia.lock().unwrap();
        let nowe = &h[n0..];
        let gotowe = nowe.iter().filter(|w| w.nadane).count() == n
            && nowe.iter().filter(|w| !w.nadane).count() >= n
            && !wspolne.tx.lock().unwrap().nadaje
            && wspolne.w_kolejce() == 0;
        drop(h);
        if gotowe && koniec_od.is_none() {
            koniec_od = Some(Instant::now());
        }
        // Jeszcze 3 s na ewentualne fałszywe odbiory.
        if koniec_od.is_some_and(|k| k.elapsed() > Duration::from_secs(3)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let (luk, ms) = wspolne.luki();
    println!("dziury w nagraniu wejścia: {luk} (razem {ms} ms)");
    drop(silnik);
    std::thread::sleep(Duration::from_millis(50));
    let po_zamknieciu = match rodzaj {
        RodzajPtt::Dtr => pod.read_data_set_ready().ok(),
        RodzajPtt::Rts => pod.read_clear_to_send().ok(),
        RodzajPtt::DtrRts => Some(pod.read_data_set_ready().unwrap_or(true) || pod.read_clear_to_send().unwrap_or(true)),
        _ => None,
    };
    if rodzaj == RodzajPtt::DtrRts {
        println!("odczyty z jedną linią w górze: {jedna_linia}");
    }
    println!("\nczasy PTT (s): {okresy:?}");
    println!("linia PTT po zamknięciu silnika: {po_zamknieciu:?}");
    if !bajty.is_empty() {
        println!("CAT razem: {bajty:02X?}");
    }
    println!("\nhistoria:");
    for w in &wspolne.historia.lock().unwrap()[n0..] {
        println!("{}", dziennik::linia(w));
    }
    if let Some(b) = wspolne.blad.lock().unwrap().as_ref() {
        println!("BŁĄD: {b}");
    }
}
