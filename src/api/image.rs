use std::path::Path;

use crate::codec::bmp;
use crate::pixel::PixelFormat;

pub struct Image {
    data: Vec<u8>,
    width: u32,
    height: u32,
    stride: usize,
    format: PixelFormat,
}

impl Image {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        let data = vec![0u8; stride * height as usize];
        Self {
            data,
            width,
            height,
            stride,
            format,
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    #[inline]
    pub fn data_ptr_mut(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    pub fn write_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        bmp::write_bmp(path, self.width, self.height, self.stride, &self.data)
    }
}
