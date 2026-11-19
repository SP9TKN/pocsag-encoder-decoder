# Koder i dekoder POCSAG (koder_dekoder_pogsag)

> [!IMPORTANT]
> **🇵🇱 Program jest w języku polskim.** 🇬🇧 English version: [README.md](README.md)

Nadaje i odbiera wiadomości pagerowe POCSAG 512/1200/2400 bit/s przez kartę dźwiękową i radio.
Okno ma konsolę do wpisywania wiadomości i historię nadanych oraz odebranych poniżej.
Historia dnia zapisuje się też w pliku tekstowym obok programu.

Silnik odbioru to kopia dekodera z programu `decoder_scrambler_pogsag` (projekt *quansheng-descrambler*)
z poprawką odporności na zgubione próbki (niżej).

## Okno

- **Ustawienia** (zwijane): wejście karty do odbioru (albo „bez nasłuchu”), wyjście do nadawania,
  PTT, port COM, opóźnienie PTT, ogon, poziom wyjścia, prędkość, polaryzacja, powtórzenia,
  „Ton próbny 3 s” (sama preambuła 1010... z PTT, do ustawienia poziomu i dewiacji).
- **Mój pager** (w ustawieniach): własny RIC i RIC grupowy programu, każdy z włącznikiem. Odebrane
  wiadomości na te numery są podświetlone w historii: na żółto „do mnie”, na zielono „do mojej grupy”
  (jak w pagerze liczy się tylko RIC, funkcja nie). Obok „Historia” są liczniki, a przy nowej
  wiadomości do programu mruga pasek zadań. „Dopisuj jako nadawcę” wstawia własny RIC na początek
  każdej wysłanej wiadomości: `(1234567) treść` (POCSAG nie przesyła nadawcy). W historii taki numer
  trafia do kolumny „Nadawca” (po lewej od „Odbiorcy”; nazwa z listy, jeśli RIC jest na liście),
  a „Treść” pokazuje resztę. Rozpoznawane są dwie postacie na samym początku: `(101) treść` (ta
  programu) i `101: treść` (spotykana w innych nadajnikach); numer to 1-7 cyfr, a „15:00 ...” nie
  jest brane za numer, bo po dwukropku stoi cyfra.
- **Start** otwiera kartę i port PTT. Port jest trzymany do Zatrzymaj, bo każde otwarcie portu
  może na chwilę podnieść linie.
- **Radio** (wiersz nad konsolą): przestrajanie radia przez CAT. „Lista” to częstotliwości z
  `czestotliwosci.txt`, „Ostatnie” to 8 ostatnio ustawionych (w ustawieniach). Można też wpisać
  MHz („435,000” albo „435.000”), Enter albo „Ustaw”. Obok pojawia się wynik: zielony, gdy radio
  potwierdziło częstotliwość (Icom odpowiada FB, a u Yaesu po zapisie jest odczyt `FA;`); czerwony,
  gdy radio odrzuciło częstotliwość albo nie odpowiada. **Częstotliwości...** otwiera listę
  (nazwa, MHz, Ustaw, ▲, Usuń, „Dodaj” z pola MHz). Lista zapisuje się sama, a w Notatniku ma postać
  `nazwa | MHz`. Program nie przestraja radia sam, także przy starcie: tylko po wyborze albo wpisie.
- **Sterowanie radiem** (w ustawieniach): wyłączone / Icom CI-V / Yaesu CAT (FA), port CAT, prędkość,
  adres CI-V. Port CAT jest otwierany tylko na chwilę zmiany, więc HRD czy JTDX mogą go używać
  między zmianami. Gdy działa silnik, zmiana czeka w kolejce za nadaniem. Gdy port CAT jest ten sam
  co PTT (IC-7100: COM6), zmiana idzie przez port, który silnik już trzyma. FT-991A: CAT tylko przez
  port „Enhanced” (COM4) z RTS w górze; port „Standard” (COM5) program odrzuca, bo RTS na nim nadaje.
- **Konsola**: odbiorca z listy, RIC, funkcja 0-3 (A-D), typ (tekst, cyfry, sam sygnał) i linia
  tekstu. Enter wysyła, strzałki góra/dół przywołują poprzednie wiadomości. Pod linią: licznik
  znaków, przewidywany czas nadania i postać po zamianie polskich liter (ą -> a itd.; pagery znają ASCII).
- **Odbiorcy...**: lista pagerów (nazwa, RIC, funkcja, typ, prędkość) w `odbiorcy.txt` obok programu,
  zapisywana sama; da się ją też poprawiać w Notatniku (`nazwa | RIC | funkcja | typ | prędkość`).
  Wybór odbiorcy w konsoli albo „Użyj” ustawia RIC, funkcję, typ i prędkość jego pagera. Pole
  „Odbiorca” pokazuje nazwę, gdy RIC i funkcja w konsoli zgadzają się z kimś z listy.
- **Historia**: tabela jak arkusz (białe tło, siatka): godzina, kierunek, odbiorca (nazwa z listy po
  RIC), treść, RIC, funkcja, bit/s, stan; nowe na górze, szerokość kolumn do przeciągania. Kolumny
  przesuwa się, przeciągając nagłówek na inny (niebieska kreska pokazuje miejsce); kolejność zapisuje
  się w ustawieniach, prawy przycisk na nagłówku przywraca domyślną. Najechanie
  na wiersz pokazuje wszystko (drugi odczyt, jakość albo wynik nadania), kliknięcie wpisuje jego RIC
  i funkcję do konsoli. Prawy przycisk na wierszu: „Kopiuj wiersz” (kolumny z ekranu, rozdzielone
  tabulatorem — wklejone w arkuszu trafiają w osobne komórki), „Kopiuj treść”, „Kopiuj RIC”. „Kopiuj
  tabelę” kopiuje nagłówek i całą historię naraz, w tej samej kolejności kolumn co na ekranie.
  „Dekoduj plik...” szuka POCSAG w nagraniu (WAV, MP3, FLAC, OGG). „Plik dnia”
  otwiera plik w Notatniku. „Wyczyść okno” czyści tylko okno.

Kolejne wiadomości o tych samych ustawieniach idą jednym nadaniem (jedna preambuła, do 8).
„Przerwij” puszcza PTT i czyści kolejkę. Odbiór własnej wiadomości (odsłuch przez radio albo kabel)
dostaje dopisek „własne nadanie (odsłuch)”.

## PTT

| Tryb | Co robi |
|---|---|
| Brak (VOX) | tylko dźwięk |
| COM: linia DTR / RTS | linia w górę na czas nadania; „odwrócona” dla kluczowania stanem niskim |
| COM: linie DTR+RTS | obie linie naraz (interfejs kluczowany jedną albo drugą); „odwrócona” działa na obie |
| CAT Icom (CI-V) | `FE FE <adres> E0 1C 00 01 FD` / `... 00 FD`; IC-7100 = 88h |
| CAT Yaesu | `TX1;` / `TX0;` (FT-991A, FTDX10) |

PTT puszcza zawsze: po danych i ogonie, po „Przerwij”, po błędzie karty, po limicie czasu
(długość nadania + 3 s) i przy zamknięciu programu. Przy otwarciu portu obie linie idą w spoczynek.

## Plik dnia

`historia_POCSAG_RRRR-MM-DD.txt` obok exe, UTF-8, CRLF:

    godzina | kierunek | RIC | funkcja | prędkość | szczegóły | drugi odczyt | treść

Treść jest ostatnia, a znak `|` w polach tekstowych zamieniany na `¦`, więc plik wraca do okna
po ponownym uruchomieniu (historia dnia, sortowana po godzinie). Wpis nadania trafia do pliku
po jego końcu, z wynikiem (nadane / PRZERWANE / BŁĄD). Wiadomości z „Dekoduj plik...” nie trafiają
do pliku dnia, bo to stare nagrania. Ustawienia: `%APPDATA%\koder-dekoder-pogsag.txt`.

## Koder

Preambuła 576 bitów, paczki sync + 8 ramek, adres w ramce `RIC & 7`, treść przez kolejne paczki.
Tekst: znaki 7-bitowe, najmłodszy bit pierwszy, EOT na końcu. Cyfry: 4 bity, zestaw
`0-9 * U spacja - ) (` (też `]` i `[`), wypełnienie spacjami. NRZ z zaokrąglonymi zboczami
(połówka kosinusa na 0,4 bitu), bit 1 = ujemne napięcie (niższa częstotliwość); „odwrócona”
zamienia polaryzację, gdy pager nie reaguje.

Najlepiej nadawać przez płaskie wejście danych radia (DATA/9600). Wejście mikrofonowe ma filtr
górnoprzepustowy i preemfazę, a 512 bit/s ma długie ciągi tych samych bitów; małe RIC (np. 9, 200)
mają ich najwięcej w słowie adresu.

## Odporność na zgubione próbki (ta sama poprawka jest w starym dekoderze)

Karta albo kabel wirtualny pod obciążeniem gubi czasem cały pakiet (10 ms). Reszta nadania się
przesuwa, a stary silnik przedłużał łańcuch paczek na przesuniętą pozycję. Przesunięte słowa idle
wyglądają tam jak adresy, więc powstawała seria fałszywych RIC co ramkę. Test: 10 ms wycinane po kolei
w różnych miejscach nadania. Przy 512 bit/s było 206 fałszywych RIC na 81 prób, teraz 0
(od korektora ze sprzężeniem). Przy 1200 bit/s było 91 na 31, teraz 0. Reguły:

1. łańcuch paczek kończy się na innej paczce, także przesuniętej o kilka bitów, i gdy dobry sync
   jest do 32 bitów obok słabego;
2. paczki „nachodzą” dopiero bliżej niż 544 - 32 bity: paczka za przeskokiem zaczyna się kilka bitów
   wcześniej i nie może przepaść razem ze swoimi wiadomościami;
3. słowo bliskie (<= 3 bity) idle przesuniętemu o 1-31 bitów (albo dosuniętemu zerami/jedynkami do
   16 bitów) liczy się jako idle przesunięte; wpada w to 78 z 2^21 słów kodowych;
4. gdy następna paczka leży 1-32 bity obok miejsca co 544, w bieżącej od pierwszego niepewnego słowa
   reszta to śmieci (nie otwiera adresów);
5. adres, za którym idzie idle przesunięte, to koniec wiadomości (styk przeskoku w ostatniej paczce).

Resztę fałszywych daje zegar, który wygładza skok fazy także wstecz, więc słowo tuż przed przeskokiem
bywa odczytane o bit obok (kod BCH jest cykliczny: przesunięty adres bywa poprawnym słowem).
Okno pokazuje, ile razy wejście zgubiło próbki (znaczniki czasu pakietów karty).

Nie sprawdzone na prawdziwych nagraniach pagera: folder z nimi został usunięty przed tą zmianą.

## Dekoder: korektor ze sprzężeniem i sito na szum (05.11.2026)

Tor audio radia (filtr górnoprzepustowy ok. 250 Hz, deemfaza) zostawia z długiego ciągu tych samych
bitów tylko zbocza. Najdłuższy taki ciąg jest zwykle w ostatnim słowie tekstu, dopełnionym zerami
po znaku końca. Przy szumie miało ono średnio 5-6 złych bitów, a pozostałe słowa poniżej 1, więc tekst
kończył się śmieciowymi znakami. Korektor dostał sprzężenie od własnych decyzji (poprzedni bit i
zanikający ślad dawniejszych). Wagi uczy się sam, metodą najmniejszych kwadratów, jak resztę toru.
Teraz wszystkie słowa mają średnio 0,0 złych bitów.

Po końcu nadania radio oddaje szum (otwarta blokada szumów albo jej ogon). Dekoder doklejał do łańcucha
paczek kolejne paczki z szumu i robił z nich fałszywe adresy. Teraz paczka bez dobrego syncu musi mieć
co najmniej 4 pewne słowa. Do tego sito odrzuca wiadomość, w której ponad połowa słów treści jest nie do
poprawienia, oraz adres poprawiony na 2 bitach bez potwierdzającej, czystej treści.

Pomiar: tor radia (4 × górnoprzepustowy 250 Hz) + szum, 30 prób, tekst „101: TAKOTAK JAKO”:

| szum / sygnał | 512 bit/s przed | po | 1200 bit/s przed | po |
|---|---|---|---|---|
| 0,30 | 17 | 30 | 10 | 29 |
| 0,45 | 8 | 25 | 9 | 28 |
| 0,60 | 2 | 16 | 3 | 24 |

Fałszywe RIC w tym samym pomiarze: przed 12-35 na 30 prób, po 0 (1-5 dopiero przy szumie
równym sygnałowi). Szum po nadaniu (2 s): przed 3-12 fałszywych na 40 prób, po 0 na 800.
Przechodzą też dawne ograniczenia: deemfaza (także 2400 bit/s) i małe RIC przez filtr radia.
Okno 8 s dekoduje się w 60-80 ms, jak wcześniej.

Nie sprawdzone na prawdziwych nagraniach pagera (folder z nimi skasowany); na żywo przez kabel
wirtualny 8/8 bez poprawek. Ten sam silnik jest w `decoder_scrambler_pogsag`.

## Budowanie i testy

Toolchain `x86_64-pc-windows-gnu`, w `PATH` musi być `C:\msys64\mingw64\bin` (dlltool, windres):

    cargo test --release --lib
    cargo build --release --bins

- `cargo test`: koder (słowa kodowe, ramki, polskie znaki), koder -> dekoder na torze płaskim
  (zero złych bitów, wszystkie prędkości, obie polaryzacje, 44,1 i 48 kHz) i torze radia,
  przeskok 10 ms bez fałszywych adresów, plik dnia tam i z powrotem, testy dekodera.
- `examples/petla.rs <wyjście> <wejście> <port PTT> <port podglądu> <dtr|rts|dtr_rts|icom|yaesu>`: test
  na żywo całego toru przez kabel wirtualny i parę com0com (stan PTT z drugiego końca pary).
  Sprawdzone 03.11.2026: CABLE-B, COM20/COM21, wszystkie 4 tryby PTT, 16/16 wiadomości bez poprawek.
- `examples/czest.rs <icom|yaesu> <port> <bit/s> <MHz> [adres CI-V]`: ustawienie częstotliwości tą
  samą funkcją co program. Sprawdzone 15.11.2026 na FT-991A (COM4, 38400 bd): przestrojenie i powrót
  w 275 ms; zła prędkość, 1200 MHz (radio zostaje) i port Standard dają czytelne komunikaty.
- `examples/czest_silnik.rs <wyjście> <port> <MHz>...`: częstotliwość przez działający silnik (PTT CAT
  Icom i CAT na jednym porcie). Sprawdzone 15.11.2026 na COM20/COM21 z udawanym IC-7100 (echo + FB/FA).
- `examples/dekoduj.rs <plik>...`: POCSAG z plików.
- `examples/czas_okna.rs`: czas dekodowania okna 8 s dla każdej prędkości.
- Raporty (`cargo test --release --lib raport -- --ignored --nocapture`): czułość przy rosnącym
  szumie, szum po nadaniu, zgubione próbki.
- `POGSAG_ZRZUT=plik.wav`: zapisuje wszystko z wejścia odbioru (diagnostyka).
- Ikona: `assets/ikona.py` (PIL), pager w stylu ikony dekodera.

## Licencja

[GPL-3.0](LICENSE) © 2026 SP9TKN.

**Narzędzia:** program powstał z pomocą [Claude Code](https://claude.com/claude-code) (Anthropic) — asystent AI do pisania kodu, testów i dokumentacji.
