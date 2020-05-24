#![no_main]
#![no_std]

use core::fmt::Write;

#[allow(unused_imports)]
use panic_itm;

use cortex_m::{iprintln, peripheral::ITM};

use rtfm::app;

use embedded_hal::digital::v2::{InputPin, OutputPin};

use crate::hal::target as pac;
use nrf52833_hal as hal;

use hal::{clocks, gpio, timer::Instance, uarte};
use pac::{RTC0, RTC1, TIMER0, TIMER1, UARTE0};

#[app(device = crate::hal::target, peripherals = true)]
const APP: () = {
    struct Resources {
        button_1: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_2: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_3: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_4: gpio::Pin<gpio::Input<gpio::PullUp>>,
        itm: ITM,
        led_1: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_2: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_3: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_4: gpio::Pin<gpio::Output<gpio::PushPull>>,
        #[init(false)]
        on_off: bool,
        rtc_0: hal::rtc::Rtc<RTC0, hal::rtc::Started>,
        rtc_1: hal::rtc::Rtc<RTC1, hal::rtc::Started>,
        #[init(0)]
        rtc_1_last: u32,
        timer_0: TIMER0,
        timer_1: TIMER1,
        #[init(0)]
        timer_1_last: u32,
        uart: uarte::Uarte<UARTE0>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        // Configure to use external clocks, and start them
        let _clocks = clocks::Clocks::new(cx.device.CLOCK)
            .set_lfclk_src_external(clocks::LfOscConfiguration::NoExternalNoBypass)
            .start_lfclk();

        cx.device.TIMER0.set_periodic();
        cx.device.TIMER0.enable_interrupt();
        cx.device.TIMER0.timer_start(1_000_000u32);

        cx.device.TIMER1.set_periodic();
        cx.device.TIMER1.timer_start(30_000_000u32);

        let mut rtc_0 = hal::rtc::Rtc::new(cx.device.RTC0);
        let _ = rtc_0.set_prescaler(4095);
        rtc_0.enable_event(hal::rtc::RtcInterrupt::Tick);
        rtc_0.enable_interrupt(hal::rtc::RtcInterrupt::Tick, None);
        let rtc_0 = rtc_0.enable_counter();

        let mut rtc_1 = hal::rtc::Rtc::new(cx.device.RTC1);
        let _ = rtc_1.set_prescaler(4095);
        let rtc_1 = rtc_1.enable_counter();

        let port0 = gpio::p0::Parts::new(cx.device.P0);
        let button_1 = port0.p0_11.into_pullup_input().degrade();
        let button_2 = port0.p0_12.into_pullup_input().degrade();
        let button_3 = port0.p0_24.into_pullup_input().degrade();
        let button_4 = port0.p0_25.into_pullup_input().degrade();
        let led_1 = port0
            .p0_13
            .into_push_pull_output(gpio::Level::Low)
            .degrade();
        let led_2 = port0
            .p0_14
            .into_push_pull_output(gpio::Level::High)
            .degrade();
        let led_3 = port0
            .p0_15
            .into_push_pull_output(gpio::Level::High)
            .degrade();
        let led_4 = port0
            .p0_16
            .into_push_pull_output(gpio::Level::High)
            .degrade();

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

        let mut itm = cx.core.ITM;
        iprintln!(&mut itm.stim[0], "Initialization");
        init::LateResources {
            itm,
            timer_0: cx.device.TIMER0,
            timer_1: cx.device.TIMER1,
            button_1,
            button_2,
            button_3,
            button_4,
            led_1,
            led_2,
            led_3,
            led_4,
            rtc_0,
            rtc_1,
            uart,
        }
    }

    #[task(binds = TIMER0, resources = [itm, rtc_1, rtc_1_last, timer_0, led_3, on_off])]
    fn timer(cx: timer::Context) {
        cx.resources.timer_0.timer_reset_event();
        let itm_port = &mut cx.resources.itm.stim[0];
        let rtc_last = *cx.resources.rtc_1_last;
        let rtc_now = cx.resources.rtc_1.get_counter();
        let elapsed = rtc_now.saturating_sub(rtc_last);
        iprintln!(itm_port, "Timer 0: {}", elapsed);
        if *cx.resources.on_off {
            let _ = cx.resources.led_3.set_low();
        } else {
            let _ = cx.resources.led_3.set_high();
        }
        *cx.resources.on_off = !*cx.resources.on_off;
        *cx.resources.rtc_1_last = rtc_now;
    }

    #[task(binds = RTC0, resources = [itm, rtc_0, timer_1, timer_1_last, button_4, led_4])]
    fn rtc(cx: rtc::Context) {
        let _ = cx
            .resources
            .rtc_0
            .get_event_triggered(hal::rtc::RtcInterrupt::Tick, true);
        let itm_port = &mut cx.resources.itm.stim[0];
        let timer_last = *cx.resources.timer_1_last;
        let timer_now = cx.resources.timer_1.read_counter();
        let elapsed = timer_now.saturating_sub(timer_last);
        iprintln!(itm_port, "RTC 0: {}", elapsed);

        let button_4 = cx.resources.button_4;
        let led_4 = cx.resources.led_4;

        match button_4.is_low() {
            Ok(true) => {
                let _ = led_4.set_low();
            }
            Ok(false) => {
                let _ = led_4.set_high();
            }
            Err(_) => {}
        }
        *cx.resources.timer_1_last = timer_now;
    }

    #[idle(resources = [button_2, led_2, uart])]
    fn idle(cx: idle::Context) -> ! {
        let button_2 = cx.resources.button_2;
        let led_2 = cx.resources.led_2;
        let uart = cx.resources.uart;

        let _ = write!(uart, "Idle\r\n");

        loop {
            match button_2.is_low() {
                Ok(true) => {
                    let _ = led_2.set_low();
                }
                Ok(false) => {
                    let _ = led_2.set_high();
                }
                Err(_) => {}
            }
        }
    }
};
