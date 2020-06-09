#![no_main]
#![no_std]

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};

use rtfm::app;

use nrf52833_hal as hal;
use crate::hal::target as pac;

use hal::{clocks, gpio, timer::Instance, uarte};

use bbqueue::{self, BBBuffer, ConstBBBuffer};

use nrf52_radio_802154::radio::{Radio, MAX_PACKET_LENGHT};

// Use a packet buffer that can hold 16 packages
pub(crate) use bbqueue::consts::U2048 as PacketBufferSize;

static PKT_BUFFER: BBBuffer<PacketBufferSize> = BBBuffer(ConstBBBuffer::new());

#[app(device = crate::hal::target, peripherals = true)]
const APP: () = {
    struct Resources {
        uart: uarte::Uarte<pac::UARTE0>,
        radio: Radio,
        rx_producer: bbqueue::Producer<'static, PacketBufferSize>,
        rx_consumer: bbqueue::Consumer<'static, PacketBufferSize>,
        timer: pac::TIMER0,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        // Configure to use external clocks, and start them
        let _clocks = clocks::Clocks::new(cx.device.CLOCK)
            .enable_ext_hfosc()
            .set_lfclk_src_external(clocks::LfOscConfiguration::NoExternalNoBypass)
            .start_lfclk();

        rtt_init_print!();

        rprintln!("Initialize");

        cx.device.TIMER0.set_periodic();
        cx.device.TIMER0.enable_interrupt();
        cx.device.TIMER0.timer_start(1_000_000u32);

        let port0 = gpio::p0::Parts::new(cx.device.P0);
        let uart = uarte::Uarte::new(
            cx.device.UARTE0,
            uarte::Pins {
                txd: port0
                    .p0_06
                    .into_push_pull_output(gpio::Level::High)
                    .degrade(),
                rxd: port0.p0_08.into_floating_input().degrade(),
                cts: Some(port0.p0_07.into_floating_input().degrade()),
                rts: Some(
                    port0
                        .p0_05
                        .into_push_pull_output(gpio::Level::High)
                        .degrade(),
                ),
            },
            uarte::Parity::EXCLUDED,
            uarte::Baudrate::BAUD115200,
        );
        let (q_producer, q_consumer) = PKT_BUFFER.try_split().unwrap();

        let mut radio = Radio::new(cx.device.RADIO);
        radio.set_channel(15);
        radio.set_transmission_power(8);
        radio.receive_prepare();

        init::LateResources {
            uart, radio,
            rx_producer: q_producer,
            rx_consumer: q_consumer,
            timer: cx.device.TIMER0,
        }
    }

    #[task(binds = RADIO, resources = [radio, rx_producer])]
    fn radio(cx: radio::Context) {
        let radio = cx.resources.radio;
        let queue = cx.resources.rx_producer;

        match queue.grant_exact(MAX_PACKET_LENGHT) {
            Ok(mut grant) => {
                if grant.buf().len() < MAX_PACKET_LENGHT {
                    rprintln!("No room in the buffer");
                    grant.commit(0);
                } else {
                    let packet_len = radio.receive_slice(grant.buf());
                    grant.commit(packet_len);
                }
            }
            Err(_) => {
                // Drop package
                let mut buffer = [0u8; MAX_PACKET_LENGHT];
                radio.receive(&mut buffer);
                rprintln!("Failed to queue packet");
            }
        }
    }

    #[task(binds = TIMER0, resources = [timer])]
    fn timer(cx: timer::Context) {
        cx.resources.timer.timer_reset_event();
    }

    #[idle(resources = [rx_consumer, uart])]
    fn idle(cx: idle::Context) -> ! {
        let mut host_packet = [0u8; MAX_PACKET_LENGHT * 2];
        let queue = cx.resources.rx_consumer;
        let uart = cx.resources.uart;

        loop {
            if let Ok(grant) = queue.read() {
                let packet_length = grant[0] as usize;
                match esercom::com_encode(
                    esercom::MessageType::RadioReceive,
                    &grant[1..packet_length],
                    &mut host_packet,
                ) {
                    Ok(written) => {
                        let _ = uart.write(&host_packet[..written]);
                    }
                    Err(_) => {
                        rprintln!("Failed to encode packet");
                    }
                }
                grant.release(packet_length);
            }
        }
    }
};
