#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::InterruptHandler;

// Bring in the panic handler via defmt-rtt and panic-probe.
use defmt_rtt as _;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

/// Embassy entry point.
///
/// This is a minimal stub that satisfies the Embassy executor entry macro.
/// No peripherals are initialised in this scaffold — actual HID and I/O
/// expander tasks will be added in later steps.
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialise RP2040 peripherals with default clocks.
    let _p = embassy_rp::init(Default::default());

    // Scaffold complete — real tasks will be spawned here in subsequent steps.
    #[allow(clippy::empty_loop)]
    loop {}
}
