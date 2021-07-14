/// Send command and data over SPI
pub trait SpiSendCommandData {
    ///
    fn send_command_data(&mut self, data: &[u8], command_bytes: u8) -> Result<(), Error>;
}

// NRF52840 and NRF9160 16 bits 1..0xFFFF
pub const EASY_DMA_SIZE: usize = 65535;
// Limits for Easy DMA - it can only read from data ram
pub const SRAM_LOWER: usize = 0x2000_0000;
pub const SRAM_UPPER: usize = 0x3000_0000;
pub const FORCE_COPY_BUFFER_SIZE: usize = 1024;

/// Does this slice reside entirely within RAM?
pub(crate) fn slice_in_ram(slice: &[u8]) -> bool {
    let ptr = slice.as_ptr() as usize;
    ptr >= SRAM_LOWER && (ptr + slice.len()) < SRAM_UPPER
}

/// Return an error if slice is not in RAM
pub(crate) fn slice_in_ram_or<T>(slice: &[u8], err: T) -> Result<(), T> {
    if slice_in_ram(slice) {
        Ok(())
    } else {
        Err(err)
    }
}

/// A handy structure for converting rust slices into ptr and len pairs
/// for use with EasyDMA. Care must be taken to make sure mutability
/// guarantees are respected
pub(crate) struct DmaSlice {
    ptr: u32,
    len: u32,
}

impl DmaSlice {
    pub fn null() -> Self {
        Self { ptr: 0, len: 0 }
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        Self {
            ptr: slice.as_ptr() as u32,
            len: slice.len() as u32,
        }
    }
}

// HAL interface to the SPIM peripheral
//
// See product specification, chapter 31.
use core::ops::Deref;
use core::sync::atomic::{compiler_fence, Ordering::SeqCst};

use crate::hal::pac::{spim0, SPIM0, SPIM1, SPIM2, SPIM3};

pub use embedded_hal::spi::{Mode, Phase, Polarity, MODE_0, MODE_1, MODE_2, MODE_3};
pub use spim0::frequency::FREQUENCY_A as Frequency;

use core::iter::repeat_with;

use crate::hal::gpio::{Floating, Input, Output, Pin, PushPull};

/// Interface to a SPIM instance
///
/// This is a very basic interface that comes with the following limitations:
/// - The SPIM instances share the same address space with instances of SPIS,
///   SPI, TWIM, TWIS, and TWI. You need to make sure that conflicting instances
///   are disabled before using `Spim`. See product specification, section 15.2.
pub struct Spim<T>(T);

impl<T> embedded_hal::blocking::spi::Transfer<u8> for Spim<T>
where
    T: Instance,
{
    type Error = Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Error> {
        // If the slice isn't in RAM, we can't write back to it at all
        slice_in_ram_or(words, Error::DMABufferNotInDataMemory)?;

        words.chunks(EASY_DMA_SIZE).try_for_each(|chunk| {
            self.do_spi_dma_transfer(DmaSlice::from_slice(chunk), DmaSlice::from_slice(chunk))
        })?;

        Ok(words)
    }
}

impl<T> embedded_hal::blocking::spi::Write<u8> for Spim<T>
where
    T: Instance,
{
    type Error = Error;

    fn write<'w>(&mut self, words: &'w [u8]) -> Result<(), Error> {
        // Mask on segment where Data RAM is located on nrf52840 and nrf52832
        // Upper limit is choosen to entire area where DataRam can be placed
        let needs_copy = !slice_in_ram(words);

        let chunk_sz = if needs_copy {
            FORCE_COPY_BUFFER_SIZE
        } else {
            EASY_DMA_SIZE
        };

        let step = if needs_copy {
            Self::spi_dma_copy
        } else {
            Self::spi_dma_no_copy
        };

        words.chunks(chunk_sz).try_for_each(|c| step(self, c))
    }
}

impl<T> SpiSendCommandData for Spim<T>
where
    T: Instance,
{
    fn send_command_data(&mut self, data: &[u8], command_bytes: u8) -> Result<(), Error> {
        self.write_dc(data, command_bytes)
    }
}

fn port_to_bool(port: crate::hal::gpio::Port) -> bool {
    use crate::hal::gpio::Port;
    match port {
        Port::Port0 => false,
        Port::Port1 => true,
    }
}

impl<T> Spim<T>
where
    T: Instance,
{
    fn spi_dma_no_copy(&mut self, chunk: &[u8]) -> Result<(), Error> {
        self.do_spi_dma_transfer(DmaSlice::from_slice(chunk), DmaSlice::null())
    }

    fn spi_dma_copy(&mut self, chunk: &[u8]) -> Result<(), Error> {
        let mut buf = [0u8; FORCE_COPY_BUFFER_SIZE];
        buf[..chunk.len()].copy_from_slice(chunk);

        self.do_spi_dma_transfer(DmaSlice::from_slice(&buf[..chunk.len()]), DmaSlice::null())
    }

    pub fn new(spim: T, pins: Pins, frequency: Frequency, mode: Mode, orc: u8) -> Self {
        // Select pins
        spim.psel.sck.write(|w| {
            let w = unsafe { w.pin().bits(pins.sck.pin()) };
            w.port()
                .bit(port_to_bool(pins.sck.port()))
                .connect()
                .connected()
        });
        match pins.mosi {
            Some(mosi) => spim.psel.mosi.write(|w| {
                let w = unsafe { w.pin().bits(mosi.pin()) };
                w.port()
                    .bit(port_to_bool(mosi.port()))
                    .connect()
                    .connected()
            }),
            None => spim.psel.mosi.write(|w| w.connect().disconnected()),
        }
        match pins.miso {
            Some(miso) => spim.psel.miso.write(|w| {
                let w = unsafe { w.pin().bits(miso.pin()) };
                w.port()
                    .bit(port_to_bool(miso.port()))
                    .connect()
                    .connected()
            }),
            None => spim.psel.miso.write(|w| w.connect().disconnected()),
        }
        match pins.csn {
            Some(cs) => {
                spim.psel.csn.write(|w| {
                    let w = unsafe { w.pin().bits(cs.pin()) };
                    w.port().bit(port_to_bool(cs.port())).connect().connected()
                });
                spim.csnpol.write(|w| w.csnpol().low());
                spim.iftiming
                    .csndur
                    .write(|w| unsafe { w.csndur().bits(0x1f) });
            }
            None => spim.psel.csn.write(|w| w.connect().disconnected()),
        }
        match pins.dcx {
            Some(dcx) => spim.pseldcx.write(|w| {
                let w = unsafe { w.pin().bits(dcx.pin()) };
                w.port().bit(port_to_bool(dcx.port())).connect().connected()
            }),
            None => spim.pseldcx.write(|w| w.connect().disconnected()),
        }

        // Enable SPIM instance
        spim.enable.write(|w| w.enable().enabled());

        // Configure mode
        spim.config.write(|w| {
            w.order().msb_first();
            match mode.polarity {
                Polarity::IdleLow => {
                    w.cpol().active_high();
                }
                Polarity::IdleHigh => {
                    w.cpol().active_low();
                }
            }
            match mode.phase {
                Phase::CaptureOnFirstTransition => {
                    w.cpha().leading();
                }
                Phase::CaptureOnSecondTransition => {
                    w.cpha().trailing();
                }
            }
            w
        });

        // Configure frequency
        spim.frequency.write(|w| w.frequency().variant(frequency));

        // Set over-read character to `0`
        spim.orc.write(|w|
            // The ORC field is 8 bits long, so `0` is a valid value to write
            // there.
            unsafe { w.orc().bits(orc) });

        Spim(spim)
    }

    /// Internal helper function to setup and execute SPIM DMA transfer
    fn do_spi_dma_transfer_dcx(
        &mut self,
        tx: DmaSlice,
        rx: DmaSlice,
        command_bytes: u8,
    ) -> Result<(), Error> {
        // Configure DCX bytes
        self.0
            .dcxcnt
            .write(|w| unsafe { w.bits(command_bytes as u32) });
        self.do_spi_dma_transfer(tx, rx)
    }

    /// Internal helper function to setup and execute SPIM DMA transfer
    fn do_spi_dma_transfer(&mut self, tx: DmaSlice, rx: DmaSlice) -> Result<(), Error> {
        // Conservative compiler fence to prevent optimizations that do not
        // take in to account actions by DMA. The fence has been placed here,
        // before any DMA action has started
        compiler_fence(SeqCst);

        // Set up the DMA write
        self.0.txd.ptr.write(|w| unsafe { w.ptr().bits(tx.ptr) });

        self.0.txd.maxcnt.write(|w|
            // Note that that nrf52840 maxcnt is a wider
            // type than a u8, so we use a `_` cast rather than a `u8` cast.
            // The MAXCNT field is thus at least 8 bits wide and accepts the full
            // range of values that fit in a `u8`.
            unsafe { w.maxcnt().bits(tx.len as _ ) });

        // Set up the DMA read
        self.0.rxd.ptr.write(|w|
            // This is safe for the same reasons that writing to TXD.PTR is
            // safe. Please refer to the explanation there.
            unsafe { w.ptr().bits(rx.ptr) });
        self.0.rxd.maxcnt.write(|w|
            // This is safe for the same reasons that writing to TXD.MAXCNT is
            // safe. Please refer to the explanation there.
            unsafe { w.maxcnt().bits(rx.len as _) });

        // Start SPI transaction
        self.0.tasks_start.write(|w|
            // `1` is a valid value to write to task registers.
            unsafe { w.bits(1) });

        // Conservative compiler fence to prevent optimizations that do not
        // take in to account actions by DMA. The fence has been placed here,
        // after all possible DMA actions have completed
        compiler_fence(SeqCst);

        // Wait for END event
        //
        // This event is triggered once both transmitting and receiving are
        // done.
        while self.0.events_end.read().bits() == 0 {}

        // Reset the event, otherwise it will always read `1` from now on.
        self.0.events_end.write(|w| w);

        // Conservative compiler fence to prevent optimizations that do not
        // take in to account actions by DMA. The fence has been placed here,
        // after all possible DMA actions have completed
        compiler_fence(SeqCst);

        if self.0.txd.amount.read().bits() != tx.len {
            return Err(Error::Transmit);
        }
        if self.0.rxd.amount.read().bits() != rx.len {
            return Err(Error::Receive);
        }
        Ok(())
    }

    /// Read from an SPI slave
    ///
    /// This method is deprecated. Consider using `transfer` or `transfer_split`
    #[inline(always)]
    pub fn read(&mut self, tx_buffer: &[u8], rx_buffer: &mut [u8]) -> Result<(), Error> {
        self.transfer_split_uneven(tx_buffer, rx_buffer)
    }

    /// Read and write from a SPI slave, using a single buffer
    ///
    /// This method implements a complete read transaction, which consists of
    /// the master transmitting what it wishes to read, and the slave responding
    /// with the requested data.
    ///
    /// Uses the provided chip select pin to initiate the transaction. Transmits
    /// all bytes in `buffer`, then receives an equal number of bytes.
    pub fn transfer(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        slice_in_ram_or(buffer, Error::DMABufferNotInDataMemory)?;

        // Don't return early, as we must reset the CS pin
        buffer.chunks(EASY_DMA_SIZE).try_for_each(|chunk| {
            self.do_spi_dma_transfer(DmaSlice::from_slice(chunk), DmaSlice::from_slice(chunk))
        })
    }

    /// Read and write from a SPI slave, using separate read and write buffers
    ///
    /// This method implements a complete read transaction, which consists of
    /// the master transmitting what it wishes to read, and the slave responding
    /// with the requested data.
    ///
    /// Uses the provided chip select pin to initiate the transaction. Transmits
    /// all bytes in `tx_buffer`, then receives bytes until `rx_buffer` is full.
    ///
    /// If `tx_buffer.len() != rx_buffer.len()`, the transaction will stop at the
    /// smaller of either buffer.
    pub fn transfer_split_even(
        &mut self,
        tx_buffer: &[u8],
        rx_buffer: &mut [u8],
    ) -> Result<(), Error> {
        // NOTE: RAM slice check for `rx_buffer` is not necessary, as a mutable
        // slice can only be built from data located in RAM
        slice_in_ram_or(tx_buffer, Error::DMABufferNotInDataMemory)?;

        let txi = tx_buffer.chunks(EASY_DMA_SIZE);
        let rxi = rx_buffer.chunks_mut(EASY_DMA_SIZE);

        // Don't return early, as we must reset the CS pin
        txi.zip(rxi).try_for_each(|(t, r)| {
            self.do_spi_dma_transfer(DmaSlice::from_slice(t), DmaSlice::from_slice(r))
        })
    }

    /// Read and write from a SPI slave, using separate read and write buffers
    ///
    /// This method implements a complete read transaction, which consists of
    /// the master transmitting what it wishes to read, and the slave responding
    /// with the requested data.
    ///
    /// Uses the provided chip select pin to initiate the transaction. Transmits
    /// all bytes in `tx_buffer`, then receives bytes until `rx_buffer` is full.
    ///
    /// This method is more complicated than the other `transfer` methods because
    /// it is allowed to perform transactions where `tx_buffer.len() != rx_buffer.len()`.
    /// If this occurs, extra incoming bytes will be discarded, OR extra outgoing bytes
    /// will be filled with the `orc` value.
    pub fn transfer_split_uneven(
        &mut self,
        tx_buffer: &[u8],
        rx_buffer: &mut [u8],
    ) -> Result<(), Error> {
        // NOTE: RAM slice check for `rx_buffer` is not necessary, as a mutable
        // slice can only be built from data located in RAM
        slice_in_ram_or(tx_buffer, Error::DMABufferNotInDataMemory)?;

        // For the tx and rx, we want to return Some(chunk)
        // as long as there is data to send. We then chain a repeat to
        // the end so once all chunks have been exhausted, we will keep
        // getting Nones out of the iterators
        let txi = tx_buffer
            .chunks(EASY_DMA_SIZE)
            .map(|c| Some(c))
            .chain(repeat_with(|| None));

        let rxi = rx_buffer
            .chunks_mut(EASY_DMA_SIZE)
            .map(|c| Some(c))
            .chain(repeat_with(|| None));

        // We then chain the iterators together, and once BOTH are feeding
        // back Nones, then we are done sending and receiving
        //
        // Don't return early, as we must reset the CS pin
        txi.zip(rxi)
            .take_while(|(t, r)| t.is_some() && r.is_some())
            // We also turn the slices into either a DmaSlice (if there was data), or a null
            // DmaSlice (if there is no data)
            .map(|(t, r)| {
                (
                    t.map(|t| DmaSlice::from_slice(t))
                        .unwrap_or_else(|| DmaSlice::null()),
                    r.map(|r| DmaSlice::from_slice(r))
                        .unwrap_or_else(|| DmaSlice::null()),
                )
            })
            .try_for_each(|(t, r)| self.do_spi_dma_transfer(t, r))
    }

    /// Write to an SPI slave
    ///
    /// This method uses the provided chip select pin to initiate the
    /// transaction, then transmits all bytes in `tx_buffer`. All incoming
    /// bytes are discarded.
    pub fn write(&mut self, tx_buffer: &[u8]) -> Result<(), Error> {
        slice_in_ram_or(tx_buffer, Error::DMABufferNotInDataMemory)?;
        self.transfer_split_uneven(tx_buffer, &mut [0u8; 0])
    }

    /// Write to an SPI slave
    ///
    /// This method uses the provided chip select pin to initiate the
    /// transaction, then transmits all bytes in `tx_buffer`. All incoming
    /// bytes are discarded.
    pub fn write_dc(&mut self, tx_buffer: &[u8], command_bytes: u8) -> Result<(), Error> {
        slice_in_ram_or(tx_buffer, Error::DMABufferNotInDataMemory)?;
        self.do_spi_dma_transfer_dcx(
            DmaSlice::from_slice(tx_buffer),
            DmaSlice::null(),
            command_bytes,
        )
    }

    /// Return the raw interface to the underlying SPIM peripheral
    pub fn free(self) -> T {
        self.0
    }
}

/// GPIO pins for SPIM interface
pub struct Pins {
    /// SPI clock
    pub sck: Pin<Output<PushPull>>,

    /// MOSI Master out, slave in
    /// None if unused
    pub mosi: Option<Pin<Output<PushPull>>>,

    /// MISO Master in, slave out
    /// None if unused
    pub miso: Option<Pin<Input<Floating>>>,

    /// Chip Select
    /// None if unused
    pub csn: Option<Pin<Output<PushPull>>>,

    /// Data / command select
    /// None if unused
    pub dcx: Option<Pin<Output<PushPull>>>,
}

#[derive(Debug)]
pub enum Error {
    TxBufferTooLong,
    RxBufferTooLong,
    /// EasyDMA can only read from data memory, read only buffers in flash will fail
    DMABufferNotInDataMemory,
    Transmit,
    Receive,
}

/// Implemented by all SPIM instances
pub trait Instance: Deref<Target = spim0::RegisterBlock> {}

impl Instance for SPIM0 {}
impl Instance for SPIM1 {}
impl Instance for SPIM2 {}
impl Instance for SPIM3 {}
