//! This crate provides a ST7735 driver to connect to TFT displays.

use crate::extended_enum;

extended_enum!(
    /// ST7735 instructions.
    Instruction, u8,
    NOP => 0x00,
    SWRESET => 0x01,
    RDDID => 0x04,
    RDDST => 0x09,
    SLPIN => 0x10,
    SLPOUT => 0x11,
    PTLON => 0x12,
    NORON => 0x13,
    INVOFF => 0x20,
    INVON => 0x21,
    DISPOFF => 0x28,
    DISPON => 0x29,
    CASET => 0x2A,
    RASET => 0x2B,
    RAMWR => 0x2C,
    RAMRD => 0x2E,
    PTLAR => 0x30,
    COLMOD => 0x3A,
    MADCTL => 0x36,
    FRMCTR1 => 0xB1,
    FRMCTR2 => 0xB2,
    FRMCTR3 => 0xB3,
    INVCTR => 0xB4,
    DISSET5 => 0xB6,
    PWCTR1 => 0xC0,
    PWCTR2 => 0xC1,
    PWCTR3 => 0xC2,
    PWCTR4 => 0xC3,
    PWCTR5 => 0xC4,
    VMCTR1 => 0xC5,
    RDID1 => 0xDA,
    RDID2 => 0xDB,
    RDID3 => 0xDC,
    RDID4 => 0xDD,
    PWCTR6 => 0xFC,
    GMCTRP1 => 0xE0,
    GMCTRN1 => 0xE1,
    );

pub const ST7735_COLS: u16 = 132;
pub const ST7735_ROWS: u16 = 162;

use embedded_hal::blocking::delay::DelayMs;

/// ST7735 driver to connect to TFT displays.
pub struct ST7735<SPI>
where
    SPI: crate::spi::SpiSendCommandData
{
    /// SPI
    spi: SPI,

    /// Whether the display is RGB (true) or BGR (false)
    rgb: bool,

    /// Whether the colours are inverted (true) or not (false)
    inverted: bool,

    /// Global image offset
    dx: u16,
    dy: u16,
    width: u32,
    height: u32,
}

extended_enum!(
    /// Display orientation.
    Orientation, u8,
    Portrait => 0x00,
    Landscape => 0x60,
    PortraitSwapped => 0xC0,
    LandscapeSwapped => 0xA0,
);

impl<SPI> ST7735<SPI>
where
    SPI: crate::spi::SpiSendCommandData
{
    /// Creates a new driver instance that uses hardware SPI.
    pub fn new(
        spi: SPI,
        rgb: bool,
        inverted: bool,
        width: u32,
        height: u32,
    ) -> Self {
        ST7735 {
            spi,
            rgb,
            inverted,
            dx: 0,
            dy: 0,
            width,
            height,
        }
    }

    /// Runs commands to initialize the display.
    pub fn init<DELAY>(&mut self, delay: &mut DELAY) -> Result<(), ()>
    where
        DELAY: DelayMs<u8>,
    {
        self.write_command(Instruction::SWRESET, &[])?;
        delay.delay_ms(200);
        self.write_command(Instruction::SLPOUT, &[])?;
        delay.delay_ms(200);
        self.write_command(Instruction::FRMCTR1, &[0x01, 0x2C, 0x2D])?;
        self.write_command(Instruction::FRMCTR2, &[0x01, 0x2C, 0x2D])?;
        self.write_command(
            Instruction::FRMCTR3,
            &[0x01, 0x2C, 0x2D, 0x01, 0x2C, 0x2D],
        )?;
        self.write_command(Instruction::INVCTR, &[0x07])?;
        self.write_command(Instruction::PWCTR1, &[0xA2, 0x02, 0x84])?;
        self.write_command(Instruction::PWCTR2, &[0xC5])?;
        self.write_command(Instruction::PWCTR3, &[0x0A, 0x00])?;
        self.write_command(Instruction::PWCTR4, &[0x8A, 0x2A])?;
        self.write_command(Instruction::PWCTR5, &[0x8A, 0xEE])?;
        self.write_command(Instruction::VMCTR1, &[0x0E])?;
        if self.inverted {
            self.write_command(Instruction::INVON, &[])?;
        } else {
            self.write_command(Instruction::INVOFF, &[])?;
        }
        if self.rgb {
            self.write_command(Instruction::MADCTL, &[0x00])?;
        } else {
            self.write_command(Instruction::MADCTL, &[0x08])?;
        }
        self.write_command(Instruction::COLMOD, &[0x05])?;
        self.write_command(Instruction::DISPON, &[])?;
        delay.delay_ms(200);
        Ok(())
    }

    fn write_command(&mut self, command: Instruction, params: &[u8]) -> Result<(), ()> {
        let mut spi_data = [0u8; 128];
        spi_data[0] = u8::from(command);
        let octets = if params.len() > 0 {
            let octets = params.len() + 1;
            spi_data[1..octets].copy_from_slice(params);
            octets
        }
        else { 1 };
        self.spi
            .send_command_data(&spi_data[..octets], 1)
            .map_err(|_| ())?;
        Ok(())
    }

    fn write_command_words(&mut self, command: Instruction, params: &[u16]) -> Result<(), ()> {
        let mut spi_data = [0u8; 128];
        spi_data[0] = u8::from(command);
        let octets = if params.len() > 0 {
            let mut offset = 1;
            for word in params {
                let bytes = word.to_be_bytes();
                spi_data[offset] = bytes[0];
                spi_data[offset + 1] = bytes[1];
                offset += 2;
            }
            offset
        } else { 1 };
        self.spi
            .send_command_data(&spi_data[..octets], 1)
            .map_err(|_| ())?;
        Ok(())
    }

    fn write_command_words_iter<P: IntoIterator<Item = u16>>(&mut self, command: Instruction, params: P) -> Result<(), ()> {
        let mut spi_data = [0u8; 32768];
        spi_data[0] = u8::from(command);
        let mut offset = 1;
        for word in params {
            let bytes = word.to_be_bytes();
            spi_data[offset] = bytes[0];
            spi_data[offset + 1] = bytes[1];
            offset += 2;
        }
        self.spi
            .send_command_data(&spi_data[..offset], 1)
            .map_err(|_| ())?;
        Ok(())
    }

    pub fn set_orientation(&mut self, orientation: Orientation) -> Result<(), ()> {
        if self.rgb {
            self.write_command(Instruction::MADCTL, &[u8::from(orientation)])?;
        } else {
            self.write_command(
                Instruction::MADCTL,
                &[u8::from(orientation) | 0x08],
            )?;
        }
        Ok(())
    }

    /// Sets the global offset of the displayed image
    pub fn set_offset(&mut self, dx: u16, dy: u16) {
        self.dx = dx;
        self.dy = dy;
    }

    /// Sets the address window for the display.
    fn set_address_window(&mut self, sx: u16, sy: u16, ex: u16, ey: u16) -> Result<(), ()> {
        self.write_command_words(Instruction::CASET, &[sx + self.dx, ex + self.dx])?;
        self.write_command_words(Instruction::RASET, &[sy + self.dy, ey + self.dy])
    }

    /// Sets a pixel color at the given coords.
    pub fn set_pixel(&mut self, x: u16, y: u16, color: u16) -> Result<(), ()> {
        self.set_address_window(x, y, x, y)?;
        self.write_command_words(Instruction::RAMWR, &[color])
    }

    /// Writes pixel colors sequentially into the current drawing window
    pub fn write_pixels<P: IntoIterator<Item = u16>>(&mut self, colors: P) -> Result<(), ()> {
        self.write_command_words_iter(Instruction::RAMWR, colors)
    }

    pub fn write_pixels_buffered<P: IntoIterator<Item = u16>>(&mut self, colors: P) -> Result<(), ()> {
        self.write_command_words_iter(Instruction::RAMWR, colors)
    }

    /// Sets pixel colors at the given drawing window
    pub fn set_pixels<P: IntoIterator<Item = u16>>(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
        colors: P,
    ) -> Result<(), ()> {
        self.set_address_window(sx, sy, ex, ey)?;
        self.write_pixels(colors)
    }

    pub fn set_pixels_buffered<P: IntoIterator<Item = u16>>(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
        colors: P,
    ) -> Result<(), ()> {
        self.set_address_window(sx, sy, ex, ey)?;
        self.write_pixels_buffered(colors)
    }
}

use embedded_graphics::{
    drawable::Pixel,
    pixelcolor::{
        raw::{RawData, RawU16},
        Rgb565,
    },
    primitives::Rectangle,
    style::{Styled, PrimitiveStyle},
    image::Image,
    prelude::*,
    DrawTarget,
};

impl<SPI> DrawTarget<Rgb565> for ST7735<SPI>
where
    SPI: crate::spi::SpiSendCommandData
{
    type Error = ();

    fn draw_pixel(&mut self, pixel: Pixel<Rgb565>) -> Result<(), Self::Error> {
        let Pixel(Point { x, y }, color) = pixel;
        self.set_pixel(x as u16, y as u16, RawU16::from(color).into_inner())
    }

    fn draw_rectangle(
        &mut self,
        item: &Styled<Rectangle, PrimitiveStyle<Rgb565>>
    ) -> Result<(), Self::Error> {
        let shape = item.primitive;
        let rect_width = shape.bottom_right.x - item.primitive.top_left.x;
        let rect_height = shape.bottom_right.y - item.primitive.top_left.y;
        let rect_size = rect_width * rect_height;

        match (item.style.fill_color, item.style.stroke_color) {
            (Some(fill), None) => {
                let color = RawU16::from(fill).into_inner();
                let iter = (0..rect_size).map(move |_| color);
                self.set_pixels_buffered(
                    shape.top_left.x as u16,
                    shape.top_left.y as u16,
                    shape.bottom_right.x as u16,
                    shape.bottom_right.y as u16,
                    iter,
                )
            },
            (Some(fill), Some(stroke)) => {
                let fill_color = RawU16::from(fill).into_inner();
                let stroke_color = RawU16::from(stroke).into_inner();
                let iter = (0..rect_size).map(move |i| {
                    if i % rect_width <= item.style.stroke_width as i32
                    || i % rect_width >= rect_width - item.style.stroke_width as i32
                    || i <= item.style.stroke_width as i32 * rect_width
                    || i >= (rect_height - item.style.stroke_width as i32) * rect_width
                    {
                        stroke_color
                    }
                    else {
                        fill_color
                    }
                });
                self.set_pixels_buffered(
                    shape.top_left.x as u16,
                    shape.top_left.y as u16,
                    shape.bottom_right.x as u16,
                    shape.bottom_right.y as u16,
                    iter,
                )
            },
            // TODO: Draw edges as subrectangles
            (None, Some(_)) => {
                self.draw_iter(item)
            }
            (None, None) => {
                self.draw_iter(item)
            }
        }
    }

    fn draw_image<'a, 'b, I>(
        &mut self,
        item: &'a Image<'b, I, Rgb565>
    ) -> Result<(), Self::Error>
    where
        &'b I: IntoPixelIter<Rgb565>,
        I: ImageDimensions,
    {
        let sx = item.top_left().x as u16;
        let sy = item.top_left().y as u16;
        let ex = item.bottom_right().x as u16;
        let ey = item.bottom_right().y as u16;
        // -1 is required because image gets skewed if it is not present
        // NOTE: Is this also required for draw_rect?
        self.set_pixels_buffered(
            sx,
            sy,
            ex-1,
            ey-1,
            item.into_iter().map(|p| RawU16::from(p.1).into_inner()),
        )
    }

    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}
