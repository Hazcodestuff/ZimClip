//! Choose an image and turn it into the small icon bitmap ZimClip stores. Nothing is
//! written here: the caller keeps the pixels in memory until setup finishes with Done.
use windows::{
    core::PCWSTR,
    Win32::{Foundation::GENERIC_READ, Graphics::Imaging::*, System::Com::*},
};
use windows_sys::Win32::{Foundation::HWND, UI::Controls::Dialogs::*};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// File picker. Ok(None) when the user cancels.
unsafe fn choose(owner: HWND) -> Option<String> {
    let mut file = [0u16; 520];
    let filter: Vec<u16> = "Images\0*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.ico\0\0".encode_utf16().collect();
    let mut o: OPENFILENAMEW = std::mem::zeroed();
    o.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    o.hwndOwner = owner;
    o.lpstrFilter = filter.as_ptr();
    o.lpstrFile = file.as_mut_ptr();
    o.nMaxFile = file.len() as u32;
    o.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER;
    if GetOpenFileNameW(&mut o) == 0 {
        return None;
    }
    let n = file.iter().position(|c| *c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&file[..n]))
}

/// Pick and decode an image: size-capped file, dimension-capped, center-cropped to a square
/// and scaled to UPLOAD_SIDE. Returns premultiplied BGRA.
pub unsafe fn pick(owner: HWND) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = choose(owner) else { return Ok(None) };
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len == 0 || len > crate::identity::MAX_FILE {
        return Err("Use an image under 8 MB.".into());
    }
    decode(&path).map(Some)
}

unsafe fn decode(path: &str) -> Result<Vec<u8>, String> {
    let e = |x: windows::core::Error| x.message();
    // Already-initialized COM on this thread is fine.
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let side = crate::identity::UPLOAD_SIDE as u32;
    let factory: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(e)?;
    let w = wide(path);
    let decoder = factory
        .CreateDecoderFromFilename(PCWSTR(w.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
        .map_err(|_| "That file is not a readable image.".to_string())?;
    let frame = decoder.GetFrame(0).map_err(e)?;
    let (mut iw, mut ih) = (0u32, 0u32);
    frame.GetSize(&mut iw, &mut ih).map_err(e)?;
    let (x, y, s) = crate::identity::crop_square(iw, ih).ok_or("Use an image of at most 16 megapixels (16 to 8192 px per side).")?;
    let clipper = factory.CreateBitmapClipper().map_err(e)?;
    clipper
        .Initialize(&frame, &WICRect { X: x as i32, Y: y as i32, Width: s as i32, Height: s as i32 })
        .map_err(e)?;
    let scaler = factory.CreateBitmapScaler().map_err(e)?;
    scaler.Initialize(&clipper, side, side, WICBitmapInterpolationModeFant).map_err(e)?;
    let conv = factory.CreateFormatConverter().map_err(e)?;
    conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
        .map_err(e)?;
    let mut px = vec![0u8; crate::identity::UPLOAD_BYTES];
    conv.CopyPixels(std::ptr::null(), side * 4, &mut px).map_err(e)?;
    Ok(px)
}
