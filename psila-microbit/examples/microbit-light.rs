#![no_main]
#![no_std]

use psila_microbit as _;
use rtic::app;

#[app(device = microbit::pac, peripherals = true, dispatchers = [I2S, QDEC])]
mod app {

    // Manufacturer name for this example
    const MANUFACTURER_NAME: &'static str = "ERIK of Sweden";
    // Model identifier for this example
    const MODEL_IDENTIFIER: &'static str = "micro:bit light";

    /// Home automation profile
    const PROFILE_HOME_AUTOMATION: u16 = 0x0104;
    /// Dimmable light device
    const DEVICE_DIMMABLE_LIGHT: u16 = 0x0101;

    /// Basic cluster
    const CLUSTER_BASIC: u16 = 0x0000;
    /// Basic cluster attribute, library version
    const BASIC_ATTR_LIBRARY_VERSION: u16 = 0x0000;
    /// Basic cluster attribute, manufacturer name
    const BASIC_ATTR_MANUFACTURER_NAME: u16 = 0x0004;
    /// Basic cluster attribute, model identifier
    const BASIC_ATTR_MODEL_IDENTIFIER: u16 = 0x0005;
    /// Basic cluster attribute, power source
    const BASIC_ATTR_POWER_SOURCE: u16 = 0x0007;

    /// On/off cluster
    const CLUSTER_ON_OFF: u16 = 0x0006;
    /// On/off cluster attribute, on/off state
    const ON_OFF_ATTR_ON_OFF_STATE: u16 = 0x0000;
    /// On/off cluster command, off
    const ON_OFF_CMD_OFF: u8 = 0x00;
    /// On/off cluster command, on
    const ON_OFF_CMD_ON: u8 = 0x01;
    /// On/off cluster command, toggle
    const ON_OFF_CMD_TOGGLE: u8 = 0x02;

    /// Level control cluster
    const CLUSTER_LEVEL_CONTROL: u16 = 0x0008;
    /// Level control cluster attribute, current level
    const LEVEL_CONTROL_ATTR_CURRENT_LEVEL: u16 = 0x0000;
    /// Level control cluster command, move to level
    const LEVEL_CONTROL_CMD_MOVE_TO_LEVEL: u8 = 0x00;
    /// Level control cluster command, move
    const LEVEL_CONTROL_CMD_MOVE: u8 = 0x01;
    /// Level control cluster command, step
    const LEVEL_CONTROL_CMD_STEP: u8 = 0x02;
    /// Level control cluster command, stop
    const LEVEL_CONTROL_CMD_STOP: u8 = 0x03;
    /// Level control cluster command, move to level with on/off
    const LEVEL_CONTROL_CMD_MOVE_TO_LEVEL_ON_OFF: u8 = 0x04;
    /// Level control cluster command, move with on/off
    const LEVEL_CONTROL_CMD_MOVE_ON_OFF: u8 = 0x05;
    /// Level control cluster command, step with on/off
    const LEVEL_CONTROL_CMD_STEP_ON_OFF: u8 = 0x06;
    /// Level control cluster command, stop with on/off
    const LEVEL_CONTROL_CMD_STOP_ON_OFF: u8 = 0x07;

    use microbit::pac as pac;

    use bbqueue::{self, BBBuffer};
    use byteorder::{ByteOrder, LittleEndian};

    use microbit::{Board, hal::{clocks, rtc::{Rtc, RtcInterrupt}}, display::nonblocking::{Display, GreyscaleImage} };

    use psila_crypto_rust_crypto::RustCryptoBackend;
    use psila_data::{security::DEFAULT_LINK_KEY, ExtendedAddress, Key, cluster_library::{AttributeDataType, ClusterLibraryStatus, Destination}, device_profile::SimpleDescriptor};
    use psila_nrf52::{
        radio::{Radio, MAX_PACKET_LENGHT},
        timer::Timer,
    };
    use psila_service::{self, PsilaService, ClusterLibraryHandler};

    const TIMER_SECOND: u32 = 1_000_000;

    const TX_BUFFER_SIZE: usize = 1024;
    const RX_BUFFER_SIZE: usize = 1024;

    static RX_BUFFER: BBBuffer<RX_BUFFER_SIZE> = BBBuffer::new();
    static TX_BUFFER: BBBuffer<TX_BUFFER_SIZE> = BBBuffer::new();

    pub struct ClusterHandler {
        on_off: bool,
        level: u8,
    }

    impl ClusterHandler {
        pub fn new() -> Self {
            Self {
                on_off: false,
                level: 127,
            }
        }

        fn update_led(&mut self) {
            let level = if self.on_off { self.level } else { 0 };
            let _ = level_update::spawn(level);
        }

        pub fn set_on_off(&mut self, enable: bool) {
            self.on_off = enable;
            self.update_led();
        }

        pub fn get_level(&self) -> u8 {
            self.level
        }

        pub fn set_level(&mut self, level: u8) {
            self.level = level;
            self.update_led();
        }
    }

    impl ClusterLibraryHandler for ClusterHandler {
        fn active_endpoints(&self) -> &[u8] {
            &[0x01]
        }
        fn get_simple_descriptor(&self, endpoint: u8) -> Option<SimpleDescriptor> {
            match endpoint {
                0x01 => Some(SimpleDescriptor::new(
                    0x01,
                    PROFILE_HOME_AUTOMATION,
                    DEVICE_DIMMABLE_LIGHT,
                    0,
                    &[
                        CLUSTER_BASIC,
                        CLUSTER_ON_OFF,
                        CLUSTER_LEVEL_CONTROL,
                    ],
                    &[],
                )),
                _ => None,
            }
        }
        fn read_attribute(
            &self,
            profile: u16,
            cluster: u16,
            _destination: Destination,
            attribute: u16,
            value: &mut [u8],
        ) -> Result<(AttributeDataType, usize), ClusterLibraryStatus> {
            match (profile, cluster, attribute) {
                (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_LIBRARY_VERSION) => {
                    value[0] = 0x02;
                    Ok((AttributeDataType::Unsigned8, 1))
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_MANUFACTURER_NAME) => {
                    value[0] = MANUFACTURER_NAME.len() as u8;
                    let end = MANUFACTURER_NAME.len() + 1;
                    value[1..end].copy_from_slice(MANUFACTURER_NAME.as_bytes());
                    Ok((AttributeDataType::CharacterString, end))
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_MODEL_IDENTIFIER) => {
                    value[0] = MODEL_IDENTIFIER.len() as u8;
                    let end = MODEL_IDENTIFIER.len() + 1;
                    value[1..end].copy_from_slice(MODEL_IDENTIFIER.as_bytes());
                    Ok((AttributeDataType::CharacterString, end))
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_POWER_SOURCE) => {
                    value[0] = 0x01;
                    Ok((AttributeDataType::Enumeration8, 1))
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_ON_OFF, ON_OFF_ATTR_ON_OFF_STATE) => {
                    value[0] = if self.on_off { 0x01 } else { 0x00 };
                    Ok((AttributeDataType::Boolean, 1))
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_ATTR_CURRENT_LEVEL) => {
                    // current level
                    defmt::info!("Read level: {=u8}", self.get_level());
                    value[0] = self.get_level();
                    Ok((AttributeDataType::Unsigned8, 1))
                }
                (_, _, _) => {
                    defmt::info!(
                    "Read attribute: {=u16:04x} {=u16:04x} {=u16:04x}",
                    profile,
                    cluster,
                    attribute
                );
                    Err(ClusterLibraryStatus::UnsupportedAttribute)
                }
            }
        }
        fn write_attribute(
            &mut self,
            profile: u16,
            cluster: u16,
            _destination: Destination,
            attribute: u16,
            data_type: AttributeDataType,
            value: &[u8],
        ) -> Result<(), ClusterLibraryStatus> {
            match (profile, cluster, attribute, data_type) {
                (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_LIBRARY_VERSION, _)
                | (PROFILE_HOME_AUTOMATION, CLUSTER_BASIC, BASIC_ATTR_POWER_SOURCE, _) => {
                    Err(ClusterLibraryStatus::ReadOnly)
                }
                (
                    PROFILE_HOME_AUTOMATION,
                    CLUSTER_ON_OFF,
                    ON_OFF_ATTR_ON_OFF_STATE,
                    AttributeDataType::Boolean,
                ) => {
                    self.set_on_off(value[0] == 0x01);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_ON_OFF, ON_OFF_ATTR_ON_OFF_STATE, _) => {
                    Err(ClusterLibraryStatus::InvalidValue)
                }
                (_, _, _, _) => Err(ClusterLibraryStatus::UnsupportedAttribute),
            }
        }
        fn run(
            &mut self,
            profile: u16,
            cluster: u16,
            _destination: Destination,
            command: u8,
            arguments: &[u8],
        ) -> Result<(), ClusterLibraryStatus> {
            match (profile, cluster, command) {
                (PROFILE_HOME_AUTOMATION, CLUSTER_ON_OFF, ON_OFF_CMD_OFF) => {
                    // set off
                    self.set_on_off(false);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_ON_OFF, ON_OFF_CMD_ON) => {
                    // set on
                    self.set_on_off(true);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_ON_OFF, ON_OFF_CMD_TOGGLE) => {
                    // toggle
                    self.set_on_off(!self.on_off);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_MOVE_TO_LEVEL) => {
                    // move to level
                    if arguments.len() >= 3 {
                        let level = arguments[0];
                        let transition_time = LittleEndian::read_u16(&arguments[1..=2]);
                        defmt::info!("Move to level: {=u8} {=u16}", level, transition_time);
                        self.set_level(level);
                    } else {
                        defmt::warn!("Move to level ?");
                    }
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_MOVE) => {
                    // move
                    let mode = arguments[0];
                    let rate = arguments[1];
                    defmt::info!("Move: {=u8} {=u8}", mode, rate);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_STEP) => {
                    // step
                    let mode = arguments[0];
                    let step = arguments[1];
                    let transition_time = LittleEndian::read_u16(&arguments[2..=3]);
                    defmt::info!("Step: {=u8} {=u8} {=u16}", mode, step, transition_time);
                    let level = match mode {
                        0 => self.level.saturating_add(step),
                        1 => self.level.saturating_sub(step),
                        _ => self.level,
                    };
                    self.set_level(level);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_STOP)
                | (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_STOP_ON_OFF) => {
                    // stop
                    defmt::info!("Stop");
                    Ok(())
                }
                (
                    PROFILE_HOME_AUTOMATION,
                    CLUSTER_LEVEL_CONTROL,
                    LEVEL_CONTROL_CMD_MOVE_TO_LEVEL_ON_OFF,
                ) => {
                    // move to level, on / off
                    let level = arguments[0];
                    let _transition_time = LittleEndian::read_u16(&arguments[1..=2]);
                    self.set_on_off(level > 0);
                    self.set_level(level);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_MOVE_ON_OFF) => {
                    // move, on / off
                    let mode = arguments[0];
                    let rate = arguments[1];
                    defmt::info!("Move (on/off): {=u8} {=u8}", mode, rate);
                    Ok(())
                }
                (PROFILE_HOME_AUTOMATION, CLUSTER_LEVEL_CONTROL, LEVEL_CONTROL_CMD_STEP_ON_OFF) => {
                    // step, on / off
                    let mode = arguments[0];
                    let step = arguments[1];
                    let transition_time = LittleEndian::read_u16(&arguments[2..=3]);
                    defmt::info!(
                    "Step (on/off): {=u8} {=u8} {=u16}",
                    mode,
                    step,
                    transition_time
                );
                    let level = match mode {
                        0 => self.level.saturating_add(step),
                        1 => self.level.saturating_sub(step),
                        _ => self.level,
                    };
                    self.set_on_off(level > 0);
                    self.set_level(level);
                    Ok(())
                }
                (_, _, _) => {
                    defmt::info!("Command {=u16:04x} {=u16:04x} {=u8:04x}", profile, cluster, command);
                    Err(ClusterLibraryStatus::UnsupportedClusterCommand)
                }
            }
        }
    }

    fn image(level: u8) -> GreyscaleImage {
        let leds = level / 10;
        let leds = if leds > 25 { 25 } else { leds };
        let brightness = level - (leds * 10);
        let brightness = if brightness > 10 { 10 } else { brightness };

        let mut counter = i32::from(leds);

        let mut data = [[0u8; 5]; 5];

        for x in 0..5 {
            for y in 0..5 {
                if counter > 0 {
                    data[y][x] = 9;
                }
                if counter == 0 {
                    data[y][x] = brightness;
                }
                counter -= 1;
            }
        }
        GreyscaleImage::new(&data)
    }

    #[local]
    struct LocalResources {
        rx_producer: bbqueue::Producer<'static, RX_BUFFER_SIZE>,
        rx_consumer: bbqueue::Consumer<'static, RX_BUFFER_SIZE>,
        tx_consumer: bbqueue::Consumer<'static, TX_BUFFER_SIZE>,
        anim_timer: Rtc<pac::RTC0>,
    }

    #[shared]
    struct SharedResources {
        level: u8,
        display: Display<pac::TIMER0>,
        timer: pac::TIMER1,
        radio: Radio,
        service: PsilaService<'static, RustCryptoBackend, ClusterHandler, TX_BUFFER_SIZE>,
    }

    #[init]
    fn init(cx: init::Context) -> (SharedResources, LocalResources, init::Monotonics) {
        let board = Board::new(cx.device, cx.core);

        let mut rtc0 = Rtc::new(board.RTC0, 2047).unwrap();
        rtc0.enable_event(RtcInterrupt::Tick);
        rtc0.enable_interrupt(RtcInterrupt::Tick, None);
        rtc0.enable_counter();

        let display = Display::new(board.TIMER0, board.display_pins);

        // Configure to use external clocks, and start them
        let _clocks = clocks::Clocks::new(board.CLOCK)
            .enable_ext_hfosc()
            .set_lfclk_src_external(clocks::LfOscConfiguration::NoExternalNoBypass)
            .start_lfclk();

        let level = 127;
        let handler = ClusterHandler::new();

        // MAC (EUI-48) address to EUI-64
        // Add FF FE in the middle
        //
        //    01 23 45 67 89 AB
        //  /  /  /       \  \  \
        // 01 23 45 FF FE 67 89 AB
        let devaddr_lo = board.FICR.deviceaddr[0].read().bits();
        let devaddr_hi = board.FICR.deviceaddr[1].read().bits() as u16;
        let extended_address = u64::from(devaddr_hi) << 48
            | u64::from(devaddr_lo & 0xff00_0000) << 40
            | u64::from(devaddr_lo & 0x00ff_ffff)
            | 0x0000_00ff_fe00_0000u64;
        let extended_address = ExtendedAddress::new(extended_address);

        let mut timer1 = board.TIMER1;
        timer1.init();
        timer1.fire_in(1, TIMER_SECOND);

        let mut radio = Radio::new(board.RADIO);
        radio.set_channel(11);
        radio.set_transmission_power(8);
        radio.receive_prepare();

        let (rx_producer, rx_consumer) = RX_BUFFER.try_split().unwrap();
        let (tx_producer, tx_consumer) = TX_BUFFER.try_split().unwrap();

        let crypto_backend = RustCryptoBackend::default();
        let default_link_key = Key::from(DEFAULT_LINK_KEY);

        (
            SharedResources {
                level,
                timer: timer1,
                radio,
                service: PsilaService::new(
                    crypto_backend,
                    tx_producer,
                    extended_address,
                    default_link_key,
                    handler,
                ),
                display,
            },
            LocalResources {
                rx_producer,
                rx_consumer,
                tx_consumer,
                anim_timer: rtc0,
            },
            init::Monotonics(),
        )
    }

    #[task(binds = TIMER1, shared = [service, timer])]
    fn timer(cx: timer::Context) {
        (cx.shared.timer, cx.shared.service).lock(|timer, service| {
            if timer.is_compare_event(1) {
                timer.ack_compare_event(1);
                let _ = service.update(timer.now());
                timer.fire_in(1, TIMER_SECOND);
            }
            let _ = radio_tx::spawn();
        });
    }

    #[task(binds = RADIO, shared = [radio, service], local = [rx_producer])]
    fn radio(cx: radio::Context) {
        let queue = cx.local.rx_producer;
        (cx.shared.radio, cx.shared.service).lock(|radio, service| {
            let mut packet = [0u8; MAX_PACKET_LENGHT as usize];
            match radio.receive(&mut packet) {
                Ok(packet_len) => {
                    if packet_len > 0 {
                        match service.handle_acknowledge(&packet[1..packet_len - 1]) {
                            Ok(to_me) => {
                                if to_me {
                                    if let Ok(mut grant) = queue.grant_exact(packet_len) {
                                        grant.copy_from_slice(&packet[..packet_len]);
                                        grant.commit(packet_len);
                                    }
                                }
                            }
                            Err(e) => match e {
                                psila_service::Error::MalformedPacket => {
                                    defmt::warn!(
                                        "service handle acknowledge failed, malformed package"
                                    );
                                }
                                psila_service::Error::NotEnoughSpace => {
                                    defmt::warn!("service handle acknowledge failed, queue full");
                                }
                                _ => {
                                    defmt::warn!("service handle acknowledge failed");
                                }
                            },
                        }
                    }
                }
                Err(psila_nrf52::radio::Error::CcaBusy) => {
                    defmt::warn!("CCA Busy");
                }
            }
            let _ = radio_tx::spawn();
        });
    }

    #[task(shared = [service, timer], local = [rx_consumer])]
    fn radio_rx(mut cx: radio_rx::Context) {
        let queue = cx.local.rx_consumer;
        let timestamp = cx.shared.timer.lock(|timer| timer.now());
        cx.shared.service.lock(|service| {
            if let Ok(grant) = queue.read() {
                let packet_length = grant[0] as usize;
                if let Err(_) = service.receive(timestamp, &grant[1..packet_length - 1]) {
                    defmt::warn!("service receive failed");
                }
                grant.release(packet_length);
                let _ = radio_tx::spawn();
            }
        });
    }

    #[task(shared = [radio], local = [tx_consumer])]
    fn radio_tx(mut cx: radio_tx::Context) {
        let queue = cx.local.tx_consumer;
        cx.shared.radio.lock(|radio| {
            if !radio.is_tx_busy() {
                if let Ok(grant) = queue.read() {
                    let packet_length = grant[0] as usize;
                    let data = &grant[1..=packet_length];
                    let _ = radio.queue_transmission(data);
                    grant.release(packet_length + 1);
                }
                let _ = radio_rx::spawn();
            }
        });
    }

    #[task(binds = TIMER0, priority = 2, shared = [display])]
    fn timer1(mut cx: timer1::Context) {
        cx.shared
            .display
            .lock(|display| display.handle_display_event());
    }

    #[task(binds = RTC0, priority = 2, shared = [display, level], local = [anim_timer])]
    fn rtc0(cx: rtc0::Context) {
        cx.local.anim_timer.reset_event(RtcInterrupt::Tick);
        (cx.shared.display, cx.shared.level).lock(|display, level| {
            display.show(&image(*level));
        });
    }

    #[task(shared = [level], capacity = 10)]
    fn level_update(mut cx: level_update::Context, new_level: u8) {
        (cx.shared.level).lock(|level| {
            *level = new_level;
        });
    }
}
