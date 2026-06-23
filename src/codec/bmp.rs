use std::fs::File;
use std::io::Write;
use std::path::Path;

/// BITMAPINFOHEADER + BI_BITFIELDS 形式で BMP を書き出す。
///
/// - negative height で top-down 格納（行反転不要）
/// - PRGB32 のバイト列をそのまま書き出す（LE 環境前提）
/// - カラーマスク: R=0x00FF0000, G=0x0000FF00, B=0x000000FF
pub fn write_bmp<P: AsRef<Path>>(
    path: P,
    width: u32,
    height: u32,
    stride: usize,
    data: &[u8],
) -> std::io::Result<()> {
    let mut file = File::create(path)?;

    let bpp: u16 = 32;
    let dib_header_size: u32 = 40;
    let masks_size: u32 = 12; // 3 x u32 カラーマスク
    let pixel_offset: u32 = 14 + dib_header_size + masks_size;
    let pixel_data_size = stride as u32 * height;
    let file_size = pixel_offset + pixel_data_size;

    // BITMAPFILEHEADER (14 bytes)
    file.write_all(b"BM")?;
    file.write_all(&file_size.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?; // reserved1
    file.write_all(&0u16.to_le_bytes())?; // reserved2
    file.write_all(&pixel_offset.to_le_bytes())?;

    // BITMAPINFOHEADER (40 bytes)
    file.write_all(&dib_header_size.to_le_bytes())?;
    file.write_all(&(width as i32).to_le_bytes())?;
    // negative height で top-down
    file.write_all(&(-(height as i32)).to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // planes
    file.write_all(&bpp.to_le_bytes())?;
    file.write_all(&3u32.to_le_bytes())?; // compression = BI_BITFIELDS
    file.write_all(&pixel_data_size.to_le_bytes())?;
    file.write_all(&2835u32.to_le_bytes())?; // x pixels per meter (72 DPI)
    file.write_all(&2835u32.to_le_bytes())?; // y pixels per meter
    file.write_all(&0u32.to_le_bytes())?; // colors used
    file.write_all(&0u32.to_le_bytes())?; // important colors

    // カラーマスク (BI_BITFIELDS, 12 bytes)
    file.write_all(&0x00FF_0000u32.to_le_bytes())?; // R
    file.write_all(&0x0000_FF00u32.to_le_bytes())?; // G
    file.write_all(&0x0000_00FFu32.to_le_bytes())?; // B

    // ピクセルデータ
    file.write_all(data)?;

    Ok(())
}
