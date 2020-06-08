#![no_main]
#![no_std]

use utilities::{spi, st7735s};

use core::fmt::Write;

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};

use rtfm::app;

use embedded_hal::{
    digital::v2::{InputPin, OutputPin}
};

use crate::hal::target as pac;
use nrf52833_hal as hal;

use hal::{clocks, gpio, timer::Instance, uarte, spim};
use pac::{RTC0, RTC1, TIMER0, TIMER1, UARTE0, SPIM3};

use embedded_graphics::{
    drawable::Drawable,
    geometry::Point,
    pixelcolor::{RgbColor, Rgb565},
    primitives::{Primitive, rectangle::Rectangle},
    style::PrimitiveStyleBuilder,
};
use embedded_graphics::{ egtext, text_style };
use profont::ProFont12Point;

use st7735s::{Orientation};

#[app(device = crate::hal::target, peripherals = true)]
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
        rtc_0: hal::rtc::Rtc<RTC0, hal::rtc::Started>,
        rtc_1: hal::rtc::Rtc<RTC1, hal::rtc::Started>,
        #[init(0)]
        rtc_1_last: u32,
        timer_0: TIMER0,
        timer_1: TIMER1,
        #[init(0)]
        timer_1_last: u32,
        uart: uarte::Uarte<UARTE0>,
        delay: hal::Delay,
        lcd: st7735s::ST7735<spi::Spim<SPIM3>>
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        // Configure to use external clocks, and start them
        let _clocks = clocks::Clocks::new(cx.device.CLOCK)
            .set_lfclk_src_external(clocks::LfOscConfiguration::NoExternalNoBypass)
            .start_lfclk();

        rtt_init_print!();
        rprintln!("Initialize...");

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

        let delay = hal::Delay::new(cx.core.SYST);
        let spi = spi::Spim::new(cx.device.SPIM3,
            spi::Pins {
                sck: port0.p0_27.into_push_pull_output(gpio::Level::Low).degrade(),
                mosi: Some(port0.p0_26.into_push_pull_output(gpio::Level::Low).degrade()),
                miso: None,
                csn: Some(port0.p0_21.into_push_pull_output(gpio::Level::Low).degrade()),
                dcx: Some(port0.p0_22.into_push_pull_output(gpio::Level::High).degrade()),
            }, spim::Frequency::M4, spim::MODE_0, 0);
        
        let lcd = st7735s::ST7735::new(spi, false, true, 80, 160);

        rprintln!("... done");

        init::LateResources {
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
            delay,
            lcd,
        }
    }

    #[task(binds = TIMER0, resources = [rtc_1, rtc_1_last, timer_0, led_3, on_off])]
    fn timer(cx: timer::Context) {
        cx.resources.timer_0.timer_reset_event();
        let rtc_last = *cx.resources.rtc_1_last;
        let rtc_now = cx.resources.rtc_1.get_counter();
        let elapsed = rtc_now.saturating_sub(rtc_last);
        rprintln!("Timer 0: {}", elapsed);

        if *cx.resources.on_off {
            let _ = cx.resources.led_3.set_low();
        } else {
            let _ = cx.resources.led_3.set_high();
        }
        *cx.resources.on_off = !*cx.resources.on_off;
        *cx.resources.rtc_1_last = rtc_now;
    }

    #[task(binds = RTC0, resources = [rtc_0, timer_1, timer_1_last, button_4, led_4])]
    fn rtc(cx: rtc::Context) {
        let _ = cx
            .resources
            .rtc_0
            .get_event_triggered(hal::rtc::RtcInterrupt::Tick, true);
        let timer_last = *cx.resources.timer_1_last;
        let timer_now = cx.resources.timer_1.read_counter();
        let elapsed = timer_now.saturating_sub(timer_last);
        rprintln!("RTC 0: {}", elapsed);

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

    #[idle(resources = [button_2, led_2, uart, lcd, delay])]
    fn idle(cx: idle::Context) -> ! {
        let button_2 = cx.resources.button_2;
        let led_2 = cx.resources.led_2;
        let uart = cx.resources.uart;
        let lcd = cx.resources.lcd;

        let _ = lcd.init(cx.resources.delay);
        let dx = (st7735s::ST7735_ROWS - 160) / 2;
        let dy = (st7735s::ST7735_COLS - 80) / 2;
        lcd.set_offset(dx, dy);
        let _ = lcd.set_orientation(Orientation::Landscape);
        let style = PrimitiveStyleBuilder::new().fill_color(Rgb565::BLACK).build();
        let backdrop = Rectangle::new(Point::new(0, 0), Point::new(160, 81)).into_styled(style);
        let _ = backdrop.draw(lcd);
        let _ = egtext!(
            text = "Rust on nRF52833-DK\n\n",
            top_left = (5, 0),
            style = text_style!(font = ProFont12Point, text_color = Rgb565::new(0xff, 0x8c, 0x00))
        )
        .draw(lcd);
        
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
