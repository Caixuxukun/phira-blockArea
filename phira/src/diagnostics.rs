//! Preserve the original iOS panic before an unwind reaches the GLKView C callback.
use futures_util::FutureExt;
use macroquad::prelude::*;
use std::{
    backtrace::Backtrace,
    future::Future,
    io::Write,
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::{Arc, Mutex},
};

static STAGE: Mutex<&str> = Mutex::new("application entry");

pub fn stage(value: &'static str) {
    if cfg!(target_os = "ios") {
        if let Ok(mut stage) = STAGE.lock() {
            *stage = value;
        }
    }
}

fn report_path() -> Option<PathBuf> {
    // HOME is the application sandbox on iOS; Documents is exposed by file sharing.
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents/phira-crash.txt"))
}

fn write_report(path: &Option<PathBuf>, message: &str) -> bool {
    let Some(path) = path else { return false };
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "\n--- {} ---\n{message}", chrono::Utc::now())?;
        file.sync_all()
    })();
    result.is_ok()
}

pub async fn run(future: impl Future<Output = anyhow::Result<()>>) {
    let path = report_path();
    let panic_message = Arc::new(Mutex::new(None));
    let hook_message = Arc::clone(&panic_message);
    let hook_path = path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let stage = STAGE.lock().map(|s| *s).unwrap_or("unknown");
        let message = format!("Stage: {stage}\n{info}\n\n{}", Backtrace::force_capture());
        // Do not touch graphics or application state here: workers can also panic.
        write_report(&hook_path, &message);
        eprintln!("{message}");
        if let Ok(mut slot) = hook_message.lock() {
            *slot = Some(message);
        }
    }));

    // Catch inside the Rust future, before unwinding through an extern C callback.
    // After failure, show a terminal error screen; never resume partially changed state.
    let message = match AssertUnwindSafe(future).catch_unwind().await {
        Ok(Ok(())) => return,
        Ok(Err(error)) => {
            let stage = STAGE.lock().map(|s| *s).unwrap_or("unknown");
            format!("Stage: {stage}\n{error:#}")
        }
        Err(payload) => panic_message.lock().ok().and_then(|slot| slot.clone()).unwrap_or_else(|| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "Rust panic (no message)".to_owned())
        }),
    };
    let saved = write_report(&path, &message);
    eprintln!("{message}");
    // Use the built-in font, independently of Phira's font and scene initialization.
    unsafe { get_internal_gl() }.quad_gl.reset();
    set_default_camera();
    gl_use_default_material();
    loop {
        clear_background(Color::new(0.07, 0.08, 0.1, 1.));
        let size = (screen_height() / 24.).clamp(14., 24.);
        draw_text("Phira stopped after an error", 20., size * 1.5, size, RED);
        let hint = if saved {
            "Report: Files > Phira > phira-crash.txt"
        } else {
            "Could not save the report. Please capture this screen."
        };
        draw_text(hint, 20., size * 3., size * 0.8, WHITE);
        let width = ((screen_width() - 40.) / (size * 0.5)).max(10.) as usize;
        let mut y = size * 4.5;
        for line in message.lines() {
            let chars: Vec<char> = line.chars().collect();
            for part in chars.chunks(width) {
                draw_text(&part.iter().collect::<String>(), 20., y, size * 0.8, WHITE);
                y += size;
                if y > screen_height() - size {
                    break;
                }
            }
            if y > screen_height() - size {
                break;
            }
        }
        next_frame().await;
    }
}
