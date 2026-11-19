// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod images;
mod platform;
mod settings_file;
mod text;

/// The desktop program.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(window_icon())
            .with_title("Bunch-a-Beads Pattern Designer")
            .with_inner_size([1300.0, 880.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Bunch-a-Beads Pattern Designer",
        options,
        Box::new(|cc| Ok(Box::new(app::BeadApp::new(cc)))),
    )
}

/// The web page: draws the program into the page's canvas (see index.html).
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("no web page");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("index.html needs <canvas id=\"the_canvas_id\">");
        let started = eframe::WebRunner::new()
            .start(canvas, web_options, Box::new(|cc| Ok(Box::new(app::BeadApp::new(cc)))))
            .await;
        // Swap the "Loading…" text for the program, or show what went wrong.
        if let Some(loading) = document.get_element_by_id("loading_text") {
            match started {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_inner_html(&format!(
                    "<p>Sorry, the program could not start in this browser.</p><p><small>{e:?}</small></p>"
                )),
            }
        }
    });
}

/// The program's icon for the window and taskbar (built into the program).
#[cfg(not(target_arch = "wasm32"))]
fn window_icon() -> eframe::egui::IconData {
    let png = include_bytes!("../assets/icon.png");
    match image::load_from_memory(png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            eframe::egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(_) => eframe::egui::IconData::default(),
    }
}
