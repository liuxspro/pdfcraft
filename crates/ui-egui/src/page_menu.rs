//! Local page-context-menu extensions.
//!
//! The page menu is assembled in `canvas::document_area`; this module is the one place where
//! local-only page items are registered. Adding an item here should not touch `comments.rs`
//! or `zoom_snap.rs`, so a rebase onto upstream only has to keep the call site in `canvas.rs`.

use pdfcraft_engine::DocId;
use pdfcraft_render::{RenderConfig, RenderRequest, RequestKind};

use crate::PdfCraftApp;

/// The resolution of Copy Page as Image, in dots per inch: the Export pages default.
const COPY_PAGE_DPI: f64 = 150.0;

/// A page-menu item's deferred action. It runs after the canvas releases its `DocView`
/// borrow, so it may read or edit the app.
pub(crate) type Action = Box<dyn FnOnce(&mut PdfCraftApp)>;

/// Add the local page items. Called only when no comment is selected and the right-click
/// landed on a page.
pub(crate) fn items(ui: &mut egui::Ui, id: DocId, page: usize) -> Option<Action> {
    let mut action: Option<Action> = None;
    if ui.button("复制页面为图像").clicked() {
        action = Some(Box::new(move |app| copy_page_as_image(app, id, page)));
        ui.close();
    }
    ui.separator();
    action
}

/// Copy Page as Image: render the whole page at [`COPY_PAGE_DPI`] and put it on the
/// clipboard. The renderer's size caps apply to huge pages, so the reported size is the
/// actual one. Hidden layers and comments stay hidden, and reading colours are left out:
/// an exported image keeps the document's own colours.
fn copy_page_as_image(app: &mut PdfCraftApp, id: DocId, page: usize) {
    if !app.may_copy(id) {
        return;
    }
    match render_page(app, id, page) {
        Ok((w, h)) => app.notify_fmt("The image has been copied ({w} × {h} pixels)", &[("w", &w.to_string()), ("h", &h.to_string())]),
        Err(e) => app.notify_fmt("Couldn't copy the image: {e}", &[("e", &e.to_string())]),
    }
}

fn render_page(app: &mut PdfCraftApp, id: DocId, page: usize) -> Result<(u32, u32), String> {
    let doc = app.session.get(id).ok_or("the document is not open")?;
    let info = doc.info.pages.get(page).ok_or("there is no such page")?;
    if !(info.width.is_finite() && info.width > 0.0 && info.height.is_finite() && info.height > 0.0) {
        return Err("the page has no drawable area".into());
    }
    let config = RenderConfig {
        password: doc.password.as_deref().map(std::sync::Arc::from),
        layers: std::sync::Arc::new(doc.info.layers.iter().map(|l| (l.id.0 as i32, l.id.1 as i32, l.visible)).collect()),
        hide_comments: doc.comments_hidden(),
        ..RenderConfig::default()
    };
    // `display` is the current view: unsaved edits and the appearances drawn only for
    // comments that have none included, as `page_render` uses.
    let mut r = pdfcraft_render::PageRenderer::new(doc.display.clone(), config);
    let out = r.render(RenderRequest { page, kind: RequestKind::Pixels, tile: None, scale: (COPY_PAGE_DPI / 72.0) as f32, tag: 0, preview: false });
    if let Some(e) = out.error {
        return Err(e);
    }
    let (w, h) = (out.width, out.height);
    #[cfg(not(target_arch = "wasm32"))]
    if app.system_clipboard {
        let img = arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(&out.rgba) };
        arboard::Clipboard::new().and_then(|mut c| c.set_image(img)).map_err(|e| e.to_string())?;
    }
    #[cfg(target_arch = "wasm32")]
    if app.system_clipboard {
        return Err("the web version can't put images on the clipboard yet; use Export PDF".to_string());
    }
    app.last_snapshot = Some((w, h, out.rgba));
    Ok((w, h))
}
