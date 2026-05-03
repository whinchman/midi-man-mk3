# Midi-Man Mk3 — Hardware Bill of Materials (BOM)

**Version:** 1.0  
**Date:** 2026-05-02  
**Author:** Architect agent  
**Scope:** MVP control surface — perfboard prototype build  
**Note:** Prices are single-unit USD from Digikey as of research date. Buy 10–20% extra on passives and switches for assembly losses.

---

## 1. Summary Table

| # | Description | Qty | Digikey Part # | Unit Price | Extended | Notes |
|---|---|---|---|---|---|---|
| 1 | Rotary encoder w/ push switch, 24 detents, 15 mm shaft | 18 | PEC11R-4215F-S0024-ND | $2.20 | $39.60 | Bourns PEC11R; 24 PPR; through-hole; PCB pin; push switch on shaft |
| 2 | 3 mm green LED, diffused, 568 nm | 16 | 754-1263-ND (WP7113GD) | $0.21 | $3.36 | Kingbright; Vf 2.2V; If 20 mA max; diffused ideal for step indicators |
| 3 | Tactile switch, 6×6 mm, 100 gf, through-hole (step buttons) | 16 | SW405-ND (B3F-4050) | $0.55 | $8.80 | Omron B3F-4050; standard 6×6 footprint; 100 gf actuation force; square feel for step buttons |
| 4 | Tactile switch, 6×6 mm, 43 gf, through-hole (param buttons) | 12 | CKN9111-ND (PTS645SL43-2 LFS) | $0.24 | $2.88 | C&K PTS645SL43-2 LFS; 43 gf (lighter than step buttons — muscle-memory differentiation) |
| 5 | I2C I/O expander, 16-bit, MCP23017, DIP-28 | 5 | MCP23017-E/SP-ND | $1.69 | $8.45 | Microchip; DIP-28 for hand-soldering; I2C; A0/A1/A2 address pins; internal pull-ups |
| 6 | 8-bit shift register 74HC595, DIP-16 | 2 | 296-1600-5-ND (SN74HC595N) | $1.64 | $3.28 | Texas Instruments; DIP-16; SPI-driven; 2 chained = 16 LED outputs |
| 7 | Raspberry Pi Pico (RP2040, 2 MB flash) | 1 | SC0915-ND | $4.00 | $4.00 | Standard Pico; NOT Pico W or Pico 2; RP2040; micro-USB; 26 GPIO; 3 ADC pins available |
| 8 | LED current-limit resistor, 100 Ω, 1/4 W, axial | 20 | CF14JT100R-ND | $0.10 | $2.00 | Stackpole CF14JT100R; 16 required + 4 spare; sets ~11 mA with 3.3V supply and Vf 2.2V |
| 9 | I2C pull-up resistor, 2.2 kΩ, 1/4 W, axial | 10 | CF14JT2K20-ND | $0.10 | $1.00 | 4 required (SDA+SCL per bus × 2 buses) + 6 spare; 2.2 kΩ suits 400 kHz with ≤100 pF bus cap |
| 10 | Decoupling capacitor, 100 nF, 50V, ceramic, radial | 15 | P4525-ND (K104K10X7RF5UH5) | $0.15 | $2.25 | Vishay X7R; 1 per IC (7 ICs) + Pico VCC bypass + spares; 5 mm lead spacing |
| 11 | Perfboard, single-sided, ~100×160 mm (or two 100×80 mm) | 2 | 1528-1609-ND (Adafruit 1609) | $4.50 | $9.00 | Adafruit Perma-Proto half-sized; labeled breadboard layout eases wire routing; use 2 boards (one per row bank) |
| 12 | Pin header, 40-pin, 1×40, 2.54 mm, breakaway male | 2 | S1011EC-40-ND (PRPC040SAAN-RC) | $0.43 | $0.86 | Sullins; break to length; used for Pico socket rows and inter-board jumpers |
| 13 | Socket header, 40-pin, 1×40, 2.54 mm female | 2 | S7012-ND (PPPC401LFBN-RC) | $1.05 | $2.10 | Optional — socket the Pico so it is removable for reflashing; break to 20-pin lengths |
| **Total** | | | | | **~$87.58** | Before tax/shipping; add ~$8–15 shipping |

---

## 2. Total Estimated Cost

| Category | Cost |
|---|---|
| Active components (encoders, ICs, Pico) | $55.33 |
| Passive components (resistors, caps) | $5.25 |
| Switches and LEDs | $15.04 |
| Prototyping hardware (perfboard, headers) | $11.96 |
| **Subtotal (parts)** | **$87.58** |
| Digikey shipping (standard) | ~$8–15 |
| **Total estimated** | **~$95–105** |

Order slightly more than minimum quantities on passives — Digikey resistors and caps are sold individually or in packs of 5/10. Buy 25 pcs of each passive value (~$2.50 extra) for attrition.

---

## 3. Notes on Component Categories

### 3.1 Rotary Encoders (Qty 18) — Bourns PEC11R-4215F-S0024

The PEC11R series is the industry-standard choice for hand-built MIDI controllers. Key specs for this variant:

- **24 detents per revolution** — standard musical feel; 1 detent per step in a 24-note span. Not too coarse (12 PPR) or too fine (36 PPR) for note selection.
- **15 mm D-shaft** — suits standard 6 mm encoder knob caps (buy separately from any knob supplier; Digikey stocks Davies 1900H-series knobs if desired).
- **Push switch (S suffix)** — shaft-click produces a momentary SPST closure; this is the "tap-to-confirm" mechanic required by requirements.md. No separate switch or PCB mod needed.
- **Through-hole PC pin** — easy hand-soldering; 5-pin footprint (2 switch + 3 encoder).
- **Panel-mount bushing** — 7 mm thread; fits standard panel cutout if a front panel is added later.
- **30,000 cycle rated life** — adequate for a personal instrument used daily.

All 18 are identical (16 step encoders + 1 tempo knob + 1 parameter knob). The two extra knobs connect directly to the RP2040's ADC pins (GPIO26/27), while the 16 step encoders route through the MCP23017 expanders.

**Alternative if 24 detents feels too fine:** Bourns PEC11R-4215F-S0012 (12 PPR, same push switch, same footprint) — Digikey part PEC11R-4215F-S0012-ND at ~$2.20.

### 3.2 LEDs (Qty 16) — Kingbright WP7113GD

- **Green** is the right color for a step sequencer. Green = "step is active/playing" is a universal groove-box convention (Roland, Elektron, etc.). Red would suggest an error/stop state.
- **Diffused lens** (D suffix) gives a wide viewing angle without a hot spot — important when the viewer is not directly overhead.
- **Vf = 2.2V, If = 20 mA max** — driven at ~11 mA from 3.3V GPIO via 100 Ω resistors. This is below max rated current and gives comfortable brightness without washing out in normal lighting.
- **3 mm diameter** — fits a 3 mm panel hole if a front plate is added later. Matches the Synthrotek-style step sequencer aesthetic.

Current-limit calculation: R = (3.3V − 2.2V) / 0.011A = **100 Ω** (standard E12 value).

### 3.3 Step Buttons (Qty 16) — Omron B3F-4050

- **100 gf actuation force** — deliberate, definite click. Appropriate for a step enable/disable button where you want tactile confirmation you have toggled a step.
- **6×6 mm footprint** — standard; many cap styles available.
- **Through-hole** — straightforward perfboard mounting.
- The higher actuation force compared to the parameter buttons creates a muscle-memory distinction between the two banks.

### 3.4 Parameter Buttons (Qty 12) — C&K PTS645SL43-2 LFS

- **43 gf actuation force** — noticeably lighter than the step buttons. The softer feel signals to the player that these are "settings" buttons, not performance buttons.
- Same 6×6 mm footprint as the B3F series — but from a different manufacturer with a different tactile profile.
- The 4×3 grid of 12 buttons maps 1:1 to the 12 parameter functions defined in requirements.md (6 regular + 6 shift).
- PTS645SL43-2 LFS is the in-stock through-hole variant (the PTS645SL43 original is now marked obsolete).

### 3.5 MCP23017 I/O Expanders (Qty 5) — DIP-28

- **DIP-28 package** is the right choice for a hand-soldered perfboard build. SOIC-28 is only 0.3 mm pitch between pins — extremely difficult to hand-solder without hot air or a fine-tip station. DIP-28 has 2.54 mm pitch. Use DIP sockets (add to order: 5× 28-pin DIP socket, ~$0.40 ea) so ICs are removable.
- 5 devices use 5 of the 8 available I2C addresses (see Section 4). No bus conflict risk.
- Internal pull-up resistors on each GPIO pin eliminate the need for external pull-downs on buttons — configure `GPPU` registers in firmware.
- Interrupt-on-change (INT pins) wake the RP2040 without polling, reducing latency for encoder events.

**Recommended addition:** 5× 28-pin DIP socket (e.g., Digikey A120347-ND, ~$0.40 ea = $2.00 total). Not in the main BOM but highly recommended.

### 3.6 74HC595 Shift Registers (Qty 2) — DIP-16

- Two 74HC595 chips chained produce 16 parallel outputs from 3 SPI lines (MOSI, SCLK, RCLK). One chip per 8 LEDs; chain them so the RP2040 shifts 16 bits total.
- **DIP-16** is trivially hand-soldered. Use DIP sockets (add 2× 16-pin DIP sockets, ~$0.20 ea).
- Texas Instruments SN74HC595N is the canonical part; well-documented, widely stocked, no substitution risk.
- Operating voltage: 2V–6V; 3.3V from the Pico is well within spec.
- Output current: 35 mA per pin max; at 11 mA per LED this is fine.

### 3.7 Raspberry Pi Pico — SC0915

- **Standard Pico (not Pico W, not Pico 2 / RP2350).** The architecture plan is written for RP2040. Embassy-rp targets RP2040 explicitly.
- **$4.00** — the cheapest capable microcontroller for this task. 26 GPIO, 3 ADC, 2× I2C, 2× SPI, native USB, 264 KB SRAM, 2 MB flash.
- Comes without headers soldered — solder male headers for socketing, or use castellated pads for direct perfboard attachment.
- USB connection: the Pico uses **micro-USB**. You need a micro-USB to USB-A cable (or adapter) to connect to the host PC. No separate cable line item needed — this is a standard cable.

### 3.8 Passive Components

#### LED Current-Limit Resistors (100 Ω)

```
V_supply = 3.3V
V_f      = 2.2V  (Kingbright WP7113GD green, typical)
I_f      = (3.3 - 2.2) / 100 = 11 mA  (within 20 mA absolute max)
```

One resistor per LED = 16 required. Buy 20 for spares.
Stackpole CF14JT100R, 1/4 W, axial carbon film — Digikey CF14JT100R-ND at $0.10 each.

#### I2C Pull-Up Resistors (2.2 kΩ)

The I2C bus operates at 400 kHz (fast mode). With 5 MCP23017 devices (each ~10 pF input cap) plus trace capacitance, total bus capacitance is estimated at 80–120 pF. Rise time constraint: t_r ≤ 300 ns at 400 kHz.

```
R_max = t_r / (0.8473 × C_bus) = 300 ns / (0.8473 × 100 pF) ≈ 3.5 kΩ
R_min = (V_dd - V_OL) / I_OL = (3.3 - 0.4) / 3 mA ≈ 967 Ω
```

**2.2 kΩ** is the standard safe choice in this window. Pull SDA and SCL to 3.3V with 2.2 kΩ on each bus. Total: 4 resistors (SDA + SCL on I2C0 and I2C1). Buy 10 for margin.

Stackpole CF14JT2K20, 1/4 W — Digikey CF14JT2K20-ND at $0.10 each.

#### Decoupling Capacitors (100 nF)

One 100 nF ceramic capacitor per IC, placed within 5 mm of the VCC pin, bypassed to GND:

| IC | Qty |
|---|---|
| MCP23017 × 5 | 5 |
| 74HC595 × 2 | 2 |
| Near Pico 3.3V rail | 2 |
| Spare | 6 |
| **Total** | **15** |

Vishay K104K10X7RF5UH5 — X7R dielectric, 50V, 5 mm lead pitch — stable over temperature. Digikey P4525-ND (or substitute any X7R 100 nF 50V radial THT). ~$0.15 each.

---

## 4. I2C Address Map for 5× MCP23017

The MCP23017 supports 8 unique I2C addresses (0x20–0x27) via A0/A1/A2 pin strapping. The architecture plan splits devices across I2C0 and I2C1 on the RP2040, so we only need 3 addresses per bus (well within the 8-address limit).

### I2C0 Bus — Encoders + Step Buttons (Left half)

| Device | A2 | A1 | A0 | Address | Assigned Function |
|---|---|---|---|---|---|
| U1 | 0 | 0 | 0 | 0x20 | Encoders 1–8 (GPB = phase A, GPA = phase B) |
| U2 | 0 | 0 | 1 | 0x21 | Encoders 9–16 |
| U3 | 0 | 1 | 0 | 0x22 | Step buttons 1–16 (GPA = steps 1–8, GPB = steps 9–16) |

### I2C1 Bus — Param Buttons + Encoder Push Switches

| Device | A2 | A1 | A0 | Address | Assigned Function |
|---|---|---|---|---|---|
| U4 | 0 | 0 | 0 | 0x20 | Param buttons 1–12 (GPA bits 0–7 = params 1–8; GPB bits 0–3 = params 9–12) |
| U5 | 0 | 0 | 1 | 0x21 | Encoder push-button switches 1–16 (GPA = switches 1–8, GPB = switches 9–16) |

Note: I2C0 and I2C1 are independent buses — address 0x20 on I2C0 and address 0x20 on I2C1 are separate devices and do not conflict.

**Address pins wiring:**

- A0 = 0: wire to GND
- A0 = 1: wire to 3.3V
- Leave unused address bits tied to GND (never float them — floating inputs cause random address collisions)

---

## 5. Wiring Summary

### RP2040 GPIO Assignments

| RP2040 Pin | Function | Notes |
|---|---|---|
| GPIO0 (I2C0 SDA) | I2C0 data | Pull-up to 3.3V via 2.2 kΩ |
| GPIO1 (I2C0 SCL) | I2C0 clock | Pull-up to 3.3V via 2.2 kΩ |
| GPIO2 (I2C1 SDA) | I2C1 data | Pull-up to 3.3V via 2.2 kΩ |
| GPIO3 (I2C1 SCL) | I2C1 clock | Pull-up to 3.3V via 2.2 kΩ |
| GPIO4 (SPI0 MOSI) | 74HC595 data | LED shift register chain |
| GPIO5 (SPI0 SCLK) | 74HC595 clock | LED shift register chain |
| GPIO6 | 74HC595 RCLK (latch) | GPIO toggled after SPI transfer |
| GPIO7 | MCP23017 INT (I2C0) | Shared INT from U1/U2/U3 (open-drain, active-low) |
| GPIO8 | MCP23017 INT (I2C1) | Shared INT from U4/U5 |
| GPIO26 (ADC0) | Tempo knob (encoder A wiper) | Direct ADC — PEC11R wiper to ADC; use as absolute position |
| GPIO27 (ADC1) | Parameter knob (encoder B wiper) | Direct ADC |

**Note on direct ADC knobs:** The two extra knobs (tempo + param) are read as analog potentiometers in the MVP notes, but the chosen PEC11R-4215F-S0024 is an **incremental encoder**, not a potentiometer. For direct ADC reading of an incremental encoder, connect quadrature A/B pins to GPIO26/27 and read as GPIO interrupt (not ADC). Alternatively, if the user prefers a true potentiometer feel for tempo/param, substitute 2× 10 kΩ linear taper potentiometer (e.g., Digikey P3A103-ND, ~$1.50 ea) wired as a voltage divider to 3.3V/GND/ADC. The BOM above uses the same PEC11R encoder for all 18 positions for simplicity and uniformity; the firmware delta-accumulation logic handles them identically. **See flag below.**

### I2C0 Bus — U1, U2, U3

```
RP2040 GPIO0 (SDA) ──┬──[2.2kΩ]── 3.3V
                     ├── U1 pin 13 (SDA)
                     ├── U2 pin 13 (SDA)
                     └── U3 pin 13 (SDA)

RP2040 GPIO1 (SCL) ──┬──[2.2kΩ]── 3.3V
                     ├── U1 pin 12 (SCL)
                     ├── U2 pin 12 (SCL)
                     └── U3 pin 12 (SCL)

INT wiring (open-drain, wire-OR):
U1 INTB ─┐
U2 INTB ─┼──[10kΩ pull-up to 3.3V]── GPIO7
U3 INTB ─┘
```

### I2C1 Bus — U4, U5

```
RP2040 GPIO2 (SDA) ──┬──[2.2kΩ]── 3.3V
                     ├── U4 pin 13 (SDA)
                     └── U5 pin 13 (SDA)

RP2040 GPIO3 (SCL) ──┬──[2.2kΩ]── 3.3V
                     ├── U4 pin 12 (SCL)
                     └── U5 pin 12 (SCL)

U4 INTB ─┐
U5 INTB ─┴──[10kΩ pull-up to 3.3V]── GPIO8
```

Add 2× 10 kΩ resistors (Digikey CF14JT10K0-ND, $0.10 ea) for INT line pull-ups — add these to your passive order but they are minor cost omitted from the main BOM summary.

### 74HC595 LED Driver Chain

```
RP2040 GPIO4 (MOSI) ── U6 (74HC595 #1) pin 14 (SER)
U6 pin 9 (QHPRIME) ── U7 (74HC595 #2) pin 14 (SER)  [cascade]

RP2040 GPIO5 (SCLK)  ── U6 pin 11 (SRCLK) ── U7 pin 11 (SRCLK)
RP2040 GPIO6 (RCLK)  ── U6 pin 12 (RCLK)  ── U7 pin 12 (RCLK)
3.3V ── U6 pin 10 (SRCLR, active-low: tie HIGH) ── U7 pin 10

U6 QA–QH → LEDs 1–8 via 100 Ω resistors to GND
U7 QA–QH → LEDs 9–16 via 100 Ω resistors to GND

U6 pin 13 (OE, active-low) → GND (always enabled)
U7 pin 13 (OE, active-low) → GND
```

### Encoder Wiring (via MCP23017)

Each PEC11R encoder has 5 pins:
- **A (phase A)** and **B (phase B)** to MCP23017 GPIO pins (configured as inputs with internal pull-ups)
- **C (common)** to GND
- **SW** (push switch, pins SW1/SW2) to MCP23017 GPIO (U5) and GND

The MCP23017 internal pull-up (configured via GPPU register) eliminates external pull-up resistors on all 16 encoders and all 28 buttons.

---

## 6. Hand-Soldering Notes and Order

### Tools Required

- Temperature-controlled iron, 330–350°C
- 63/37 or 60/40 rosin-core solder, 0.5–0.8 mm diameter
- Flux pen or flux paste
- Solder wick and desoldering pump
- Multimeter for continuity checks
- Isopropyl alcohol (99%) and brush for flux cleanup

### Recommended Soldering Order

Solder in this order to avoid clearance and access problems:

**Step 1 — DIP sockets first (do not solder ICs yet)**

Place all five 28-pin DIP sockets (MCP23017) and two 16-pin DIP sockets (74HC595) into the perfboard. Solder all socket pins before inserting ICs. Sockets allow IC removal if damaged.

**Step 2 — Passive components (resistors, capacitors)**

Solder all resistors and decoupling capacitors while the board is clear. These lie flat and have the lowest profile — harder to access once tall components are installed. Bend leads to 90° and insert, solder the topside, clip excess leads flush.

**Step 3 — Pico pin headers**

Solder male pin headers to the Pico (if not already done). Mount the Pico via female socket headers on the perfboard so it can be removed for firmware flashing.

**Step 4 — LED driver shift registers (74HC595)**

Insert 74HC595 ICs into their sockets. Verify orientation of notch. These are placed before LEDs to allow wire routing underneath.

**Step 5 — I2C expanders (MCP23017)**

Insert MCP23017 ICs into sockets. Verify all address pin (A0/A1/A2) connections before powering on. Check address wiring against the address map in Section 4.

**Step 6 — LEDs**

Insert all 16 LEDs. Polarity: long lead (anode) toward the 74HC595 output via the 100 Ω current-limit resistor; short lead (cathode) to GND. A consistent bend direction for all 16 LEDs improves visual alignment. Solder one leg per LED, check alignment and height, then solder second leg.

**Step 7 — Step buttons (B3F-4050)**

Mount all 16 step buttons. These are symmetric; polarity does not apply. Ensure flat seating on the board.

**Step 8 — Parameter buttons (PTS645SL43-2)**

Mount the 4×3 grid of 12 parameter buttons.

**Step 9 — Rotary encoders (PEC11R)**

Encoders last — they are the tallest and most awkward components. Align all 18, tack one pin per encoder, confirm alignment and height uniformity, then complete soldering. The bushing/mounting thread can optionally be used to mount through a front panel.

**Step 10 — Wire runs**

Use 28 AWG solid core hookup wire for all inter-component wiring. Run I2C bus as two parallel wires for each bus (SDA, SCL). Keep I2C runs short (< 20 cm) to minimize bus capacitance.

**Step 11 — Power-on test**

Before connecting to the host PC:
1. Check 3.3V rail (RP2040 VSYS → internal 3.3V LDO → 3V3 pin).
2. Check for shorts across VCC/GND on each IC.
3. Power up without USB data; confirm no ICs run hot.
4. Connect USB; run firmware; test each expander address with an I2C scan.

---

## 7. Flags and Recommendations

### Long Lead Times / Stock Risk

- **MCP23017-E/SP (DIP-28)**: historically has had allocation shortages (2021–2023). As of May 2026 it shows in-stock on Digikey ($1.69 ea), but consider buying 6–8 instead of exactly 5 as insurance. If DIP-28 is out of stock, the SOIC-28 variant (MCP23017-E/SO, Digikey MCP23017-E/SO-ND) can substitute but requires careful hand-soldering or a breakout board.
- **PEC11R-4215F-S0024**: Bourns encoders are generally well-stocked. Order 20 instead of 18 (same unit price tier, minimal cost delta: +$4.40).
- All other components are commodity parts with thousands of units in stock.

### Custom PCB vs. Perfboard

At this scale (1 unit, prototype), **perfboard is the right choice**:

| | Perfboard | Custom PCB |
|---|---|---|
| Cost | ~$9 (2 boards) | $50–150 (JLCPCB/PCBWay, min 5 pcs) |
| Lead time | 0 days (order from Digikey) | 7–14 days international shipping |
| Iteration cost | Low (desolder and redo) | High (respin = another $50–150) |
| Risk | Manual wiring errors | CAD errors, DRC misses |
| Result quality | Adequate for personal prototype | Professional-grade, reproducible |

**Recommendation:** Build on perfboard for the MVP. If the layout and functionality are validated and a second unit or revision is desired, commission a 2-layer PCB at that point. At $50 for 5 boards, a PCB break-even requires 5 builds. One prototype does not justify that.

### Design Flag — Tempo and Param Knobs as Encoders vs. Potentiometers

The BOM uses the same PEC11R incremental encoder for all 18 positions. The firmware accumulates deltas for the two "direct GPIO" knobs exactly as it does for the I2C-connected ones. This is the clean approach — all 18 knobs behave identically in software.

The alternative is to use **linear potentiometers** for tempo and param (wired to ADC pins), which provides absolute position readout. For tempo, absolute position is arguably more intuitive (you can see where the knob is physically set). However, it adds a different part type and requires the firmware to handle ADC reading in addition to delta accumulation. **Flag for stakeholder decision** — the BOM can accommodate either; if potentiometers are chosen, add 2× 10 kΩ linear taper pot (e.g., Bourns PTV09A-4020F-B103, Digikey PTV09A-4020F-B103-ND, ~$1.50 ea) and remove 2× PEC11R from the order.

### USB Cable Note

The Raspberry Pi Pico uses **micro-USB**. You need a standard micro-USB to USB-A cable to connect to the host PC. This is not included as a BOM line item — use any cable you already own, or add one for ~$5. No USB hub or special adapter is required for USB HID operation.

### MCP23017 Internal Pull-Ups

The MCP23017 has configurable internal weak pull-ups (~100 kΩ) on each GPIO via the GPPU register. **Enable pull-ups in firmware for all button and encoder input pins.** This eliminates the need for any external pull-down or pull-up resistors on the button/encoder matrix — a significant reduction in passive component count vs. discrete pull-ups.

---

*BOM complete. Recommend placing the Digikey order in a single cart to hit the free-shipping threshold (typically $50+, which this order exceeds). Verify part numbers on Digikey's site before ordering as part numbers and pricing can change.*
