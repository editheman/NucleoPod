# Proiect PM (Proiectare cu Microprocesoare) - 2026


**Autor:** Eduard Pană  
**Universitatea Politehnica din București (UPB) - Facultatea de Automatică și Calculatoare (ACS)** **Anul:** 3

---

## Descrierea Proiectului
Acest repository conține implementarea proiectului final la materia **Proiectare cu Microprocesoare (PM)**. Proiectul constă într-un sistem embedded complex, dezvoltat în **Rust**, folosind framework-ul asincron **Embassy**. 

Sistemul este un player audio și un controller interactiv hardware, care integrează o varietate de componente externe, comunicând prin diverse magistrale și periferice (I2C, SPI, ADC, GPIO).

## Funcționalități Principale
- **Redare Audio de Înaltă Calitate:** Ieșire de sunet prin mufă Jack de 3.5mm folosind un DAC, cu suport pentru decodare audio High-Definition.
- **Interfață Vizuală (Display):** Afișare de imagini dinamice (cover photos pentru melodii) și bară de progres pentru redarea curentă.
- **Control Intuitiv:**
  - **Rotary Encoder (Rotiță):** Folosit pentru navigare rapidă prin meniuri sau piese.
  - **Butoane Fizice:** Buton dedicat de *Back* pentru navigare în ierarhia meniului.
  - **Senzor Capacitiv:** Interacțiune touch experimentală prin fire libere pentru declanșarea acțiunilor.
- **Feedback Haptic:** Motor de vibrații care se activează la interacțiunea cu meniul pentru o experiență tactilă îmbunătățită.
- **Stocare Externă:** Citirea resurselor (audio, imagini) direct de pe un card SD.

## Componente Hardware Folosite
1. Placă de dezvoltare (Microcontroller compatibil cu ecosistemul Rust/Embassy)
2. Modul Display (OLED/TFT)
3. Cititor Card SD (interfață SPI)
4. Modul DAC (Digital-to-Analog Converter) cu Mufă Jack 3.5mm
5. Rotary Encoder (cu buton de apăsare integrat)
6. Senzor Capacitiv (Touch)
7. Motor de vibrații (Haptic feedback)

##  Design Hardware și Software

### Arhitectura Hardware
Sistemul este centralizat pe microcontroller, care acționează ca un orchestrator. Acesta citește input-urile utilizatorului de la encoder, butoane și senzorul capacitiv prin pini GPIO și ADC. Ulterior, preia datele multimedia de pe cardul SD (via SPI) și le trimite sincronizat către display și modulul DAC pentru redare.

![Schematic](/images/edi_schematic.webp)
![Display](/images/display_photo.webp)
![Fire](/images/project_on_breadboard.webp)

## Structura Proiectului
Dezvoltarea s-a realizat iterativ pe parcursul a două luni:
1. **Faza de Testare:** S-au creat scripturi individuale pentru a testa pe rând fiecare modul hardware (ex. `sd_test.rs`, `display_test.rs`, `capacity_senzor_test.rs`).
2. **Faza de Integrare Audio:** Multiple iterații pentru stabilizarea redării muzicii prin DAC (`music_test_1` -> `the_best_audio`).
3. **Integrarea Finală:** Unificarea logicii de control (rotiță, butoane, interfață grafică) cu player-ul audio.

Ramura `main` reprezintă versiunea stabilă (production-ready), conținând logica finală consolidată în `main.rs`, menținută curată și fără fișierele experimentale intermediare.

## Compilare și Rulare
Pentru a rula acest cod pe microcontroller, asigură-te că ai instalat [Rust](https://rustup.rs/) și utilitarul `probe-rs` (sau echivalentul pentru placa ta).

### 1. Cloneaza repozitory-ul
```bash
git clone git@github.com:UPB-PMRust-Students/acs-project-2026-editheman.git
```

### 2. Navighează în directorul sursă
```bash
cd acs-project-2026-editheman/embassy
```

### 3. Rulează proiectul pe placă
```bash
cargo run --release
```