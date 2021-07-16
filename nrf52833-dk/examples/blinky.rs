#![no_main]
#![no_std]

use nrf52833_dk as _;

use rtic::app;

use embedded_hal::digital::v2::{InputPin, OutputPin};

use crate::hal::pac;
use nrf52833_hal as hal;

use hal::{clocks, gpio, timer::Instance};
use pac::{RTC0, TIMER0};

#[app(device = crate::hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        button_1: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_2: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_3: gpio::Pin<gpio::Input<gpio::PullUp>>,
        button_4: gpio::Pin<gpio::Input<gpio::PullUp>>,
        led_1: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_2: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_3: gpio::Pin<gpio::Output<gpio::PushPull>>,
        led_4: gpio::Pin<gpio::Output<gpio::PushPull>>,
        #[init(false)]
        on_off: bool,
        rtc_0: hal::rtc::Rtc<RTC0>,
        timer_0: TIMER0,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        // Configure to use external clocks, and start them
        let _clocks = clocks::Clocks::new(cx.device.CLOCK)
            .enable_ext_hfosc()
            .set_lfclk_src_external(clocks::LfOscConfiguration::NoExternalNoBypass)
            .start_lfclk();

        cx.device.TIMER0.set_periodic();
        cx.device.TIMER0.enable_interrupt();
        cx.device.TIMER0.timer_start(1_000_000u32);

        defmt::info!("Initialize");

        let rtc_0 = match hal::rtc::Rtc::new(cx.device.RTC0, 4095) {
            Ok(mut rtc) => {
                rtc.enable_event(hal::rtc::RtcInterrupt::Tick);
                rtc.enable_interrupt(hal::rtc::RtcInterrupt::Tick, None);
                rtc.enable_counter();
                rtc
            }
            Err(_) => {
                panic!("Failed to initialize RTC");
            }
        };

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

        init::LateResources {
            timer_0: cx.device.TIMER0,
            button_1,
            button_2,
            button_3,
            button_4,
            led_1,
            led_2,
            led_3,
            led_4,
            rtc_0,
        }
    }

    #[task(binds = TIMER0, resources = [timer_0, led_3, on_off])]
    fn timer(cx: timer::Context) {
        cx.resources.timer_0.timer_reset_event();
        if *cx.resources.on_off {
            let _ = cx.resources.led_3.set_low();
        } else {
            let _ = cx.resources.led_3.set_high();
        }
        *cx.resources.on_off = !*cx.resources.on_off;
    }

    #[task(binds = RTC0, resources = [rtc_0, button_4, led_4])]
    fn rtc(cx: rtc::Context) {
        let _ = cx
            .resources
            .rtc_0
            .is_event_triggered(hal::rtc::RtcInterrupt::Tick);
        let button_4 = cx.resources.button_4;
        let led_4 = cx.resources.led_4;

        match button_4.is_low() {
            Ok(true) => {
                defmt::info!("Button 4");
                let _ = led_4.set_low();
            }
            Ok(false) => {
                let _ = led_4.set_high();
            }
            Err(_) => {}
        }
    }

    #[idle(resources = [button_2, led_2])]
    fn idle(cx: idle::Context) -> ! {
        let button_2 = cx.resources.button_2;
        let led_2 = cx.resources.led_2;

        defmt::info!("Idle");

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
