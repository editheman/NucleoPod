# NucleoPod: High-Fidelity Digital Audio Player

## Description

NucleoPod is a standalone, portable digital audio player inspired by classic media players. Built around the STM32U545RE microcontroller and programmed entirely in Rust, the device reads audio files from an external MicroSD card, applies software-based Digital Signal Processing (DSP) for a 3-band equalizer, and outputs high-fidelity audio via the STM32's internal DAC. It features a capacitive touch interface (click-wheel style) for navigation and a color TFT display for a hierarchical user interface (UI).

## Motivation

The motivation behind this project is rooted in a childhood dream. Growing up, I always wanted an original iPod, but I never had the opportunity to own one. Now that the iconic device has been officially discontinued, I decided to fulfill that wish by engineering my own functional tribute from scratch. Beyond this personal fulfillment, the project serves as a comprehensive technical challenge. Recreating the seamless experience of a classic media player allows me to dive deep into the embedded Rust ecosystem (specifically the Embassy async framework), master real-time Digital Signal Processing (DSP), and handle complex hardware synchronization (GPIO, PWM, DAC, SPI, I2C) required for high-fidelity audio playback.

## The Story Behind the Build

I've wanted an MP3 player for as long as I can remember. As a kid, my cousin had one, and I think that's exactly where this whole obsession started. My original plan was to make NucleoPod as compact as possible, but the development board mandated by the faculty forced a bigger form factor than I'd hoped for, so the "pocket iPod" idea turned into something closer to a desk unit. My real goal was to build an iPod that felt like it belonged in 2026, which turned out to be a lot harder than I expected, especially since I'd never actually owned or used a real iPod, so I had to reverse-engineer what I was even supposed to be replicating from photos and videos.

The plan itself was simple on paper: a screen, a headphone jack, an SD card reader (my stand-in upgrade for the iPod's original spinning HDD), a vibration motor, and a capacitive touch wheel, the kind of "haptic click-wheel" mod that people who still use iPods in 2026 either wish for or add themselves. I started by testing every component in isolation to make sure I understood how each one actually behaved, and only then moved on to getting everything working together in one program. I'll admit the entire firmware still lives in a single `main.rs`, this was my first real project in Rust, and it shows; getting there took a lot of trial, error, and borrow-checker fights I'd rather forget. Once the project came alive and I'd iterated on the UI (how things are laid out on screen, how you navigate between menu and player), it was time to move everything off the breadboard and onto a proper perfboard.

<!-- ![perfboard](./images/perfboard_7.jpeg)
![perfboard](./images/perfboard_3.jpeg) -->
<img src="./images/perfboard_7.jpeg" width="48%" /> <img src="./images/perfboard_3.jpeg" width="48%" />

That's where I learned that wanting a "company-prototype-grade" finish almost cost me the deadline. I soldered every component directly onto the perfboard, ran tin traces for the positive and negative rails by hand, and soldered female headers onto the board so the STM32 Nucleo itself could plug straight into the populated board like a shield. When I powered it on, nothing worked properly: components that should have been getting 3.3V or 5V were reading around 0.3V. I couldn't immediately tell whether I had a short somewhere or whether my hand-soldered +/- traces just weren't conductive enough. Either way, the fix meant redoing the power traces, possibly with a solid copper wire bridging the whole run, and I had less than 48 hours left, having already sunk about 5 hours into soldering and another 3 into planning the component layout.

<!-- ![perfboard](./images/perfboard_1.jpeg)
![perfboard](./images/perfboard_2.jpeg) -->
<img src="./images/perfboard_1.jpeg" width="48%" /> <img src="./images/perfboard_2.jpeg" width="48%" />

I couldn't risk showing up to the presentation with a dead board, so I made the call to tear it apart. This was easily the worst part of the whole project: I tried desoldering components off the perfboard first, which didn't work at all (perfboard solder joints don't come off cleanly without proper desoldering tools), so I ended up cutting the board apart with a hobby knife, slicing as close as I could to each component's pins to salvage the parts themselves. In the process I lost two components for good, the haptic touch sensor and the jack breakout board (a DAC module I'd actually bought by mistake, since the STM32U545RE doesn't expose I2S pins on the Nucleo-64; I ended up using it purely as a 3.5mm jack breakout, not for its DAC). My hands didn't come out unscathed either, a lot of small cuts and irritated skin from the blade work, but stopping wasn't really an option after everything I'd already put into it, and it was a project I genuinely wanted to see working.

![perfboard](./images/perfboard_4.jpeg)
![perfboard](./images/perfboard_5.jpeg)
<img src="./images/perfboard_4.jpeg" width="48%" /> <img src="./images/perfboard_5.jpeg" width="48%" />

So I gathered the board and components and put everything back on a breadboard, for the third time. The plan shifted again: instead of the perfboard build (and the 3D-printed case I'd originally designed for it, which no longer fit once I gave up on the perfboard layout), I bought a small mini-breadboard, transplanted every component onto it, and built an enclosure out of a plain plastic box. I drilled the cutouts with a soldering iron tip heated on the stove, spray-painted the box out on the balcony, bolted the STM32 board directly to the enclosure with screws, and routed the whole wiring harness inside. I re-tested continuously through this process to make sure nothing had come loose or gotten miswired along the way. In the end, it worked, smoothly, with no leftover bugs, and I was genuinely excited to demo it to anyone curious enough to ask, classmates and professors alike.

<!-- [▶️ Video](./images/video_functioneaza.mp4) -->

<!-- ![black_box](./images/black_box.jpeg)
![perfboard](./images/perfboard_6.jpeg -->

<img src="./images/black_box.jpeg" width="48%" /> <img src="./images/perfboard_6.jpeg" width="48%" />

Still, there's a small gap I feel about it: the final shape only I ever really saw in my head, the 3D-printed enclosure I'd originally designed never made it onto the finished device. That said, I'm proud of what actually shipped, and I still intend to build the version I originally imagined.

![3D Print](./images/3d_print_1.jpg)
![3D Print](./images/3d_print_2.jpg)

## Architecture

The software is built on the Embassy executor and combines a hardware interrupt, one spawned async task, and a main loop:

* **Audio ISR (TIM6):** Runs on the TIM6 hardware interrupt at the WAV file's sample rate. The interrupt handler reads samples directly from a buffer and writes them to the internal DAC registers via PAC. This provides true hardware-driven audio output independent of CPU scheduling.
* **Main Loop:** Reads audio data from `.wav` files via SPI2 in chunks, using a double-buffering (ping-pong) technique so the ISR always has a ready buffer to fall back on. In the same loop, it polls the MPR121 capacitive touch sensor via I2C1 and the center push-button, detects click-wheel gestures (scroll up/down, select/back), updates the menu/player state machine, and pushes UI updates to the TFT display over SPI1.
* **Haptic Task:** A separate Embassy task, spawned at startup, that triggers the vibration motor via GPIO on each scroll event to provide tactile feedback.

## Block Diagram

* **Power Subsystem:** External 5V Power Bank ➔ STM32 Nucleo USB-C port (`5V` rail).
* **Processing:** STM32U545RE (Nucleo-64), clocked at 160MHz via PLL.
* **Inputs:**
  * MicroSD Card Module ➔ connected via SPI2 (PB13/PB14/PB15, CS on PB5).
  * MPR121 Capacitive Touch Sensor ➔ connected via I2C1 (PB6/PB7).
  * Tactile push-button (center select) ➔ connected to GPIO PB4.
* **Outputs:**
  * Internal 12-bit DAC (DAC1_OUT1 on PA4) ➔ RC filter (100Ω + 100nF) ➔ 3.5mm Audio Jack.
  * TFT LCD Display (ILI9341, 320x240) ➔ connected via SPI1 (PA5/PA6/PA7, DC/RST/CS on PC6/PC7/PC9).
  * Vibration motor module (haptic feedback) ➔ connected to GPIO PB10.

## Log

* **Week Mar 30 - Apr 5:** * Project ideation and initial brainstorming. Evaluated multiple concepts (e.g., smart solar tracker, automated sorter) before settling on the digital audio player (NucleoPod) concept.
* Verified the core capabilities of the STM32U545RE microcontroller to ensure it has the necessary processing power (FPU) and peripherals for audio processing.
* **Week Apr 6 - Apr 12:** * Hardware research and compatibility checks. Investigated the necessary external modules for high-fidelity audio (I2S), storage (SPI), and user input (I2C).
* Made the architectural decision to exclusively use the STM32 Nucleo board, discarding the initially proposed Raspberry Pi Pico to streamline development and focus entirely on the STM32 ecosystem.
* **Week Apr 13 - Apr 19:** * Software architecture planning and scope management. Researched the Rust `embassy-stm32` framework for handling asynchronous tasks and DMA transfers.
* Consulted with the laboratory assistant to refine the project scope.
* **Week Apr 20 - Apr 26:** * Component selection and finalization of the Bill of Materials (BoM).
* Identified specific, compatible breakout boards (PCM5102A DAC, ST7789 TFT display, MPR121 Touch sensor) and designed the theoretical portable power subsystem (Li-Po battery, TP4056 charger, and 5V Boost converter).
* **Week Apr 27 - May 3:** * Drafting the official project documentation and Moodle proposal.
* Initializing the GitHub repository and setting up the basic Rust toolchain for the target architecture (`thumbv8m.main-none-eabihf`). Currently preparing to order the hardware components to begin physical prototyping.
* **Week May 4 - May 10:**
  * Verified Nucleo board power rails and tested each component individually.
  * Successfully integrated and tested the ILI9341 display via SPI1 (with mirror correction via `flip_horizontal`).
  * Validated SD card communication on SPI2 and verified FAT32 file listing.
  * Tested MPR121 touch sensor on I2C1 — confirmed touch detection by reading raw electrode capacitance values.
  * Tested vibration motor on GPIO PB10 for haptic feedback.
  * Validated push-button input on PB4 with internal pull-up.
* **Week May 11 - May 17:**
  * Discovered that SAI/I2S peripheral pins are not exposed on Nucleo-64, blocking the PCM5102A I2S path. Pivoted to using STM32's internal DAC on PA4.
  * Resolved DAC clock configuration by routing it through the LSE oscillator and increasing system clock to 160 MHz via PLL.
  * Built initial WAV playback prototype using a single-task busy-wait loop. Identified audio glitches caused by SD card read latency interleaved with sample output.
  * Implemented double-buffering (ping-pong) between two 8 KB / 16 KB / 32 KB buffers.
  * Tested an Embassy async task split (audio task + SD task) with a `Channel`, but confirmed that Embassy's cooperative scheduling on a single core could not eliminate the interruption while `embedded-sdmmc` performs blocking SPI transfers.
  * Currently implementing TIM6 hardware interrupt for audio output via PAC, decoupling sample timing from the async executor and allowing the main task to handle SD I/O without affecting audio continuity.
* **Week May 18 - May 24:**
  * Finished the TIM6-driven audio output and implemented the wheel-based merge logic, combining click-wheel scroll navigation with playback control, plus a back button to return from the player view to the menu.
  * Explored several parallel implementations on top of that base to compare UI approaches: one adding album cover (BMP) rendering, one adding a playback progress bar, and one combining both.
  * Consolidated the best pieces of these experiments into a single `main.rs`, and added explanatory comments throughout the code for readability.
  * Cleaned up the repository by deleting all the intermediate experimental and test source files (music tests, DAC iterations, merge variants) once their logic had been folded into `main.rs`.
  * Wrote the project documentation (this README) and added the hardware photos and schematic images.
  * Merged the `development` and `cleanup-tests` branches into `main` via pull requests, consolidating all of the above work.

## Hardware Overview

The core of the system is the **STM32 Nucleo-64 (STM32U545RE)**, chosen for its ARM Cortex-M33 core with hardware FPU, built-in 12-bit DAC, and good Embassy support. The system clock is configured at 160 MHz using HSI + PLL to provide enough processing headroom for parallel SD reads and audio output. The hardware design is modular: each peripheral (display, SD, touch, motor) is on its own breakout board, connected via standard SPI/I2C/GPIO buses.

### Schematics

![Schematic](./images/edi_schematic.webp)

## Components

* **STM32U545RE Nucleo-64:** Main microcontroller and development board.
* **Internal 12-bit DAC (PA4):** Generates analog audio signal, AC-coupled via 100Ω + 100nF to the headphone jack.
* **PCM5102A Module (jack reuse only):** Provides the 3.5mm headphone jack and AGND reference; its I2S DAC chip is unused.
* **MicroSD SPI Module:** Mass storage for `.wav` audio files (FAT32 formatted, max 32 GB SDHC).
* **2.4" Color TFT LCD (ILI9341, 320x240):** Displays the menu UI, track list, and playback status.
* **MPR121 Touch Sensor:** Reads capacitive touch on copper-tape electrodes arranged in a circle, simulating a click-wheel.
* **Tactile Push-Button (6x6mm):** Center "Select / Play-Pause" button.
* **Vibration Motor Module:** Provides haptic feedback on scroll events.
* **5V USB Power Bank:** Portable power source.

![Wireing](./images/project_on_breadboard.webp)

## Bill of Materials (Hardware)

| Device | Usage | Price |
|--------|-------|-------|

| [STM32 Nucleo U545RE-Q](https://eu.mouser.com/ProductDetail/STMicroelectronics/NUCLEO-U545RE-Q?qs=mELouGlnn3cp3Tn45zRmFA%3D%3D) | The main microcontroller running the NucleoPod firmware | 110.00 RON |
| [3.5 mm Stereo Audio Jack Module](https://www.optimusdigital.ro/en/connectors/752-modul-jack-audio-stereo-de-35-mm.html?search_query=jack&results=193) | Used to provide high-quality signal via 3.5mm jack | 2.10 RON |
| [MicroSD Card Reader Module](https://ardushop.ro/ro/module/1553-groundstudio-microsd-module-6427854023056.html) | SPI module used to read the `.WAV` audio files and `.BMP` album covers | 8.00 RON |
| [MicroSD Card](https://www.emag.ro/search/microsd+16gb) | Formatted to FAT32 to store the uncompressed music and images | 20.00 RON |
| [2.4" TFT LCD Display (ILI9341)](https://ardushop.ro/ro/electronica/1348-modul-lcd-24-cu-spi-controller-ili9341-6427854019523.html) | Used as the graphical user interface for the MP3 Player | 67.00 RON |
| [MPR121 Capacitive Touch Module](https://ardushop.ro/ro/senzori/984-modul-senzor-capacitiv-mpr121-6427854013279.html) | Used to read the 8 copper pads that make up the Haptic Touch Wheel | 10.00 RON |
| [Tactile push-button 6x6mm](https://ardushop.ro/ro/butoane--switch-uri/713-buton-mic-push-button-trough-hole-6427854009050.html) | Used as the physical center Select / Play / Pause button | 0.50 RON |
| [Vibration Motor Module](https://www.emag.ro/comutator-de-vibratii-pwm-pentru-motor-modul-senzor-motor-pentru-jucarii-motor-dc-vibrator-pentru-telefon-mobil-pentru-kitul-diy-arduino-uno-mega2560-r3-741050524275/pd/D69LHM2BM/?ref=history-shopping_487417681_245879_1) | Used to provide physical haptic feedback (clicks) when scrolling the touch wheel | 29.00 RON |
| [Resistors 100Ω](https://www.optimusdigital.ro/en/search?s=100+ohm+resistor) x2 | Used alongside the capacitors for the DAC output audio coupling | 0.10 RON |
| [Capacitors 10nF](https://www.optimusdigital.ro/en/search?s=100nf+capacitor) x2 | Used alongside the resistors for the DAC output audio coupling | 0.20 RON |
| [Breadboard 830 tie-points](https://sigmanortec.ro/Breadboard-830-puncte-p125425574) | Used as the base to safely prototype and wire all electronic components | 12.00 RON |
| [Dupont Jumper Wires](https://ardushop.ro/ro/fire-si-conectori/8-10-x-fire-dupont-mama-tata-20cm-6427854039200.html) | Assorted wires (M-M, M-F) used to connect the modules to the Nucleo board | 10.00 RON |

## Software

| Library | Description | Usage |
|---------|-------------|-------|

| [embassy-stm32](https://github.com/embassy-rs/embassy) | Async HAL for STM32 microcontrollers | Provides hardware abstraction for peripherals: DAC (audio playback), SPI (Display & SD card), I2C (Touch wheel), GPIO, and Timers. |
| [embassy-executor](https://github.com/embassy-rs/embassy) | Async executor for embedded | Manages and schedules concurrent asynchronous tasks (e.g., separating the haptic vibration motor task from the main audio playback loop). |
| [embassy-time](https://github.com/embassy-rs/embassy) | Time and delay primitives | Provides precise timing and delays required for display initialization, button debouncing, and haptic feedback duration. |
| [cortex-m](https://github.com/rust-embedded/cortex-m) | ARM Cortex-M core crates | Used for low-level core processor operations, such as unmasking the NVIC interrupts for the audio hardware timer. |
| [defmt-rtt](https://github.com/knurling-rs/defmt) / [panic-probe](https://github.com/knurling-rs/panic-probe) | Debugging & Panic handler | Catches fatal system errors (panics) and safely logs them to the host console via the RTT (Real-Time Transfer) debug interface. |
| [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics) | 2D graphics library | Draws the entire graphical user interface: anti-flicker text rendering, control buttons (rectangles), and the progress bar (circles and lines). |
| [tinybmp](https://github.com/embedded-graphics/tinybmp) | BMP image parser | Performs bit-level decoding of uncompressed `bgr24` images (Album Covers) so they can be rendered on the display. |
| [mipidsi](https://github.com/almindor/mipidsi) | Display driver for MIPI DSI / SPI | Initializes and controls the ILI9341 TFT display, configuring screen orientation, pixel color formats, and SPI communication. |
| [embedded-hal-bus](https://github.com/rust-embedded/embedded-hal) | SPI Bus sharing | Creates an `ExclusiveDevice` for the display, ensuring proper management of the Chip Select (CS) pin on the shared SPI bus. |
| [embedded-sdmmc](https://github.com/rust-embedded-community/embedded-sdmmc) | SD/MMC file system | Enables navigation of the FAT32 file system on the SD card, directory iteration (handling 8.3 short names), and efficient byte-level reading for WAV and BMP files. |
| [heapless](https://github.com/japaric/heapless) | Data structures without dynamic allocation | Manages fixed-capacity vectors for the playlist and string formatting (such as playback time tracking) directly on the stack, safeguarding the limited RAM. |

![Software](./images/display_photo.webp)

## Links

1. [https://embedded-rust-101.wyliodrin.com/docs/acs_cc/category/lab](https://embedded-rust-101.wyliodrin.com/docs/acs_cc/category/lab)
2. [wav files downloader](https://www.cutyt.com/yt-wav)
3. [downloader for album cover](https://spotidownloader.com/en19)
4. [https://github.com/MYaqoobEmbedded/STM32-Tutorials/tree/master/Tutorial%2043%20-%20WAV%20Player](https://github.com/MYaqoobEmbedded/STM32-Tutorials/tree/master/Tutorial%2043%20-%20WAV%20Player)
5. [https://github.com/MabezDev/embedded-fatfs](https://github.com/MabezDev/embedded-fatfs)
6. [online wav conversion tool](https://g711.org/)
7. [ffmpeg - tool for transforming wav file from 16-bit to 8-bit](https://ffmpeg.org/ffmpeg.html)
8. [perfboard soldering](https://www.youtube.com/watch?v=5tydtZl95dE)
9. [DMA for SD with a 12-bit DAC logic](https://www.youtube.com/watch?v=fY4CHt99SuY)
10. [wav player exemple](https://www.youtube.com/watch?v=QPmFvSFyIbs&t=1301s)
11. [audio player exemple](https://www.youtube.com/watch?v=Eki52Y2Ou5s&t=931s)
